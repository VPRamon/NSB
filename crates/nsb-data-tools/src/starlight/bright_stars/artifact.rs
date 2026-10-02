//! Fail-closed, checksum-pinned bright-star supplement artifact.

use super::policy::{BrightStarPopulationPolicy, BrightStarPrecedencePolicy, SupplementClass};
use crate::platform::checksum_io;
use crate::starlight::healpix::galactic_nested_pixel_from_icrs_position;
use crate::starlight::map::accumulator::StableSum;
use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::Path;

pub const BRIGHT_STAR_ARTIFACT_SCHEMA_VERSION: u32 = 1;
pub const BRIGHT_STAR_MODEL_ID: &str = "starlight-bright-stars-v1";
pub const BRIGHT_STAR_PRODUCT_BAND_ID: &str = "measured-336-650";

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BrightStarInputRole {
    Hipparcos2,
    Tycho2,
    HipGaiaCrossmatch,
    GaiaDr3QualityExtract,
    SpectralTypeCatalogue,
    SpectralTemplateLibrary,
    PhotometricResponseCurve,
    PhotometricZeroPoint,
    BuildConfig,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BrightStarInputProvenance {
    pub role: BrightStarInputRole,
    pub source_id: String,
    pub release: String,
    pub sha256: String,
    pub retrieval_url: String,
    pub license_or_terms_url: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CorrelatedUncertainty {
    pub correlation_group_id: String,
    pub uncertainty_ph_m2_s: f64,
}

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
    pub systematic_independent_uncertainty_ph_m2_s: f64,
    pub systematic_catalogue_correlated: Vec<CorrelatedUncertainty>,
    pub spectral_route: String,
    pub classification_reason: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BrightStarPixel {
    pub pixel: u64,
    pub flux_ph_m2_s: f64,
    pub statistical_uncertainty_ph_m2_s: f64,
    pub systematic_independent_uncertainty_ph_m2_s: f64,
    pub systematic_catalogue_correlated: BTreeMap<String, f64>,
    pub admitted_sources: u64,
}

/// Runtime / merge-report identity for a checksum-verified bright-star supplement.
///
/// Originates from the verified [`BrightStarArtifact`] pin and must survive
/// synthetic-shard construction and deterministic merge so a published map can
/// be interpreted independently of external run manifests.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BrightStarSupplementProvenance {
    pub artifact_sha256: String,
    pub model_id: String,
    pub population_policy_id: String,
    pub precedence_policy_id: String,
    pub build_commit: String,
}

impl BrightStarSupplementProvenance {
    pub fn validate(&self) -> Result<()> {
        validate_sha256(&self.artifact_sha256, "bright-star artifact_sha256")?;
        if self.model_id != BRIGHT_STAR_MODEL_ID {
            bail!("bright-star supplement provenance model_id must be {BRIGHT_STAR_MODEL_ID}");
        }
        if self.population_policy_id != super::policy::POPULATION_POLICY_ID_V1 {
            bail!("bright-star supplement provenance population_policy_id is unknown");
        }
        if self.precedence_policy_id != super::policy::PRECEDENCE_POLICY_ID_V1 {
            bail!("bright-star supplement provenance precedence_policy_id is unknown");
        }
        if !is_full_git_sha(&self.build_commit) {
            bail!(
                "bright-star supplement provenance build_commit must be a full 40-character lowercase Git SHA"
            );
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BrightStarArtifact {
    pub schema_version: u32,
    pub model_id: String,
    pub product_band: String,
    pub nside: u32,
    pub ordering: String,
    pub population_policy: BrightStarPopulationPolicy,
    pub precedence_policy: BrightStarPrecedencePolicy,
    pub inputs: Vec<BrightStarInputProvenance>,
    pub build_commit: String,
    pub sources: Vec<BrightStarSourceRecord>,
    pub pixels: Vec<BrightStarPixel>,
    pub counts: BrightStarCounts,
    pub scientifically_validated: bool,
    pub redistribution_embedded: bool,
    pub notes: Vec<String>,
}

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

#[derive(Default)]
struct PixelBuild {
    flux: StableSum,
    stat_var: StableSum,
    independent_sys_var: StableSum,
    correlated: BTreeMap<String, StableSum>,
    sources: u64,
}

impl BrightStarArtifact {
    pub fn from_sources(
        nside: u32,
        build_commit: &str,
        inputs: Vec<BrightStarInputProvenance>,
        mut sources: Vec<BrightStarSourceRecord>,
        population_policy: BrightStarPopulationPolicy,
        precedence_policy: BrightStarPrecedencePolicy,
    ) -> Result<Self> {
        sources.sort_by(|a, b| a.source_id.cmp(&b.source_id));
        let artifact = Self {
            schema_version: BRIGHT_STAR_ARTIFACT_SCHEMA_VERSION,
            model_id: BRIGHT_STAR_MODEL_ID.into(),
            product_band: BRIGHT_STAR_PRODUCT_BAND_ID.into(),
            nside,
            ordering: "nested".into(),
            population_policy,
            precedence_policy,
            inputs,
            build_commit: build_commit.into(),
            counts: recompute_counts(&sources),
            pixels: rebuild_pixels(nside, &sources)?,
            sources,
            scientifically_validated: false,
            redistribution_embedded: false,
            notes: vec![
                "External opt-in supplement; catalogue bytes are not embedded in NSB.".into(),
                "Measured 336-650 nm only; use with 300-650 nm products is forbidden.".into(),
            ],
        };
        artifact.validate()?;
        Ok(artifact)
    }

    pub fn validate(&self) -> Result<()> {
        if self.schema_version != BRIGHT_STAR_ARTIFACT_SCHEMA_VERSION {
            bail!("unknown bright-star schema_version {}", self.schema_version);
        }
        if self.model_id != BRIGHT_STAR_MODEL_ID {
            bail!("unknown bright-star model_id {}", self.model_id);
        }
        if self.product_band != BRIGHT_STAR_PRODUCT_BAND_ID {
            bail!("unsupported bright-star product_band {}", self.product_band);
        }
        if self.ordering != "nested" {
            bail!("bright-star ordering must be nested");
        }
        crate::starlight::config::validate_canonical_nside(self.nside)?;
        self.population_policy.validate()?;
        self.precedence_policy.validate()?;
        if !is_full_git_sha(&self.build_commit) {
            bail!("bright-star build_commit must be a full 40-character lowercase Git SHA");
        }
        if self.scientifically_validated || self.redistribution_embedded {
            bail!("experimental bright-star artifact cannot claim approval or embedding");
        }
        validate_inputs(&self.inputs)?;
        let mut source_ids = BTreeSet::new();
        let mut replacement_ids = BTreeSet::new();
        for source in &self.sources {
            if source.source_id.trim().is_empty() || !source_ids.insert(source.source_id.as_str()) {
                bail!(
                    "bright-star source_id must be non-empty and unique: {}",
                    source.source_id
                );
            }
            validate_source(source)?;
            if source.class == SupplementClass::MatchedAndReplacesPrimary {
                let gaia_id = source
                    .gaia_source_id
                    .expect("replacement identity validated");
                if !replacement_ids.insert(gaia_id) {
                    bail!("two supplement records replace Gaia source_id {gaia_id}");
                }
            }
        }
        if self
            .sources
            .windows(2)
            .any(|pair| pair[0].source_id >= pair[1].source_id)
        {
            bail!("bright-star sources must be sorted by unique source_id");
        }
        if self.counts != recompute_counts(&self.sources) {
            bail!("bright-star serialized counts do not match source records");
        }
        if self.pixels != rebuild_pixels(self.nside, &self.sources)? {
            bail!("bright-star serialized pixels do not match source reconstruction");
        }
        validate_conservation(self.nside, &self.sources, &self.pixels)?;
        Ok(())
    }

    pub fn suppressed_gaia_source_ids(&self) -> Result<BTreeSet<u64>> {
        self.validate()?;
        Ok(self
            .sources
            .iter()
            .filter(|source| source.class == SupplementClass::MatchedAndReplacesPrimary)
            .filter_map(|source| source.gaia_source_id)
            .collect())
    }

    /// Build runtime provenance from this artifact and its verified SHA-256 pin.
    pub fn supplement_provenance(
        &self,
        verified_artifact_sha256: &str,
    ) -> Result<BrightStarSupplementProvenance> {
        self.validate()?;
        validate_sha256(
            verified_artifact_sha256,
            "verified bright-star artifact sha256",
        )?;
        let provenance = BrightStarSupplementProvenance {
            artifact_sha256: verified_artifact_sha256.to_owned(),
            model_id: self.model_id.clone(),
            population_policy_id: self.population_policy.policy_id.clone(),
            precedence_policy_id: self.precedence_policy.policy_id.clone(),
            build_commit: self.build_commit.clone(),
        };
        provenance.validate()?;
        Ok(provenance)
    }

    pub fn to_json_pretty(&self) -> Result<Vec<u8>> {
        self.validate()?;
        Ok(serde_json::to_vec_pretty(self)?)
    }

    fn validate_identity(&self, expected_sha256: &str, path: &Path) -> Result<()> {
        self.validate()?;
        validate_sha256(expected_sha256, "expected bright-star sha256")?;
        let actual =
            checksum_io::sha256_file(path).with_context(|| format!("hash {}", path.display()))?;
        if actual != expected_sha256 {
            bail!(
                "bright-star artifact sha256 mismatch: {actual} != {expected_sha256} ({})",
                path.display()
            );
        }
        Ok(())
    }
}

/// Full 40-character lowercase Git object name used for reproducible provenance.
pub(crate) fn is_full_git_sha(value: &str) -> bool {
    value.len() == 40
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn validate_inputs(inputs: &[BrightStarInputProvenance]) -> Result<()> {
    let mut identities = BTreeSet::new();
    for input in inputs {
        if input.source_id.trim().is_empty() || input.release.trim().is_empty() {
            bail!("bright-star input source_id and release must not be empty");
        }
        validate_sha256(&input.sha256, "bright-star input sha256")?;
        if !input.retrieval_url.starts_with("https://")
            || !input.license_or_terms_url.starts_with("https://")
        {
            bail!("bright-star input retrieval and terms URLs must use HTTPS");
        }
        if !identities.insert((input.role, input.source_id.as_str())) {
            bail!("duplicate bright-star provenance role/source entry");
        }
    }
    Ok(())
}

fn validate_source(source: &BrightStarSourceRecord) -> Result<()> {
    if source.origin_catalogue.trim().is_empty()
        || source.spectral_route.trim().is_empty()
        || source.classification_reason.trim().is_empty()
    {
        bail!("bright-star source identity, route, and reason must not be empty");
    }
    if !source.ra_deg_j2016.is_finite()
        || !(0.0..360.0).contains(&source.ra_deg_j2016)
        || !source.dec_deg_j2016.is_finite()
        || !(-90.0..=90.0).contains(&source.dec_deg_j2016)
    {
        bail!(
            "bright-star source {} has invalid J2016 coordinates",
            source.source_id
        );
    }
    let admitted = matches!(
        source.class,
        SupplementClass::SupplementOnly | SupplementClass::MatchedAndReplacesPrimary
    );
    match source.class {
        SupplementClass::SupplementOnly if source.gaia_source_id.is_some() => bail!(
            "supplement-only source {} must not have gaia_source_id",
            source.source_id
        ),
        SupplementClass::MatchedAndReplacesPrimary
        | SupplementClass::MatchedAndRejectedAsDuplicate
            if source.gaia_source_id.is_none() =>
        {
            bail!(
                "matched source {} requires gaia_source_id",
                source.source_id
            )
        }
        SupplementClass::AmbiguousManualReview if source.gaia_source_id.is_some() => bail!(
            "ambiguous source {} must not select a Gaia identity",
            source.source_id
        ),
        _ => {}
    }
    let numeric = [
        source.flux_336_650_ph_m2_s,
        source.statistical_uncertainty_ph_m2_s,
        source.systematic_independent_uncertainty_ph_m2_s,
    ];
    if numeric
        .iter()
        .any(|value| !value.is_finite() || *value < 0.0)
        || (admitted && source.flux_336_650_ph_m2_s <= 0.0)
    {
        bail!(
            "bright-star source {} has invalid flux or uncertainty",
            source.source_id
        );
    }
    let mut groups = BTreeSet::new();
    for term in &source.systematic_catalogue_correlated {
        if term.correlation_group_id.trim().is_empty()
            || !groups.insert(term.correlation_group_id.as_str())
            || !term.uncertainty_ph_m2_s.is_finite()
            || term.uncertainty_ph_m2_s < 0.0
        {
            bail!(
                "bright-star source {} has invalid correlated uncertainty",
                source.source_id
            );
        }
    }
    Ok(())
}

fn recompute_counts(sources: &[BrightStarSourceRecord]) -> BrightStarCounts {
    let mut counts = BrightStarCounts {
        input_stars: sources.len() as u64,
        supplement_only: 0,
        matched_and_replaces_primary: 0,
        matched_and_rejected_as_duplicate: 0,
        ambiguous: 0,
        final_admitted: 0,
    };
    for source in sources {
        match source.class {
            SupplementClass::SupplementOnly => counts.supplement_only += 1,
            SupplementClass::MatchedAndReplacesPrimary => counts.matched_and_replaces_primary += 1,
            SupplementClass::MatchedAndRejectedAsDuplicate => {
                counts.matched_and_rejected_as_duplicate += 1
            }
            SupplementClass::AmbiguousManualReview => counts.ambiguous += 1,
        }
        if matches!(
            source.class,
            SupplementClass::SupplementOnly | SupplementClass::MatchedAndReplacesPrimary
        ) {
            counts.final_admitted += 1;
        }
    }
    counts
}

fn rebuild_pixels(nside: u32, sources: &[BrightStarSourceRecord]) -> Result<Vec<BrightStarPixel>> {
    crate::starlight::config::validate_canonical_nside(nside)?;
    let mut pixels: BTreeMap<u64, PixelBuild> = BTreeMap::new();
    for source in sources {
        validate_source(source)?;
        if !matches!(
            source.class,
            SupplementClass::SupplementOnly | SupplementClass::MatchedAndReplacesPrimary
        ) {
            continue;
        }
        let pixel = u64::from(galactic_nested_pixel_from_icrs_position(
            source.ra_deg_j2016,
            source.dec_deg_j2016,
            nside,
        )?);
        let entry = pixels.entry(pixel).or_default();
        entry.flux.add(source.flux_336_650_ph_m2_s)?;
        entry
            .stat_var
            .add(source.statistical_uncertainty_ph_m2_s.powi(2))?;
        entry
            .independent_sys_var
            .add(source.systematic_independent_uncertainty_ph_m2_s.powi(2))?;
        for term in &source.systematic_catalogue_correlated {
            entry
                .correlated
                .entry(term.correlation_group_id.clone())
                .or_default()
                .add(term.uncertainty_ph_m2_s)?;
        }
        entry.sources += 1;
    }
    Ok(pixels
        .into_iter()
        .map(|(pixel, p)| BrightStarPixel {
            pixel,
            flux_ph_m2_s: p.flux.value(),
            statistical_uncertainty_ph_m2_s: p.stat_var.value().sqrt(),
            systematic_independent_uncertainty_ph_m2_s: p.independent_sys_var.value().sqrt(),
            systematic_catalogue_correlated: p
                .correlated
                .into_iter()
                .map(|(group, sum)| (group, sum.value()))
                .collect(),
            admitted_sources: p.sources,
        })
        .collect())
}

fn validate_conservation(
    nside: u32,
    sources: &[BrightStarSourceRecord],
    pixels: &[BrightStarPixel],
) -> Result<()> {
    let admitted = |s: &&BrightStarSourceRecord| {
        matches!(
            s.class,
            SupplementClass::SupplementOnly | SupplementClass::MatchedAndReplacesPrimary
        )
    };
    let mut source_flux = StableSum::default();
    for source in sources.iter().filter(admitted) {
        source_flux.add(source.flux_336_650_ph_m2_s)?;
    }
    let source_count = sources.iter().filter(admitted).count() as u64;
    let mut by_pixel: BTreeMap<u64, StableSum> = BTreeMap::new();
    for source in sources.iter().filter(admitted) {
        let pixel = u64::from(galactic_nested_pixel_from_icrs_position(
            source.ra_deg_j2016,
            source.dec_deg_j2016,
            nside,
        )?);
        by_pixel
            .entry(pixel)
            .or_default()
            .add(source.flux_336_650_ph_m2_s)?;
    }
    let mut pixel_flux = StableSum::default();
    for sum in by_pixel.values() {
        pixel_flux.merge(sum)?;
    }
    let pixel_count: u64 = pixels.iter().map(|p| p.admitted_sources).sum();
    if source_flux != pixel_flux || source_count != pixel_count {
        bail!("bright-star source/pixel flux or count conservation failed");
    }
    Ok(())
}

fn validate_sha256(value: &str, label: &str) -> Result<()> {
    if value.len() != 64
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        bail!("{label} must be 64 lowercase hexadecimal characters");
    }
    Ok(())
}

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
    use tempfile::NamedTempFile;

    fn source(
        id: &str,
        class: SupplementClass,
        gaia: Option<u64>,
        flux: f64,
    ) -> BrightStarSourceRecord {
        BrightStarSourceRecord {
            source_id: id.into(),
            origin_catalogue: "hip2".into(),
            class,
            gaia_source_id: gaia,
            ra_deg_j2016: 10.0,
            dec_deg_j2016: 20.0,
            flux_336_650_ph_m2_s: flux,
            statistical_uncertainty_ph_m2_s: flux * 0.01,
            systematic_independent_uncertainty_ph_m2_s: flux * 0.02,
            systematic_catalogue_correlated: vec![CorrelatedUncertainty {
                correlation_group_id: "hip2-zero-point".into(),
                uncertainty_ph_m2_s: flux * 0.03,
            }],
            spectral_route: "fixture".into(),
            classification_reason: "fixture".into(),
        }
    }
    fn fixture_commit() -> String {
        "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".into()
    }

    fn artifact(sources: Vec<BrightStarSourceRecord>) -> Result<BrightStarArtifact> {
        BrightStarArtifact::from_sources(
            1,
            &fixture_commit(),
            Vec::new(),
            sources,
            BrightStarPopulationPolicy::v1(),
            BrightStarPrecedencePolicy::v1(),
        )
    }

    #[test]
    fn admitted_accounting_and_uncertainty_groups() {
        let art = artifact(vec![
            source("a", SupplementClass::SupplementOnly, None, 10.0),
            source(
                "b",
                SupplementClass::MatchedAndReplacesPrimary,
                Some(7),
                5.0,
            ),
            source(
                "c",
                SupplementClass::MatchedAndRejectedAsDuplicate,
                Some(8),
                99.0,
            ),
            source("d", SupplementClass::AmbiguousManualReview, None, 7.0),
        ])
        .unwrap();
        assert_eq!(art.counts.final_admitted, 2);
        assert_eq!(art.pixels[0].flux_ph_m2_s, 15.0);
        assert!(
            (art.pixels[0].systematic_catalogue_correlated["hip2-zero-point"] - 0.45).abs() < 1e-12
        );
        assert_eq!(
            art.suppressed_gaia_source_ids().unwrap(),
            BTreeSet::from([7])
        );
    }

    #[test]
    fn duplicate_id_invariants_fail() {
        assert!(artifact(vec![
            source("a", SupplementClass::SupplementOnly, None, 1.0),
            source("a", SupplementClass::SupplementOnly, None, 2.0)
        ])
        .is_err());
        assert!(artifact(vec![
            source(
                "a",
                SupplementClass::MatchedAndReplacesPrimary,
                Some(9),
                1.0
            ),
            source(
                "b",
                SupplementClass::MatchedAndReplacesPrimary,
                Some(9),
                2.0
            )
        ])
        .is_err());
    }

    #[test]
    fn classification_and_numeric_invariants_fail() {
        assert!(artifact(vec![source(
            "a",
            SupplementClass::SupplementOnly,
            Some(1),
            1.0
        )])
        .is_err());
        assert!(artifact(vec![source(
            "a",
            SupplementClass::MatchedAndReplacesPrimary,
            None,
            1.0
        )])
        .is_err());
        assert!(artifact(vec![source(
            "a",
            SupplementClass::SupplementOnly,
            None,
            f64::NAN
        )])
        .is_err());
    }

    #[test]
    fn tampered_counts_and_pixels_fail() {
        let mut art = artifact(vec![source(
            "a",
            SupplementClass::SupplementOnly,
            None,
            1.0,
        )])
        .unwrap();
        art.counts.final_admitted = 0;
        assert!(art.validate().is_err());
        art.counts.final_admitted = 1;
        art.pixels[0].flux_ph_m2_s = 2.0;
        assert!(art.validate().is_err());
    }

    #[test]
    fn checksum_and_post_deserialization_validation() {
        let art = artifact(vec![source(
            "a",
            SupplementClass::SupplementOnly,
            None,
            1.0,
        )])
        .unwrap();
        let bytes = art.to_json_pretty().unwrap();
        let mut tmp = NamedTempFile::new().unwrap();
        std::io::Write::write_all(&mut tmp, &bytes).unwrap();
        let sha = checksum_io::sha256_bytes(&bytes);
        assert_eq!(load_bright_star_artifact(tmp.path(), &sha).unwrap(), art);
        assert!(load_bright_star_artifact(tmp.path(), &"0".repeat(64)).is_err());
    }

    #[test]
    fn exact_accumulation_is_order_independent_and_serialization_is_stable() {
        let sources = vec![
            source("large", SupplementClass::SupplementOnly, None, 1.0e16),
            source("small-a", SupplementClass::SupplementOnly, None, 1.0),
            source("small-b", SupplementClass::SupplementOnly, None, 1.0),
        ];
        let mut reversed = sources.clone();
        reversed.reverse();
        let first = artifact(sources).unwrap();
        let second = artifact(reversed).unwrap();
        assert_eq!(first, second);
        assert_eq!(first.pixels[0].flux_ph_m2_s, 1.000_000_000_000_000_2e16);
        assert_eq!(
            first.to_json_pretty().unwrap(),
            second.to_json_pretty().unwrap()
        );
        assert_eq!(
            first.to_json_pretty().unwrap(),
            first.to_json_pretty().unwrap()
        );
    }

    #[test]
    fn supplement_provenance_requires_verified_sha_and_full_build_commit() {
        let art = artifact(vec![source(
            "a",
            SupplementClass::MatchedAndReplacesPrimary,
            Some(11),
            3.0,
        )])
        .unwrap();
        let sha = "b".repeat(64);
        let provenance = art.supplement_provenance(&sha).unwrap();
        assert_eq!(provenance.artifact_sha256, sha);
        assert_eq!(provenance.model_id, BRIGHT_STAR_MODEL_ID);
        assert_eq!(
            provenance.population_policy_id,
            BrightStarPopulationPolicy::v1().policy_id
        );
        assert_eq!(
            provenance.precedence_policy_id,
            BrightStarPrecedencePolicy::v1().policy_id
        );
        assert_eq!(provenance.build_commit, fixture_commit());
        provenance.validate().unwrap();

        assert!(art.supplement_provenance("abcd").is_err());
        assert!(art.supplement_provenance(&"B".repeat(64)).is_err());

        let mut short_commit = art.clone();
        short_commit.build_commit = "ceba696".into();
        assert!(short_commit.validate().is_err());
        assert!(short_commit.supplement_provenance(&sha).is_err());

        let mut bad = provenance.clone();
        bad.build_commit = "ceba696".into();
        assert!(bad.validate().is_err());
        bad = provenance.clone();
        bad.model_id = "other".into();
        assert!(bad.validate().is_err());
        bad = provenance;
        bad.artifact_sha256 = "0".repeat(63);
        assert!(bad.validate().is_err());
    }
}
