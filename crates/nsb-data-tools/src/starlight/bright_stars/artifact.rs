//! Checksum-pinned bright-star supplement artifact (external opt-in).

use super::policy::{BrightStarPopulationPolicy, BrightStarPrecedencePolicy, SupplementClass};
use crate::platform::checksum_io;
use crate::starlight::healpix::galactic_nested_pixel_from_icrs_position;
use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

/// Artifact schema version.
pub const BRIGHT_STAR_ARTIFACT_SCHEMA_VERSION: u32 = 1;
/// Stable model id for runtime provenance.
pub const BRIGHT_STAR_MODEL_ID: &str = "starlight-bright-stars-v1";

/// One admitted or classified supplement source.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BrightStarSourceRecord {
    pub source_id: String,
    pub origin_catalogue: String,
    pub class: SupplementClass,
    pub gaia_source_id: Option<u64>,
    pub ra_deg_j2016: f64,
    pub dec_deg_j2016: f64,
    pub flux_336_650_ph_m2_s: f64,
    pub statistical_uncertainty_ph_m2_s: f64,
    pub systematic_uncertainty_ph_m2_s: f64,
    pub spectral_route: String,
    pub classification_reason: String,
}

/// Sparse HEALPix pixel contribution from the supplement.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BrightStarPixel {
    pub pixel: u64,
    pub flux_ph_m2_s: f64,
    pub statistical_uncertainty_ph_m2_s: f64,
    pub systematic_uncertainty_ph_m2_s: f64,
    pub admitted_sources: u64,
}

/// Versioned bright-star supplement artifact.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BrightStarArtifact {
    pub schema_version: u32,
    pub model_id: String,
    pub nside: u32,
    pub ordering: String,
    pub population_policy: BrightStarPopulationPolicy,
    pub precedence_policy: BrightStarPrecedencePolicy,
    pub input_checksums: BTreeMap<String, String>,
    pub build_commit: String,
    pub sources: Vec<BrightStarSourceRecord>,
    pub pixels: Vec<BrightStarPixel>,
    pub counts: BrightStarCounts,
    pub scientifically_validated: bool,
    pub redistribution_embedded: bool,
    pub notes: Vec<String>,
}

/// Population accounting for provenance.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BrightStarCounts {
    pub input_stars: u64,
    pub supplement_only: u64,
    pub matched_and_replaces_primary: u64,
    pub matched_and_rejected_as_duplicate: u64,
    pub ambiguous: u64,
    pub final_admitted: u64,
}

