// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Vallés Puig, Ramon
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
pub const BRIGHT_STAR_COMBINED_ARTIFACT_SCHEMA_VERSION: u32 = 1;
pub const BRIGHT_STAR_MODEL_ID: &str = "starlight-bright-stars-v1";
pub const BRIGHT_STAR_COMBINED_MODEL_ID: &str = "starlight-bright-stars-combined-v1";
pub const BRIGHT_STAR_PRODUCT_BAND_ID: &str = "measured-336-650";
pub const BRIGHT_STAR_COMBINED_PRODUCT_BAND_ID: &str = "combined-300-650";

/// Return true when a bright-star artifact product band may enter the given
/// Starlight production product.
pub fn artifact_compatible_with_product_band(
    artifact_product_band: &str,
    product_band: crate::starlight::config::StarlightProductBand,
) -> bool {
    use crate::starlight::config::StarlightProductBand;
    match product_band {
        StarlightProductBand::Measured336To650 => {
            artifact_product_band == BRIGHT_STAR_PRODUCT_BAND_ID
        }
        StarlightProductBand::Combined300To650 => {
            artifact_product_band == BRIGHT_STAR_COMBINED_PRODUCT_BAND_ID
        }
    }
}

pub fn is_combined_product_band(product_band: &str) -> bool {
    product_band == BRIGHT_STAR_COMBINED_PRODUCT_BAND_ID
}

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
    /// Justified 300--336 nm contribution. Required and strictly positive for
    /// admitted sources in a Combined300To650 artifact; absent for measured-only.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub flux_300_336_ph_m2_s: Option<f64>,
    pub flux_336_650_ph_m2_s: f64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub statistical_uncertainty_300_336_ph_m2_s: Option<f64>,
    pub statistical_uncertainty_ph_m2_s: f64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub systematic_independent_uncertainty_300_336_ph_m2_s: Option<f64>,
    pub systematic_independent_uncertainty_ph_m2_s: f64,
    pub systematic_catalogue_correlated: Vec<CorrelatedUncertainty>,
    pub spectral_route: String,
    /// CK04 Hp-scaled UV completion model id when a 300--336 nm term is present.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub uv_completion_model_id: Option<String>,
    pub classification_reason: String,
}

impl BrightStarSourceRecord {
    pub fn selected_flux_ph_m2_s(&self, combined: bool) -> Result<f64> {
        if !combined {
            return Ok(self.flux_336_650_ph_m2_s);
        }
        let uv = self
            .flux_300_336_ph_m2_s
            .context("combined bright-star source missing 300-336 nm flux")?;
        Ok(uv + self.flux_336_650_ph_m2_s)
    }

    pub fn selected_statistical_uncertainty_ph_m2_s(&self, combined: bool) -> Result<f64> {
        if !combined {
            return Ok(self.statistical_uncertainty_ph_m2_s);
        }
        let uv = self
            .statistical_uncertainty_300_336_ph_m2_s
            .context("combined bright-star source missing 300-336 nm statistical uncertainty")?;
        // The UV and measured terms share the same source-level Hp scale
        // uncertainty. Add them linearly across bands; different sources are
        // still combined in quadrature by the pixel accumulator.
        Ok(uv + self.statistical_uncertainty_ph_m2_s)
    }

    pub fn selected_independent_systematic_ph_m2_s(&self, combined: bool) -> Result<f64> {
        if !combined {
            return Ok(self.systematic_independent_uncertainty_ph_m2_s);
        }
        let uv = self
            .systematic_independent_uncertainty_300_336_ph_m2_s
            .context(
                "combined bright-star source missing 300-336 nm independent systematic uncertainty",
            )?;
        // "independent" describes independence between sources, not between
        // the two bands reconstructed from one shared template scale.
        Ok(uv + self.systematic_independent_uncertainty_ph_m2_s)
    }
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
    pub product_band: String,
    pub uv_completion_model_id: Option<String>,
    pub population_policy_id: String,
    pub precedence_policy_id: String,
    pub build_commit: String,
    pub spectral_route: String,
    pub redistribution_scope: String,
    pub inputs: Vec<BrightStarInputProvenance>,
}

