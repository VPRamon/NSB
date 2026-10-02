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
pub const BRIGHT_STAR_ARTIFACT_SCHEMA_VERSION_COMBINED: u32 = 2;
pub const BRIGHT_STAR_MODEL_ID: &str = "starlight-bright-stars-v1";
pub const BRIGHT_STAR_MODEL_ID_COMBINED: &str = "starlight-bright-stars-combined-v1";
pub const BRIGHT_STAR_PRODUCT_BAND_ID: &str = "measured-336-650";
pub const BRIGHT_STAR_PRODUCT_BAND_COMBINED_ID: &str = "combined-300-650";

/// Declared spectral coverage of a bright-star supplement artifact.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BrightStarArtifactProductBand {
    Measured336To650,
    Combined300To650,
}

impl BrightStarArtifactProductBand {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Measured336To650 => BRIGHT_STAR_PRODUCT_BAND_ID,
            Self::Combined300To650 => BRIGHT_STAR_PRODUCT_BAND_COMBINED_ID,
        }
    }

    pub fn parse(value: &str) -> Result<Self> {
        match value {
            BRIGHT_STAR_PRODUCT_BAND_ID => Ok(Self::Measured336To650),
            BRIGHT_STAR_PRODUCT_BAND_COMBINED_ID => Ok(Self::Combined300To650),
            other => bail!("unsupported bright-star product_band {other}"),
        }
    }

    pub fn schema_version(self) -> u32 {
        match self {
            Self::Measured336To650 => BRIGHT_STAR_ARTIFACT_SCHEMA_VERSION,
            Self::Combined300To650 => BRIGHT_STAR_ARTIFACT_SCHEMA_VERSION_COMBINED,
        }
    }

    pub fn model_id(self) -> &'static str {
        match self {
            Self::Measured336To650 => BRIGHT_STAR_MODEL_ID,
            Self::Combined300To650 => BRIGHT_STAR_MODEL_ID_COMBINED,
        }
    }
}

/// Explicit 300--336 / 300--650 components for combined-band artifacts.
///
/// `flux_300_650` is always the sum of the stored 300--336 and 336--650 terms.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BrightStarBandComponents {
    pub flux_300_336_ph_m2_s: f64,
    pub flux_300_650_ph_m2_s: f64,
    pub statistical_uncertainty_300_336_ph_m2_s: f64,
    pub statistical_uncertainty_300_650_ph_m2_s: f64,
    pub systematic_independent_uncertainty_300_336_ph_m2_s: f64,
    pub systematic_independent_uncertainty_300_650_ph_m2_s: f64,
    pub systematic_catalogue_correlated_300_336: Vec<CorrelatedUncertainty>,
    pub systematic_catalogue_correlated_300_650: Vec<CorrelatedUncertainty>,
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
    pub flux_336_650_ph_m2_s: f64,
    pub statistical_uncertainty_ph_m2_s: f64,
    pub systematic_independent_uncertainty_ph_m2_s: f64,
    pub systematic_catalogue_correlated: Vec<CorrelatedUncertainty>,
    /// Required for combined-300-650 artifacts; forbidden on measured-only v1.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub band_components: Option<BrightStarBandComponents>,
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
    /// Spectral coverage declared by the verified artifact.
    #[serde(default = "default_measured_product_band")]
    pub product_band: String,
    /// Spectral reconstruction model identity for combined-band supplements.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub spectral_reconstruction_model_id: Option<String>,
}

fn default_measured_product_band() -> String {
    BRIGHT_STAR_PRODUCT_BAND_ID.to_owned()
}