impl BrightStarArtifact {
    /// Build a deterministic artifact from classified source records.
    pub fn from_sources(
        nside: u32,
        build_commit: &str,
        input_checksums: BTreeMap<String, String>,
        sources: Vec<BrightStarSourceRecord>,
        population_policy: BrightStarPopulationPolicy,
        precedence_policy: BrightStarPrecedencePolicy,
    ) -> Result<Self> {
        if nside == 0 || !nside.is_power_of_two() {
            bail!("nside must be a positive power of two");
        }
        let mut counts = BrightStarCounts {
            input_stars: sources.len() as u64,
            supplement_only: 0,
            matched_and_replaces_primary: 0,
            matched_and_rejected_as_duplicate: 0,
            ambiguous: 0,
            final_admitted: 0,
        };
        let mut pixel_acc: BTreeMap<u64, (f64, f64, f64, u64)> = BTreeMap::new();
        // Stable source order for determinism.
        let mut sources = sources;
        sources.sort_by(|a, b| a.source_id.cmp(&b.source_id));

        for source in &sources {
            match source.class {
                SupplementClass::SupplementOnly => counts.supplement_only += 1,
                SupplementClass::MatchedAndReplacesPrimary => {
                    counts.matched_and_replaces_primary += 1
                }
                SupplementClass::MatchedAndRejectedAsDuplicate => {
                    counts.matched_and_rejected_as_duplicate += 1
                }
                SupplementClass::AmbiguousManualReview => counts.ambiguous += 1,
            }
            let admit = matches!(
                source.class,
                SupplementClass::SupplementOnly | SupplementClass::MatchedAndReplacesPrimary
            );
            if !admit {
                continue;
            }
            if !(source.flux_336_650_ph_m2_s.is_finite() && source.flux_336_650_ph_m2_s > 0.0) {
                bail!(
                    "admitted bright-star source {} requires positive finite flux",
                    source.source_id
                );
            }
            let pixel = galactic_nested_pixel_from_icrs_position(
                source.ra_deg_j2016,
                source.dec_deg_j2016,
                nside,
            )?;
            let entry = pixel_acc
                .entry(u64::from(pixel))
                .or_insert((0.0, 0.0, 0.0, 0));
            entry.0 += source.flux_336_650_ph_m2_s;
            entry.1 += source.statistical_uncertainty_ph_m2_s.powi(2);
            entry.2 += source.systematic_uncertainty_ph_m2_s; // correlated catalogue ZP style
            entry.3 += 1;
            counts.final_admitted += 1;
        }

        let pixels = pixel_acc
            .into_iter()
            .map(
                |(pixel, (flux, stat_var, sys_corr, admitted_sources))| BrightStarPixel {
                    pixel,
                    flux_ph_m2_s: flux,
                    statistical_uncertainty_ph_m2_s: stat_var.sqrt(),
                    systematic_uncertainty_ph_m2_s: sys_corr,
                    admitted_sources,
                },
            )
            .collect();

        Ok(Self {
            schema_version: BRIGHT_STAR_ARTIFACT_SCHEMA_VERSION,
            model_id: BRIGHT_STAR_MODEL_ID.to_string(),
            nside,
            ordering: "nested".to_string(),
            population_policy,
            precedence_policy,
            input_checksums,
            build_commit: build_commit.to_string(),
            sources,
            pixels,
            counts,
            scientifically_validated: false,
            redistribution_embedded: false,
            notes: vec![
                "External opt-in supplement; catalogue bytes are not embedded in NSB.".into(),
                "scientifically_validated remains false until independent validation + human gates."
                    .into(),
            ],
        })
    }

    /// Fail-closed load: schema, model id, and SHA-256 pin must match.
    pub fn validate_identity(&self, expected_sha256: &str, path: &Path) -> Result<()> {
        if self.schema_version != BRIGHT_STAR_ARTIFACT_SCHEMA_VERSION {
            bail!(
                "bright-star artifact schema_version {} != {}",
                self.schema_version,
                BRIGHT_STAR_ARTIFACT_SCHEMA_VERSION
            );
        }
        if self.model_id != BRIGHT_STAR_MODEL_ID {
            bail!(
                "bright-star model_id {} != {}",
                self.model_id,
                BRIGHT_STAR_MODEL_ID
            );
        }
        if self.scientifically_validated {
            bail!("bright-star artifact must not claim scientifically_validated yet");
        }
        if self.redistribution_embedded {
            bail!("bright-star artifact must not claim repository embedding without #103 approval");
        }
        let actual =
            checksum_io::sha256_file(path).with_context(|| format!("hash {}", path.display()))?;
        if !expected_sha256.chars().all(|c| c.is_ascii_hexdigit()) || expected_sha256.len() != 64 {
            bail!("expected bright-star sha256 must be 64 hex chars");
        }
        if actual != expected_sha256 {
            bail!(
                "bright-star artifact sha256 mismatch: {} != {} ({})",
                actual,
                expected_sha256,
                path.display()
            );
        }
        Ok(())
    }
}