impl BrightStarSupplementProvenance {
    pub fn validate(&self) -> Result<()> {
        validate_sha256(&self.artifact_sha256, "bright-star artifact_sha256")?;
        if self.model_id != BRIGHT_STAR_MODEL_ID && self.model_id != BRIGHT_STAR_COMBINED_MODEL_ID {
            bail!(
                "bright-star supplement provenance model_id must be {BRIGHT_STAR_MODEL_ID} or {BRIGHT_STAR_COMBINED_MODEL_ID}"
            );
        }
        let combined = self.model_id == BRIGHT_STAR_COMBINED_MODEL_ID;
        if (combined && self.product_band != BRIGHT_STAR_COMBINED_PRODUCT_BAND_ID)
            || (!combined && self.product_band != BRIGHT_STAR_PRODUCT_BAND_ID)
            || (combined
                && self.uv_completion_model_id.as_deref()
                    != Some(super::reconstruction::UV_COMPLETION_MODEL_ID_V1))
            || (!combined && self.uv_completion_model_id.is_some())
        {
            bail!("bright-star supplement provenance has incompatible product/model identity");
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
        let expected_route = if combined {
            "Hipparcos/XHIP-selected CK04; Hp-scaled CK04 336-650 nm plus Hp-scaled CK04 300-336 nm completion"
        } else {
            "Hipparcos/XHIP-selected CK04; Hp-scaled CK04 336-650 nm"
        };
        if self.spectral_route != expected_route
            || self.redistribution_scope
                != "derived map only; source catalogue bytes are not embedded"
        {
            bail!("bright-star supplement spectral or redistribution provenance is unknown");
        }
        validate_inputs(&self.inputs)?;
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
        sources: Vec<BrightStarSourceRecord>,
        population_policy: BrightStarPopulationPolicy,
        precedence_policy: BrightStarPrecedencePolicy,
    ) -> Result<Self> {
        Self::from_sources_with_product_band(
            nside,
            build_commit,
            inputs,
            sources,
            population_policy,
            precedence_policy,
            BRIGHT_STAR_PRODUCT_BAND_ID,
        )
    }

    /// Build a bright-star artifact for either measured-only or combined coverage.
    pub fn from_sources_with_product_band(
        nside: u32,
        build_commit: &str,
        inputs: Vec<BrightStarInputProvenance>,
        mut sources: Vec<BrightStarSourceRecord>,
        population_policy: BrightStarPopulationPolicy,
        precedence_policy: BrightStarPrecedencePolicy,
        product_band: &str,
    ) -> Result<Self> {
        sources.sort_by(|a, b| a.source_id.cmp(&b.source_id));
        let combined = is_combined_product_band(product_band);
        let (schema_version, model_id, notes) = if combined {
            (
                BRIGHT_STAR_COMBINED_ARTIFACT_SCHEMA_VERSION,
                BRIGHT_STAR_COMBINED_MODEL_ID,
                vec![
                    "External opt-in supplement; catalogue bytes are not embedded in NSB.".into(),
                    "Combined 300-650 nm: measured 336-650 from Hp-scaled CK04 plus justified 300-336 from the same template scale (ck04-hp-scaled-uv-300-336-v1).".into(),
                    "300-336 nm is never a relabelled copy of 336-650 nm flux.".into(),
                ],
            )
        } else {
            (
                BRIGHT_STAR_ARTIFACT_SCHEMA_VERSION,
                BRIGHT_STAR_MODEL_ID,
                vec![
                    "External opt-in supplement; catalogue bytes are not embedded in NSB.".into(),
                    "Measured 336-650 nm only; use with 300-650 nm products is forbidden.".into(),
                ],
            )
        };
        if combined {
            for source in &mut sources {
                if matches!(
                    source.class,
                    SupplementClass::SupplementOnly | SupplementClass::MatchedAndReplacesPrimary
                ) {
                    let Some(uv) = source
                        .flux_300_336_ph_m2_s
                        .filter(|v| v.is_finite() && *v > 0.0)
                    else {
                        bail!(
                            "combined bright-star admitted source {} lacks justified 300-336 nm flux",
                            source.source_id
                        );
                    };
                    if source.statistical_uncertainty_300_336_ph_m2_s.is_none()
                        || source
                            .systematic_independent_uncertainty_300_336_ph_m2_s
                            .is_none()
                        || source.uv_completion_model_id.as_deref()
                            != Some(super::reconstruction::UV_COMPLETION_MODEL_ID_V1)
                    {
                        bail!(
                            "combined bright-star admitted source {} lacks complete UV provenance",
                            source.source_id
                        );
                    }
                    let _ = uv;
                } else {
                    // Non-admitted rows may omit UV; clear any partial UV payload.
                    source.flux_300_336_ph_m2_s = None;
                    source.statistical_uncertainty_300_336_ph_m2_s = None;
                    source.systematic_independent_uncertainty_300_336_ph_m2_s = None;
                    source.uv_completion_model_id = None;
                }
            }
        } else {
            for source in &mut sources {
                // Measured-only artifacts must not carry UV fields that could be
                // silently reinterpreted as 300-650 nm coverage.
                source.flux_300_336_ph_m2_s = None;
                source.statistical_uncertainty_300_336_ph_m2_s = None;
                source.systematic_independent_uncertainty_300_336_ph_m2_s = None;
                source.uv_completion_model_id = None;
            }
        }
        let artifact = Self {
            schema_version,
            model_id: model_id.into(),
            product_band: product_band.into(),
            nside,
            ordering: "nested".into(),
            population_policy,
            precedence_policy,
            inputs,
            build_commit: build_commit.into(),
            counts: recompute_counts(&sources),
            pixels: rebuild_pixels(nside, &sources, combined)?,
            sources,
            scientifically_validated: false,
            redistribution_embedded: false,
            notes,
        };
        artifact.validate()?;
        Ok(artifact)
    }

    pub fn validate(&self) -> Result<()> {
        let combined = is_combined_product_band(&self.product_band);
        if combined {
            if self.schema_version != BRIGHT_STAR_COMBINED_ARTIFACT_SCHEMA_VERSION {
                bail!("unknown bright-star schema_version {}", self.schema_version);
            }
            if self.model_id != BRIGHT_STAR_COMBINED_MODEL_ID {
                bail!("unknown bright-star model_id {}", self.model_id);
            }
        } else {
            if self.schema_version != BRIGHT_STAR_ARTIFACT_SCHEMA_VERSION {
                bail!("unknown bright-star schema_version {}", self.schema_version);
            }
            if self.model_id != BRIGHT_STAR_MODEL_ID {
                bail!("unknown bright-star model_id {}", self.model_id);
            }
            if self.product_band != BRIGHT_STAR_PRODUCT_BAND_ID {
                bail!("unsupported bright-star product_band {}", self.product_band);
            }
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
            validate_source(source, combined)?;
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
        let rebuilt = rebuild_pixels(self.nside, &self.sources, combined)?;
        if !pixels_match(&self.pixels, &rebuilt)? {
            bail!("bright-star serialized pixels do not match source reconstruction");
        }
        validate_conservation(self.nside, &self.sources, &self.pixels, combined)?;
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
            product_band: self.product_band.clone(),
            uv_completion_model_id: is_combined_product_band(&self.product_band)
                .then(|| super::reconstruction::UV_COMPLETION_MODEL_ID_V1.to_owned()),
            population_policy_id: self.population_policy.policy_id.clone(),
            precedence_policy_id: self.precedence_policy.policy_id.clone(),
            build_commit: self.build_commit.clone(),
            spectral_route: if is_combined_product_band(&self.product_band) {
                "Hipparcos/XHIP-selected CK04; Hp-scaled CK04 336-650 nm plus Hp-scaled CK04 300-336 nm completion".into()
            } else {
                "Hipparcos/XHIP-selected CK04; Hp-scaled CK04 336-650 nm".into()
            },
            redistribution_scope: "derived map only; source catalogue bytes are not embedded"
                .into(),
            inputs: self.inputs.clone(),
        };
        provenance.validate()?;
        Ok(provenance)
    }

    pub fn to_json_pretty(&self) -> Result<Vec<u8>> {
        self.validate()?;
        Ok(serde_json::to_vec_pretty(self)?)
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

fn validate_source(source: &BrightStarSourceRecord, combined: bool) -> Result<()> {
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
    if combined && admitted {
        let uv = source
            .flux_300_336_ph_m2_s
            .filter(|v| v.is_finite() && *v > 0.0);
        let uv_stat = source
            .statistical_uncertainty_300_336_ph_m2_s
            .filter(|v| v.is_finite() && *v >= 0.0);
        let uv_sys = source
            .systematic_independent_uncertainty_300_336_ph_m2_s
            .filter(|v| v.is_finite() && *v >= 0.0);
        if uv.is_none()
            || uv_stat.is_none()
            || uv_sys.is_none()
            || source.uv_completion_model_id.as_deref()
                != Some(super::reconstruction::UV_COMPLETION_MODEL_ID_V1)
        {
            bail!(
                "combined bright-star admitted source {} requires justified 300-336 nm flux and provenance",
                source.source_id
            );
        }
    } else if !combined
        && (source.flux_300_336_ph_m2_s.is_some()
            || source.statistical_uncertainty_300_336_ph_m2_s.is_some()
            || source
                .systematic_independent_uncertainty_300_336_ph_m2_s
                .is_some()
            || source.uv_completion_model_id.is_some())
    {
        bail!(
            "measured-only bright-star source {} must not carry 300-336 nm fields",
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

/// Compare serialized pixels with a source-derived rebuild.
///
/// Flux and uncertainty fields allow a tiny tolerance so JSON deserialization
/// round-trips remain valid when decimal text does not uniquely identify an `f64`.
fn pixels_match(stored: &[BrightStarPixel], rebuilt: &[BrightStarPixel]) -> Result<bool> {
    let mut stored_by_pixel: BTreeMap<u64, &BrightStarPixel> =
        stored.iter().map(|pixel| (pixel.pixel, pixel)).collect();
    if stored_by_pixel.len() != stored.len() || stored.len() != rebuilt.len() {
        return Ok(false);
    }
    for pixel in rebuilt {
        let Some(stored_pixel) = stored_by_pixel.remove(&pixel.pixel) else {
            return Ok(false);
        };
        if stored_pixel.admitted_sources != pixel.admitted_sources {
            return Ok(false);
        }
        if stored_pixel.systematic_catalogue_correlated != pixel.systematic_catalogue_correlated {
            return Ok(false);
        }
        for (left, right, label) in [
            (
                stored_pixel.flux_ph_m2_s,
                pixel.flux_ph_m2_s,
                "flux_ph_m2_s",
            ),
            (
                stored_pixel.statistical_uncertainty_ph_m2_s,
                pixel.statistical_uncertainty_ph_m2_s,
                "statistical_uncertainty_ph_m2_s",
            ),
            (
                stored_pixel.systematic_independent_uncertainty_ph_m2_s,
                pixel.systematic_independent_uncertainty_ph_m2_s,
                "systematic_independent_uncertainty_ph_m2_s",
            ),
        ] {
            if !pixel_scalar_matches(left, right) {
                bail!(
                    "bright-star pixel {} {label} mismatch: stored {left} rebuilt {right}",
                    pixel.pixel,
                );
            }
        }
    }
    Ok(stored_by_pixel.is_empty())
}

fn pixel_scalar_matches(left: f64, right: f64) -> bool {
    if left.to_bits() == right.to_bits() {
        return true;
    }
    if !left.is_finite() || !right.is_finite() {
        return false;
    }
    let scale = left.abs().max(right.abs()).max(1.0);
    (left - right).abs() <= scale * 1e-12
}

fn rebuild_pixels(
    nside: u32,
    sources: &[BrightStarSourceRecord],
    combined: bool,
) -> Result<Vec<BrightStarPixel>> {
    crate::starlight::config::validate_canonical_nside(nside)?;
    let mut pixels: BTreeMap<u64, PixelBuild> = BTreeMap::new();
    for source in sources {
        validate_source(source, combined)?;
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
        entry.flux.add(source.selected_flux_ph_m2_s(combined)?)?;
        entry.stat_var.add(
            source
                .selected_statistical_uncertainty_ph_m2_s(combined)?
                .powi(2),
        )?;
        entry.independent_sys_var.add(
            source
                .selected_independent_systematic_ph_m2_s(combined)?
                .powi(2),
        )?;
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
    combined: bool,
) -> Result<()> {
    let admitted = |s: &&BrightStarSourceRecord| {
        matches!(
            s.class,
            SupplementClass::SupplementOnly | SupplementClass::MatchedAndReplacesPrimary
        )
    };
    let mut source_flux = StableSum::default();
    for source in sources.iter().filter(admitted) {
        source_flux.add(source.selected_flux_ph_m2_s(combined)?)?;
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
            .add(source.selected_flux_ph_m2_s(combined)?)?;
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
    validate_sha256(expected_sha256, "expected bright-star sha256")?;
    let actual = checksum_io::sha256_bytes(&bytes);
    if actual != expected_sha256 {
        bail!(
            "bright-star artifact sha256 mismatch: {actual} != {expected_sha256} ({})",
            path.display()
        );
    }
    let artifact: BrightStarArtifact = serde_json::from_slice(&bytes)
        .with_context(|| format!("parse bright-star artifact {}", path.display()))?;
    // Validate the serialized pixels before any optional in-memory
    // canonicalization. A matching byte checksum is an identity check, not a
    // substitute for source/pixel scientific consistency.
    artifact.validate()?;
    let combined = is_combined_product_band(&artifact.product_band);
    let mut canonical = artifact;
    canonical.pixels = rebuild_pixels(canonical.nside, &canonical.sources, combined)?;
    Ok(canonical)
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
            flux_300_336_ph_m2_s: None,
            flux_336_650_ph_m2_s: flux,
            statistical_uncertainty_300_336_ph_m2_s: None,
            statistical_uncertainty_ph_m2_s: flux * 0.01,
            systematic_independent_uncertainty_300_336_ph_m2_s: None,
            systematic_independent_uncertainty_ph_m2_s: flux * 0.02,
            systematic_catalogue_correlated: vec![CorrelatedUncertainty {
                correlation_group_id: "hip2-zero-point".into(),
                uncertainty_ph_m2_s: flux * 0.03,
            }],
            spectral_route: "fixture".into(),
            uv_completion_model_id: None,
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

    fn combined_source(
        id: &str,
        class: SupplementClass,
        gaia: Option<u64>,
        flux_uv: f64,
        flux: f64,
    ) -> BrightStarSourceRecord {
        BrightStarSourceRecord {
            source_id: id.into(),
            origin_catalogue: "hip2".into(),
            class,
            gaia_source_id: gaia,
            ra_deg_j2016: 10.0,
            dec_deg_j2016: 20.0,
            flux_300_336_ph_m2_s: Some(flux_uv),
            flux_336_650_ph_m2_s: flux,
            statistical_uncertainty_300_336_ph_m2_s: Some(flux_uv * 0.01),
            statistical_uncertainty_ph_m2_s: flux * 0.01,
            systematic_independent_uncertainty_300_336_ph_m2_s: Some(flux_uv * 0.02),
            systematic_independent_uncertainty_ph_m2_s: flux * 0.02,
            systematic_catalogue_correlated: vec![CorrelatedUncertainty {
                correlation_group_id: "hip2-zero-point".into(),
                uncertainty_ph_m2_s: flux * 0.03,
            }],
            spectral_route: "fixture".into(),
            uv_completion_model_id: Some(
                crate::starlight::bright_stars::reconstruction::UV_COMPLETION_MODEL_ID_V1.into(),
            ),
            classification_reason: "fixture".into(),
        }
    }

    fn combined_artifact(sources: Vec<BrightStarSourceRecord>) -> Result<BrightStarArtifact> {
        BrightStarArtifact::from_sources_with_product_band(
            1,
            &fixture_commit(),
            Vec::new(),
            sources,
            BrightStarPopulationPolicy::v1(),
            BrightStarPrecedencePolicy::v1(),
            BRIGHT_STAR_COMBINED_PRODUCT_BAND_ID,
        )
    }

    #[test]
    fn combined_artifact_json_roundtrip_loads_after_decimal_float_roundtrip() {
        let art = combined_artifact(vec![combined_source(
            "a",
            SupplementClass::SupplementOnly,
            None,
            1.0e11,
            5.0e11,
        )])
        .unwrap();
        let bytes = art.to_json_pretty().unwrap();
        let mut tmp = NamedTempFile::new().unwrap();
        std::io::Write::write_all(&mut tmp, &bytes).unwrap();
        let sha = checksum_io::sha256_bytes(&bytes);
        let loaded = load_bright_star_artifact(tmp.path(), &sha).unwrap();
        assert_eq!(loaded.sources, art.sources);
        assert_eq!(loaded.pixels.len(), art.pixels.len());
        assert!(pixels_match(&loaded.pixels, &art.pixels).unwrap());
    }

    #[test]
    fn checksum_pinned_corrupt_serialized_pixels_are_rejected() {
        let art = combined_artifact(vec![combined_source(
            "a",
            SupplementClass::SupplementOnly,
            None,
            1.0e11,
            5.0e11,
        )])
        .unwrap();
        let mut json = serde_json::to_value(art).unwrap();
        json["pixels"][0]["flux_ph_m2_s"] = serde_json::json!(1.0);
        let bytes = serde_json::to_vec_pretty(&json).unwrap();
        let mut tmp = NamedTempFile::new().unwrap();
        std::io::Write::write_all(&mut tmp, &bytes).unwrap();
        let sha = checksum_io::sha256_bytes(&bytes);
        let error = load_bright_star_artifact(tmp.path(), &sha).unwrap_err();
        assert!(
            error.to_string().contains("pixel") && error.to_string().contains("mismatch"),
            "{error}"
        );
    }

    #[test]
    fn combined_band_preserves_shared_scale_covariance() {
        let source = combined_source("a", SupplementClass::SupplementOnly, None, 100.0, 400.0);
        assert_eq!(
            source
                .selected_statistical_uncertainty_ph_m2_s(true)
                .unwrap(),
            5.0
        );
        assert_eq!(
            source
                .selected_independent_systematic_ph_m2_s(true)
                .unwrap(),
            10.0
        );
        assert!(5.0 > 1.0_f64.hypot(4.0));
        assert!(10.0 > 2.0_f64.hypot(8.0));

        let art = combined_artifact(vec![source]).unwrap();
        assert_eq!(art.pixels[0].statistical_uncertainty_ph_m2_s, 5.0);
        assert_eq!(
            art.pixels[0].systematic_independent_uncertainty_ph_m2_s,
            10.0
        );
    }

    #[test]
    fn combined_production_artifact_loads_if_present() {
        let path = Path::new("/tmp/combined-bright-stars.json");
        if !path.exists() {
            return;
        }
        let bytes = fs::read(path).unwrap();
        let sha = checksum_io::sha256_bytes(&bytes);
        load_bright_star_artifact(path, &sha).expect("production combined artifact must load");
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
        bad = provenance.clone();
        bad.population_policy_id = "other".into();
        assert!(bad.validate().is_err());
        bad = provenance.clone();
        bad.precedence_policy_id = "other".into();
        assert!(bad.validate().is_err());
        bad = provenance;
        bad.artifact_sha256 = "0".repeat(63);
        assert!(bad.validate().is_err());
    }

    #[test]
    fn artifact_header_invariants_fail_closed() {
        let mut art = artifact(vec![source(
            "a",
            SupplementClass::SupplementOnly,
            None,
            1.0,
        )])
        .unwrap();
        art.schema_version = 99;
        assert!(art.validate().is_err());
        art.schema_version = BRIGHT_STAR_ARTIFACT_SCHEMA_VERSION;
        art.model_id = "other".into();
        assert!(art.validate().is_err());
        art.model_id = BRIGHT_STAR_MODEL_ID.into();
        art.product_band = "other".into();
        assert!(art.validate().is_err());
        art.product_band = BRIGHT_STAR_PRODUCT_BAND_ID.into();
        art.ordering = "ring".into();
        assert!(art.validate().is_err());
        art.ordering = "nested".into();
        art.scientifically_validated = true;
        assert!(art.validate().is_err());
        art.scientifically_validated = false;
        art.redistribution_embedded = true;
        assert!(art.validate().is_err());
        art.redistribution_embedded = false;

        let bad_input = BrightStarInputProvenance {
            role: BrightStarInputRole::Hipparcos2,
            source_id: " ".into(),
            release: "r".into(),
            sha256: "a".repeat(64),
            retrieval_url: "https://example.invalid/a".into(),
            license_or_terms_url: "https://example.invalid/b".into(),
        };
        art.inputs = vec![bad_input.clone()];
        assert!(art.validate().is_err());
        let mut https = bad_input.clone();
        https.source_id = "ok".into();
        https.retrieval_url = "http://example.invalid/a".into();
        art.inputs = vec![https];
        assert!(art.validate().is_err());
        let mut dup = bad_input;
        dup.source_id = "ok".into();
        art.inputs = vec![dup.clone(), dup];
        assert!(art.validate().is_err());
    }
}