impl BrightStarSupplementProvenance {
    pub fn validate(&self) -> Result<()> {
        validate_sha256(&self.artifact_sha256, "bright-star artifact_sha256")?;
        let product_band = BrightStarArtifactProductBand::parse(&self.product_band)?;
        if self.model_id != product_band.model_id() {
            bail!(
                "bright-star supplement provenance model_id/product_band mismatch: {} / {}",
                self.model_id,
                self.product_band
            );
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
        match product_band {
            BrightStarArtifactProductBand::Measured336To650 => {
                if let Some(model_id) = &self.spectral_reconstruction_model_id {
                    if model_id != super::reconstruction::SPECTRAL_RECONSTRUCTION_MODEL_ID_V1 {
                        bail!("bright-star measured provenance has unknown spectral reconstruction model");
                    }
                }
            }
            BrightStarArtifactProductBand::Combined300To650 => {
                match &self.spectral_reconstruction_model_id {
                    Some(model_id)
                        if model_id
                            == super::reconstruction::SPECTRAL_RECONSTRUCTION_MODEL_ID_V1 => {}
                    _ => bail!(
                        "combined bright-star provenance requires spectral reconstruction model {}",
                        super::reconstruction::SPECTRAL_RECONSTRUCTION_MODEL_ID_V1
                    ),
                }
            }
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
        sources: Vec<BrightStarSourceRecord>,
        population_policy: BrightStarPopulationPolicy,
        precedence_policy: BrightStarPrecedencePolicy,
    ) -> Result<Self> {
        Self::from_sources_for_band(
            nside,
            build_commit,
            inputs,
            sources,
            population_policy,
            precedence_policy,
            BrightStarArtifactProductBand::Measured336To650,
        )
    }

    pub fn from_sources_for_band(
        nside: u32,
        build_commit: &str,
        inputs: Vec<BrightStarInputProvenance>,
        mut sources: Vec<BrightStarSourceRecord>,
        population_policy: BrightStarPopulationPolicy,
        precedence_policy: BrightStarPrecedencePolicy,
        product_band: BrightStarArtifactProductBand,
    ) -> Result<Self> {
        sources.sort_by(|a, b| a.source_id.cmp(&b.source_id));
        let notes = match product_band {
            BrightStarArtifactProductBand::Measured336To650 => vec![
                "External opt-in supplement; catalogue bytes are not embedded in NSB.".into(),
                "Measured 336-650 nm only; use with 300-650 nm products is forbidden.".into(),
            ],
            BrightStarArtifactProductBand::Combined300To650 => vec![
                "External opt-in supplement; catalogue bytes are not embedded in NSB.".into(),
                "Combined 300-650 nm from one Hp-scaled CK04 SED; 300-336 is not an independent UV model.".into(),
                "Compatible only with Combined300To650 Starlight products.".into(),
            ],
        };
        let artifact = Self {
            schema_version: product_band.schema_version(),
            model_id: product_band.model_id().into(),
            product_band: product_band.as_str().into(),
            nside,
            ordering: "nested".into(),
            population_policy,
            precedence_policy,
            inputs,
            build_commit: build_commit.into(),
            counts: recompute_counts(&sources),
            pixels: rebuild_pixels(nside, product_band, &sources)?,
            sources,
            scientifically_validated: false,
            redistribution_embedded: false,
            notes,
        };
        artifact.validate()?;
        Ok(artifact)
    }

    pub fn product_band_kind(&self) -> Result<BrightStarArtifactProductBand> {
        BrightStarArtifactProductBand::parse(&self.product_band)
    }

    pub fn validate(&self) -> Result<()> {
        let product_band = self.product_band_kind()?;
        if self.schema_version != product_band.schema_version() {
            bail!(
                "bright-star schema_version {} does not match product_band {}",
                self.schema_version,
                self.product_band
            );
        }
        if self.model_id != product_band.model_id() {
            bail!(
                "bright-star model_id {} does not match product_band {}",
                self.model_id,
                self.product_band
            );
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
            validate_source(product_band, source)?;
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
        if self.pixels != rebuild_pixels(self.nside, product_band, &self.sources)? {
            bail!("bright-star serialized pixels do not match source reconstruction");
        }
        validate_conservation(self.nside, product_band, &self.sources, &self.pixels)?;
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
            product_band: self.product_band.clone(),
            spectral_reconstruction_model_id: match self.product_band_kind()? {
                BrightStarArtifactProductBand::Measured336To650 => None,
                BrightStarArtifactProductBand::Combined300To650 => {
                    Some(super::reconstruction::SPECTRAL_RECONSTRUCTION_MODEL_ID_V1.to_owned())
                }
            },
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

fn validate_source(
    product_band: BrightStarArtifactProductBand,
    source: &BrightStarSourceRecord,
) -> Result<()> {
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
    validate_correlated_groups(
        &source.systematic_catalogue_correlated,
        &source.source_id,
        "measured",
    )?;
    match (product_band, source.band_components.as_ref()) {
        (BrightStarArtifactProductBand::Measured336To650, None) => {}
        (BrightStarArtifactProductBand::Measured336To650, Some(_)) => bail!(
            "measured-only bright-star source {} must not carry combined band_components",
            source.source_id
        ),
        (BrightStarArtifactProductBand::Combined300To650, None) => bail!(
            "combined bright-star source {} requires band_components",
            source.source_id
        ),
        (BrightStarArtifactProductBand::Combined300To650, Some(components)) => {
            validate_band_components(source, components, admitted)?;
        }
    }
    Ok(())
}

fn validate_correlated_groups(
    groups: &[CorrelatedUncertainty],
    source_id: &str,
    label: &str,
) -> Result<()> {
    let mut seen = BTreeSet::new();
    for term in groups {
        if term.correlation_group_id.trim().is_empty()
            || !seen.insert(term.correlation_group_id.as_str())
            || !term.uncertainty_ph_m2_s.is_finite()
            || term.uncertainty_ph_m2_s < 0.0
        {
            bail!("bright-star source {source_id} has invalid {label} correlated uncertainty");
        }
    }
    Ok(())
}

fn validate_band_components(
    source: &BrightStarSourceRecord,
    components: &BrightStarBandComponents,
    admitted: bool,
) -> Result<()> {
    let numeric = [
        components.flux_300_336_ph_m2_s,
        components.flux_300_650_ph_m2_s,
        components.statistical_uncertainty_300_336_ph_m2_s,
        components.statistical_uncertainty_300_650_ph_m2_s,
        components.systematic_independent_uncertainty_300_336_ph_m2_s,
        components.systematic_independent_uncertainty_300_650_ph_m2_s,
    ];
    if numeric
        .iter()
        .any(|value| !value.is_finite() || *value < 0.0)
        || (admitted && components.flux_300_336_ph_m2_s <= 0.0)
    {
        bail!(
            "bright-star source {} has invalid combined-band flux or uncertainty",
            source.source_id
        );
    }
    let expected_total = components.flux_300_336_ph_m2_s + source.flux_336_650_ph_m2_s;
    let scale = expected_total.abs().max(1.0);
    if (components.flux_300_650_ph_m2_s - expected_total).abs() > 1e-9 * scale {
        bail!(
            "bright-star source {} violates flux_300_650 = flux_300_336 + flux_336_650",
            source.source_id
        );
    }
    let expected_stat =
        components.statistical_uncertainty_300_336_ph_m2_s + source.statistical_uncertainty_ph_m2_s;
    let expected_sys = components.systematic_independent_uncertainty_300_336_ph_m2_s
        + source.systematic_independent_uncertainty_ph_m2_s;
    if (components.statistical_uncertainty_300_650_ph_m2_s - expected_stat).abs()
        > 1e-9 * expected_stat.max(1.0)
        || (components.systematic_independent_uncertainty_300_650_ph_m2_s - expected_sys).abs()
            > 1e-9 * expected_sys.max(1.0)
    {
        bail!(
            "bright-star source {} violates fully-correlated band uncertainty conservation",
            source.source_id
        );
    }
    validate_correlated_groups(
        &components.systematic_catalogue_correlated_300_336,
        &source.source_id,
        "300-336",
    )?;
    validate_correlated_groups(
        &components.systematic_catalogue_correlated_300_650,
        &source.source_id,
        "300-650",
    )?;
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

fn rebuild_pixels(
    nside: u32,
    product_band: BrightStarArtifactProductBand,
    sources: &[BrightStarSourceRecord],
) -> Result<Vec<BrightStarPixel>> {
    crate::starlight::config::validate_canonical_nside(nside)?;
    let mut pixels: BTreeMap<u64, PixelBuild> = BTreeMap::new();
    for source in sources {
        validate_source(product_band, source)?;
        if !matches!(
            source.class,
            SupplementClass::SupplementOnly | SupplementClass::MatchedAndReplacesPrimary
        ) {
            continue;
        }
        let (flux, stat, sys, groups) = match product_band {
            BrightStarArtifactProductBand::Measured336To650 => (
                source.flux_336_650_ph_m2_s,
                source.statistical_uncertainty_ph_m2_s,
                source.systematic_independent_uncertainty_ph_m2_s,
                &source.systematic_catalogue_correlated,
            ),
            BrightStarArtifactProductBand::Combined300To650 => {
                let components = source
                    .band_components
                    .as_ref()
                    .context("combined source missing band_components")?;
                (
                    components.flux_300_650_ph_m2_s,
                    components.statistical_uncertainty_300_650_ph_m2_s,
                    components.systematic_independent_uncertainty_300_650_ph_m2_s,
                    &components.systematic_catalogue_correlated_300_650,
                )
            }
        };
        let pixel = u64::from(galactic_nested_pixel_from_icrs_position(
            source.ra_deg_j2016,
            source.dec_deg_j2016,
            nside,
        )?);
        let entry = pixels.entry(pixel).or_default();
        entry.flux.add(flux)?;
        entry.stat_var.add(stat.powi(2))?;
        entry.independent_sys_var.add(sys.powi(2))?;
        for term in groups {
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
    product_band: BrightStarArtifactProductBand,
    sources: &[BrightStarSourceRecord],
    pixels: &[BrightStarPixel],
) -> Result<()> {
    let admitted = |s: &&BrightStarSourceRecord| {
        matches!(
            s.class,
            SupplementClass::SupplementOnly | SupplementClass::MatchedAndReplacesPrimary
        )
    };
    let selected_flux = |source: &BrightStarSourceRecord| -> Result<f64> {
        Ok(match product_band {
            BrightStarArtifactProductBand::Measured336To650 => source.flux_336_650_ph_m2_s,
            BrightStarArtifactProductBand::Combined300To650 => {
                source
                    .band_components
                    .as_ref()
                    .context("combined source missing band_components")?
                    .flux_300_650_ph_m2_s
            }
        })
    };
    let mut source_flux = StableSum::default();
    for source in sources.iter().filter(admitted) {
        source_flux.add(selected_flux(source)?)?;
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
            .add(selected_flux(source)?)?;
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
            band_components: None,
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

    fn combined_source(
        id: &str,
        class: SupplementClass,
        gaia: Option<u64>,
        flux_336: f64,
        flux_300: f64,
    ) -> BrightStarSourceRecord {
        let mut record = source(id, class, gaia, flux_336);
        let relative = 0.01;
        record.band_components = Some(BrightStarBandComponents {
            flux_300_336_ph_m2_s: flux_300,
            flux_300_650_ph_m2_s: flux_300 + flux_336,
            statistical_uncertainty_300_336_ph_m2_s: flux_300 * relative,
            statistical_uncertainty_300_650_ph_m2_s: (flux_300 + flux_336) * relative,
            systematic_independent_uncertainty_300_336_ph_m2_s: flux_300 * 0.02,
            systematic_independent_uncertainty_300_650_ph_m2_s: (flux_300 + flux_336) * 0.02,
            systematic_catalogue_correlated_300_336: vec![CorrelatedUncertainty {
                correlation_group_id: "hip2-zero-point".into(),
                uncertainty_ph_m2_s: flux_300 * 0.03,
            }],
            systematic_catalogue_correlated_300_650: vec![CorrelatedUncertainty {
                correlation_group_id: "hip2-zero-point".into(),
                uncertainty_ph_m2_s: (flux_300 + flux_336) * 0.03,
            }],
        });
        // Measured-field statistical/sys must match the relative used above so
        // fully-correlated band conservation holds.
        record.statistical_uncertainty_ph_m2_s = flux_336 * relative;
        record.systematic_independent_uncertainty_ph_m2_s = flux_336 * 0.02;
        record.systematic_catalogue_correlated = vec![CorrelatedUncertainty {
            correlation_group_id: "hip2-zero-point".into(),
            uncertainty_ph_m2_s: flux_336 * 0.03,
        }];
        record
    }

    #[test]
    fn combined_artifact_conserves_band_flux_and_uncertainty() {
        let art = BrightStarArtifact::from_sources_for_band(
            1,
            &fixture_commit(),
            Vec::new(),
            vec![
                combined_source("a", SupplementClass::SupplementOnly, None, 10.0, 2.0),
                combined_source(
                    "b",
                    SupplementClass::MatchedAndReplacesPrimary,
                    Some(7),
                    5.0,
                    1.0,
                ),
            ],
            BrightStarPopulationPolicy::v1(),
            BrightStarPrecedencePolicy::v1(),
            BrightStarArtifactProductBand::Combined300To650,
        )
        .unwrap();
        assert_eq!(
            art.schema_version,
            BRIGHT_STAR_ARTIFACT_SCHEMA_VERSION_COMBINED
        );
        assert_eq!(art.model_id, BRIGHT_STAR_MODEL_ID_COMBINED);
        assert_eq!(art.product_band, BRIGHT_STAR_PRODUCT_BAND_COMBINED_ID);
        assert_eq!(art.pixels[0].flux_ph_m2_s, 18.0);
        let provenance = art.supplement_provenance(&"c".repeat(64)).unwrap();
        assert_eq!(
            provenance.spectral_reconstruction_model_id.as_deref(),
            Some(super::super::reconstruction::SPECTRAL_RECONSTRUCTION_MODEL_ID_V1)
        );
    }

    #[test]
    fn measured_artifact_rejects_combined_band_components() {
        let mut sources = vec![source("a", SupplementClass::SupplementOnly, None, 1.0)];
        sources[0].band_components = Some(BrightStarBandComponents {
            flux_300_336_ph_m2_s: 0.1,
            flux_300_650_ph_m2_s: 1.1,
            statistical_uncertainty_300_336_ph_m2_s: 0.0,
            statistical_uncertainty_300_650_ph_m2_s: 0.01,
            systematic_independent_uncertainty_300_336_ph_m2_s: 0.0,
            systematic_independent_uncertainty_300_650_ph_m2_s: 0.02,
            systematic_catalogue_correlated_300_336: Vec::new(),
            systematic_catalogue_correlated_300_650: Vec::new(),
        });
        assert!(BrightStarArtifact::from_sources(
            1,
            &fixture_commit(),
            Vec::new(),
            sources,
            BrightStarPopulationPolicy::v1(),
            BrightStarPrecedencePolicy::v1(),
        )
        .is_err());
    }

    #[test]
    fn combined_artifact_rejects_missing_or_drifting_band_components() {
        let missing = source("a", SupplementClass::SupplementOnly, None, 1.0);
        assert!(BrightStarArtifact::from_sources_for_band(
            1,
            &fixture_commit(),
            Vec::new(),
            vec![missing],
            BrightStarPopulationPolicy::v1(),
            BrightStarPrecedencePolicy::v1(),
            BrightStarArtifactProductBand::Combined300To650,
        )
        .is_err());

        let mut drifted = combined_source("a", SupplementClass::SupplementOnly, None, 1.0, 0.2);
        drifted
            .band_components
            .as_mut()
            .unwrap()
            .flux_300_650_ph_m2_s = 9.0;
        assert!(BrightStarArtifact::from_sources_for_band(
            1,
            &fixture_commit(),
            Vec::new(),
            vec![drifted],
            BrightStarPopulationPolicy::v1(),
            BrightStarPrecedencePolicy::v1(),
            BrightStarArtifactProductBand::Combined300To650,
        )
        .is_err());
    }

    #[test]
    fn provenance_defaults_and_spectral_model_contracts() {
        let measured = artifact(vec![source(
            "a",
            SupplementClass::SupplementOnly,
            None,
            1.0,
        )])
        .unwrap();
        let sha = "c".repeat(64);
        let mut provenance = measured.supplement_provenance(&sha).unwrap();
        let legacy = serde_json::json!({
            "artifact_sha256": sha,
            "model_id": BRIGHT_STAR_MODEL_ID,
            "population_policy_id": BrightStarPopulationPolicy::v1().policy_id,
            "precedence_policy_id": BrightStarPrecedencePolicy::v1().policy_id,
            "build_commit": fixture_commit(),
        });
        let decoded: BrightStarSupplementProvenance = serde_json::from_value(legacy).unwrap();
        assert_eq!(decoded.product_band, BRIGHT_STAR_PRODUCT_BAND_ID);
        assert!(decoded.validate().is_ok());

        provenance.spectral_reconstruction_model_id =
            Some(super::super::reconstruction::SPECTRAL_RECONSTRUCTION_MODEL_ID_V1.into());
        assert!(provenance.validate().is_ok());
        provenance.spectral_reconstruction_model_id = Some("unknown-model".into());
        assert!(provenance.validate().is_err());

        let combined = BrightStarArtifact::from_sources_for_band(
            1,
            &fixture_commit(),
            Vec::new(),
            vec![combined_source(
                "a",
                SupplementClass::SupplementOnly,
                None,
                1.0,
                0.2,
            )],
            BrightStarPopulationPolicy::v1(),
            BrightStarPrecedencePolicy::v1(),
            BrightStarArtifactProductBand::Combined300To650,
        )
        .unwrap();
        let mut combined_prov = combined.supplement_provenance(&sha).unwrap();
        assert!(combined_prov.validate().is_ok());
        combined_prov.spectral_reconstruction_model_id = None;
        assert!(combined_prov.validate().is_err());
    }

    #[test]
    fn combined_source_rejects_invalid_uv_flux_and_uncertainty_drift() {
        let zero_uv = combined_source("a", SupplementClass::SupplementOnly, None, 1.0, 0.0);
        assert!(BrightStarArtifact::from_sources_for_band(
            1,
            &fixture_commit(),
            Vec::new(),
            vec![zero_uv],
            BrightStarPopulationPolicy::v1(),
            BrightStarPrecedencePolicy::v1(),
            BrightStarArtifactProductBand::Combined300To650,
        )
        .is_err());

        let mut bad_stat = combined_source("a", SupplementClass::SupplementOnly, None, 1.0, 0.2);
        bad_stat
            .band_components
            .as_mut()
            .unwrap()
            .statistical_uncertainty_300_650_ph_m2_s = 9.0;
        assert!(BrightStarArtifact::from_sources_for_band(
            1,
            &fixture_commit(),
            Vec::new(),
            vec![bad_stat],
            BrightStarPopulationPolicy::v1(),
            BrightStarPrecedencePolicy::v1(),
            BrightStarArtifactProductBand::Combined300To650,
        )
        .is_err());

        let mut bad_group = combined_source("a", SupplementClass::SupplementOnly, None, 1.0, 0.2);
        bad_group
            .band_components
            .as_mut()
            .unwrap()
            .systematic_catalogue_correlated_300_336
            .push(CorrelatedUncertainty {
                correlation_group_id: "hip2-zero-point".into(),
                uncertainty_ph_m2_s: 0.01,
            });
        assert!(BrightStarArtifact::from_sources_for_band(
            1,
            &fixture_commit(),
            Vec::new(),
            vec![bad_group],
            BrightStarPopulationPolicy::v1(),
            BrightStarPrecedencePolicy::v1(),
            BrightStarArtifactProductBand::Combined300To650,
        )
        .is_err());
    }
}