/// Load and verify a checksum-pinned bright-star artifact.
pub fn load_bright_star_artifact(path: &Path, expected_sha256: &str) -> Result<BrightStarArtifact> {
    let bytes = fs::read(path).with_context(|| format!("read {}", path.display()))?;
    let artifact: BrightStarArtifact = serde_json::from_slice(&bytes)
        .with_context(|| format!("parse bright-star artifact {}", path.display()))?;
    artifact.validate_identity(expected_sha256, path)?;
    Ok(artifact)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::starlight::bright_stars::policy::{
        BrightStarPopulationPolicy, BrightStarPrecedencePolicy, SupplementClass,
    };
    use std::io::Write;
    use tempfile::NamedTempFile;

    fn sample_source(id: &str, class: SupplementClass, flux: f64) -> BrightStarSourceRecord {
        BrightStarSourceRecord {
            source_id: id.into(),
            origin_catalogue: "fixture-hip2".into(),
            class,
            gaia_source_id: None,
            ra_deg_j2016: 10.0,
            dec_deg_j2016: 20.0,
            flux_336_650_ph_m2_s: flux,
            statistical_uncertainty_ph_m2_s: flux * 0.01,
            systematic_uncertainty_ph_m2_s: flux * 0.05,
            spectral_route: "fixture-template".into(),
            classification_reason: "test".into(),
        }
    }

    #[test]
    fn artifact_bins_admitted_sources_only() {
        let sources = vec![
            sample_source("b", SupplementClass::SupplementOnly, 10.0),
            sample_source("a", SupplementClass::MatchedAndRejectedAsDuplicate, 99.0),
            sample_source("c", SupplementClass::MatchedAndReplacesPrimary, 5.0),
            sample_source("d", SupplementClass::AmbiguousManualReview, 7.0),
        ];
        let art = BrightStarArtifact::from_sources(
            1,
            "deadbeef",
            BTreeMap::new(),
            sources,
            BrightStarPopulationPolicy::v1(),
            BrightStarPrecedencePolicy::v1(),
        )
        .unwrap();
        assert_eq!(art.counts.final_admitted, 2);
        assert_eq!(art.counts.matched_and_rejected_as_duplicate, 1);
        assert_eq!(art.pixels.len(), 1);
        assert!((art.pixels[0].flux_ph_m2_s - 15.0).abs() < 1e-12);
        // Deterministic source ordering by id.
        assert_eq!(art.sources[0].source_id, "a");
    }

    #[test]
    fn load_rejects_wrong_checksum() {
        let art = BrightStarArtifact::from_sources(
            1,
            "c",
            BTreeMap::new(),
            vec![sample_source("s", SupplementClass::SupplementOnly, 1.0)],
            BrightStarPopulationPolicy::v1(),
            BrightStarPrecedencePolicy::v1(),
        )
        .unwrap();
        let mut tmp = NamedTempFile::new().unwrap();
        serde_json::to_writer_pretty(&mut tmp, &art).unwrap();
        tmp.flush().unwrap();
        let err = load_bright_star_artifact(tmp.path(), &"0".repeat(64)).unwrap_err();
        assert!(err.to_string().contains("sha256 mismatch"));
    }

    #[test]
    fn load_accepts_matching_checksum() {
        let art = BrightStarArtifact::from_sources(
            1,
            "c",
            BTreeMap::new(),
            vec![sample_source("s", SupplementClass::SupplementOnly, 1.0)],
            BrightStarPopulationPolicy::v1(),
            BrightStarPrecedencePolicy::v1(),
        )
        .unwrap();
        let mut tmp = NamedTempFile::new().unwrap();
        let bytes = serde_json::to_vec_pretty(&art).unwrap();
        tmp.write_all(&bytes).unwrap();
        tmp.flush().unwrap();
        let sha = checksum_io::sha256_bytes(&bytes);
        let loaded = load_bright_star_artifact(tmp.path(), &sha).unwrap();
        assert_eq!(loaded.model_id, BRIGHT_STAR_MODEL_ID);
        assert!(!loaded.scientifically_validated);
    }

    #[test]
    fn ordering_changes_are_byte_identical() {
        let s1 = vec![
            sample_source("z", SupplementClass::SupplementOnly, 3.0),
            sample_source("a", SupplementClass::SupplementOnly, 2.0),
        ];
        let s2 = vec![
            sample_source("a", SupplementClass::SupplementOnly, 2.0),
            sample_source("z", SupplementClass::SupplementOnly, 3.0),
        ];
        let a1 = BrightStarArtifact::from_sources(
            1,
            "c",
            BTreeMap::new(),
            s1,
            BrightStarPopulationPolicy::v1(),
            BrightStarPrecedencePolicy::v1(),
        )
        .unwrap();
        let a2 = BrightStarArtifact::from_sources(
            1,
            "c",
            BTreeMap::new(),
            s2,
            BrightStarPopulationPolicy::v1(),
            BrightStarPrecedencePolicy::v1(),
        )
        .unwrap();
        let b1 = serde_json::to_vec(&a1).unwrap();
        let b2 = serde_json::to_vec(&a2).unwrap();
        assert_eq!(b1, b2);
    }
}
