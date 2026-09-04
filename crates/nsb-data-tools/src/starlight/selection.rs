//! Versioned, fail-closed Gaia selection-function / faint-tail contract.
//!
//! Conditioned on HEALPix, G, and BP−RP. Missing colour follows
//! [`ColourMarginalisation`]. Absent cells: fail closed when
//! [`CalibrationStatus::Validated`]; sparse non-validated tables treat
//! absence as completeness `1.0` (weight `1`) — a documented limitation.

use crate::platform::checksum_io;
use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::Path;

use super::healpix::{self, HealpixCoordinateFrame, HealpixOrderingScheme};
use super::uv::CalibrationStatus;
use siderust::coordinates::frames::{Galactic, ICRS};
use siderust::coordinates::spherical::Direction;
use siderust::coordinates::transform::TransformFrame;

pub const SELECTION_ARTIFACT_SCHEMA_VERSION: u32 = 1;

/// Immutable reference-dataset identity embedded in a selection artifact.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SelectionReferenceDataset {
    pub name: String,
    pub release: String,
    pub licence: String,
    pub doi: String,
    pub files: Vec<SelectionReferenceFile>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SelectionReferenceFile {
    pub name: String,
    pub sha256: String,
}

/// How missing BP−RP is resolved against the colour axis.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "policy", rename_all = "kebab-case", deny_unknown_fields)]
pub enum ColourMarginalisation {
    AlwaysRequireColour,
    /// Average completeness over colour bins present for (healpix, magnitude).
    MarginaliseUniform,
    FixedColourBin {
        bin: u32,
    },
}

/// Residual flux beyond the effective Gaia magnitude limit.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FaintTailModel {
    pub enabled: bool,
    pub magnitude_limit_g: f64,
    pub residual_fraction_per_pixel: f64,
    pub systematic_fraction: f64,
}

/// One sky / magnitude / colour completeness cell.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CompletenessEntry {
    pub healpix: u32,
    pub magnitude_bin: u32,
    pub colour_bin: u32,
    /// Completeness in `(0, 1]`.
    pub completeness: f64,
}

/// Immutable Gaia selection-function artifact (schema v1).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SelectionArtifact {
    pub schema_version: u32,
    pub model_id: String,
    pub calibration_status: CalibrationStatus,
    pub reference_dataset: SelectionReferenceDataset,
    /// Maximum inverse-completeness weight (e.g. 5.0).
    pub weight_cap: f64,
    /// Sorted G-band magnitude bin edges (length = bins + 1).
    pub magnitude_bins: Vec<f64>,
    /// Sorted BP−RP colour bin edges (length = bins + 1).
    pub colour_bins: Vec<f64>,
    pub healpix_nside: u32,
    /// Coordinate frame of tabulated HEALPix cells (defaults to equatorial for v1).
    #[serde(default = "default_selection_coordinate_frame")]
    pub coordinate_frame: HealpixCoordinateFrame,
    /// Pixel ordering of tabulated HEALPix cells (defaults to nested for v1).
    #[serde(default = "default_selection_ordering")]
    pub ordering: HealpixOrderingScheme,
    /// Native spatial resolution of sparse `completeness_table` cells when they
    /// subsample a coarser HEALPix grid (e.g. NSIDE=32 representatives embedded
    /// at `healpix_nside`). When unset, inferred from table structure.
    #[serde(default)]
    pub table_spatial_nside: Option<u32>,
    /// Sparse completeness cells (optional when `m10_map` is present).
    #[serde(default)]
    pub completeness_table: Vec<CompletenessEntry>,
    /// Dense Cantat-Gaudin M10 map at `healpix_nside` (length = 12 nside²).
    /// When set, completeness is evaluated with the published logistic mapping.
    #[serde(default)]
    pub m10_map: Vec<f64>,
    pub colour_marginalisation: ColourMarginalisation,
    pub faint_tail: FaintTailModel,
    pub training_command: String,
    pub software_version: String,
}

/// Result of one selection-function evaluation.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SelectionEvaluation {
    /// Inverse-completeness weight clamped to `[1, weight_cap]`.
    pub weight: f64,
    pub completeness: f64,
    pub capped: bool,
    /// `0` when faint-tail is disabled or the source is bright.
    pub faint_tail_flux_fraction: f64,
    pub systematic_uncertainty_fraction: f64,
}

/// Validated artifact paired with the digest of its exact serialized bytes.
#[derive(Debug, Clone)]
pub struct SelectionCorrection {
    artifact: SelectionArtifact,
    artifact_sha256: String,
    index: BTreeMap<(u32, u32, u32), f64>,
    /// For each healpix, the nearest tabulated healpix used by sparse tables.
    nearest_tabulated_healpix: Vec<u32>,
}

impl SelectionArtifact {
    pub fn validate(&self) -> Result<()> {
        if self.schema_version != SELECTION_ARTIFACT_SCHEMA_VERSION {
            bail!(
                "unsupported selection artifact schema_version {}",
                self.schema_version
            );
        }
        require_text("model_id", &self.model_id)?;
        self.reference_dataset.validate()?;
        if !self.weight_cap.is_finite() || self.weight_cap < 1.0 {
            bail!("selection weight_cap must be finite and >= 1");
        }
        validate_sorted_edges("magnitude_bins", &self.magnitude_bins)?;
        validate_sorted_edges("colour_bins", &self.colour_bins)?;
        if !self.healpix_nside.is_power_of_two() || self.healpix_nside == 0 {
            bail!("selection healpix_nside must be a positive power of two");
        }
        if self.ordering != HealpixOrderingScheme::Nested {
            bail!(
                "selection artifact ordering {:?} is unsupported; only nested is implemented",
                self.ordering
            );
        }
        if self.coordinate_frame != HealpixCoordinateFrame::Equatorial {
            bail!(
                "selection artifact coordinate_frame {:?} is unsupported; only equatorial is implemented",
                self.coordinate_frame
            );
        }
        let n_mag = (self.magnitude_bins.len() - 1) as u32;
        let n_col = (self.colour_bins.len() - 1) as u32;
        let npix = 12u64
            .checked_mul(u64::from(self.healpix_nside).pow(2))
            .context("healpix pixel count overflow")?;
        if !self.m10_map.is_empty() {
            if self.m10_map.len() as u64 != npix {
                bail!(
                    "m10_map length {} does not match nside {} pixel count {npix}",
                    self.m10_map.len(),
                    self.healpix_nside
                );
            }
            if self.m10_map.iter().any(|v| !v.is_finite()) {
                bail!("m10_map contains non-finite values");
            }
        }
        if self.completeness_table.is_empty() && self.m10_map.is_empty() {
            bail!("selection artifact requires completeness_table or m10_map");
        }
        let mut keys = BTreeSet::new();
        for entry in &self.completeness_table {
            if u64::from(entry.healpix) >= npix {
                bail!(
                    "completeness healpix {} exceeds nside {} pixel count {npix}",
                    entry.healpix,
                    self.healpix_nside
                );
            }
            if entry.magnitude_bin >= n_mag || entry.colour_bin >= n_col {
                bail!("completeness bin indices out of range");
            }
            if !entry.completeness.is_finite()
                || entry.completeness <= 0.0
                || entry.completeness > 1.0
            {
                bail!("completeness must be finite and in (0, 1]");
            }
            if !keys.insert((entry.healpix, entry.magnitude_bin, entry.colour_bin)) {
                bail!(
                    "duplicate completeness cell ({}, {}, {})",
                    entry.healpix,
                    entry.magnitude_bin,
                    entry.colour_bin
                );
            }
        }
        match self.colour_marginalisation {
            ColourMarginalisation::AlwaysRequireColour
            | ColourMarginalisation::MarginaliseUniform => {}
            ColourMarginalisation::FixedColourBin { bin } if bin < n_col => {}
            ColourMarginalisation::FixedColourBin { bin } => {
                bail!("FixedColourBin {{ bin: {bin} }} exceeds colour axis")
            }
        }
        self.faint_tail.validate()?;
        require_text("training_command", &self.training_command)?;
        require_text("software_version", &self.software_version)
    }
}

impl SelectionReferenceDataset {
    fn validate(&self) -> Result<()> {
        require_text("reference dataset name", &self.name)?;
        require_text("reference dataset release", &self.release)?;
        require_text("reference dataset licence", &self.licence)?;
        require_text("reference dataset doi", &self.doi)?;
        if self.files.is_empty() {
            bail!("reference dataset requires immutable files");
        }
        let mut names = BTreeSet::new();
        for file in &self.files {
            require_safe_relative_path("reference file name", &file.name)?;
            require_sha256("reference file", &file.sha256)?;
            if !names.insert(file.name.as_str()) {
                bail!("duplicate reference file {}", file.name);
            }
        }
        Ok(())
    }
}

impl FaintTailModel {
    fn validate(&self) -> Result<()> {
        if !self.magnitude_limit_g.is_finite() {
            bail!("faint_tail.magnitude_limit_g must be finite");
        }
        for (label, value) in [
            (
                "residual_fraction_per_pixel",
                self.residual_fraction_per_pixel,
            ),
            ("systematic_fraction", self.systematic_fraction),
        ] {
            if !value.is_finite() || !(0.0..=1.0).contains(&value) {
                bail!("faint_tail.{label} must be finite and in [0, 1]");
            }
        }
        if self.enabled && self.residual_fraction_per_pixel == 0.0 {
            bail!("enabled faint_tail requires a positive residual_fraction_per_pixel");
        }
        Ok(())
    }
}

impl SelectionCorrection {
    /// Load exact artifact bytes, verify their pinned digest, and validate them.
    pub fn load(path: &Path, pinned_sha256: &str) -> Result<Self> {
        require_sha256("configured selection artifact", pinned_sha256)?;
        let bytes = fs::read(path)
            .with_context(|| format!("read selection artifact {}", path.display()))?;
        let actual = checksum_io::sha256_bytes(&bytes);
        if actual != pinned_sha256 {
            bail!(
                "selection artifact checksum mismatch for {}: expected {}, actual {}",
                path.display(),
                pinned_sha256,
                actual
            );
        }
        let artifact: SelectionArtifact = serde_json::from_slice(&bytes)
            .with_context(|| format!("parse selection artifact {}", path.display()))?;
        artifact.validate()?;
        let index: BTreeMap<(u32, u32, u32), f64> = artifact
            .completeness_table
            .iter()
            .map(|e| ((e.healpix, e.magnitude_bin, e.colour_bin), e.completeness))
            .collect();
        let nearest_tabulated_healpix =
            build_resolve_healpix_map(&artifact, &index, artifact.table_spatial_nside)?;
        Ok(Self {
            artifact,
            artifact_sha256: actual,
            index,
            nearest_tabulated_healpix,
        })
    }

    /// Refuse artifacts that have not completed the independent validation gate.
    pub fn require_production_status(&self) -> Result<()> {
        if self.artifact.calibration_status != CalibrationStatus::Validated {
            bail!(
                "selection artifact {} has status {:?}, not validated",
                self.artifact.model_id,
                self.artifact.calibration_status
            );
        }
        Ok(())
    }

    pub fn artifact(&self) -> &SelectionArtifact {
        &self.artifact
    }

    pub fn artifact_sha256(&self) -> &str {
        &self.artifact_sha256
    }

    /// Evaluate inverse-completeness weight and faint-tail terms.
    ///
    /// `healpix_nested` must already be at [`SelectionArtifact::healpix_nside`].
    pub fn evaluate(
        &self,
        healpix_nested: u32,
        g_mag: f64,
        bp_rp: Option<f64>,
    ) -> Result<SelectionEvaluation> {
        if !g_mag.is_finite() {
            bail!("selection G magnitude is not finite");
        }
        let npix = 12u64 * u64::from(self.artifact.healpix_nside).pow(2);
        if u64::from(healpix_nested) >= npix {
            bail!(
                "healpix {healpix_nested} exceeds nside {} pixel count {npix}",
                self.artifact.healpix_nside
            );
        }
        let magnitude_bin = bin_index("G", &self.artifact.magnitude_bins, g_mag)?;
        let completeness = if !self.artifact.m10_map.is_empty() {
            let m10 = self.artifact.m10_map[healpix_nested as usize];
            m10_to_completeness(g_mag, m10)
        } else {
            self.lookup_completeness(healpix_nested, magnitude_bin, bp_rp)?
        };
        let raw_weight = 1.0 / completeness;
        let capped = raw_weight > self.artifact.weight_cap;
        let weight = raw_weight.min(self.artifact.weight_cap).max(1.0);
        let (faint_tail_flux_fraction, systematic_uncertainty_fraction) =
            self.faint_tail_terms(g_mag);
        Ok(SelectionEvaluation {
            weight,
            completeness,
            capped,
            faint_tail_flux_fraction,
            systematic_uncertainty_fraction,
        })
    }

    fn lookup_completeness(
        &self,
        healpix: u32,
        magnitude_bin: u32,
        bp_rp: Option<f64>,
    ) -> Result<f64> {
        let colour_bin = match (bp_rp, &self.artifact.colour_marginalisation) {
            (Some(colour), _) => {
                if !colour.is_finite() {
                    bail!("selection BP−RP colour is not finite");
                }
                Some(bin_index("bp_rp", &self.artifact.colour_bins, colour)?)
            }
            (None, ColourMarginalisation::AlwaysRequireColour) => {
                bail!("selection artifact requires BP−RP colour")
            }
            (None, ColourMarginalisation::MarginaliseUniform) => None,
            (None, ColourMarginalisation::FixedColourBin { bin }) => Some(*bin),
        };

        if let Some(colour_bin) = colour_bin {
            let lookup_healpix = self.resolve_healpix(healpix);
            return match self.index.get(&(lookup_healpix, magnitude_bin, colour_bin)) {
                Some(c) => Ok(*c),
                None => self.absent_cell(healpix, magnitude_bin, Some(colour_bin)),
            };
        }

        let lookup_healpix = self.resolve_healpix(healpix);
        let values: Vec<f64> = self
            .index
            .range((lookup_healpix, magnitude_bin, 0)..=(lookup_healpix, magnitude_bin, u32::MAX))
            .map(|(_, c)| *c)
            .collect();
        if values.is_empty() {
            return self.absent_cell(healpix, magnitude_bin, None);
        }
        Ok(values.iter().sum::<f64>() / values.len() as f64)
    }

    fn resolve_healpix(&self, healpix: u32) -> u32 {
        self.nearest_tabulated_healpix
            .get(healpix as usize)
            .copied()
            .unwrap_or(healpix)
    }

    fn absent_cell(
        &self,
        healpix: u32,
        magnitude_bin: u32,
        colour_bin: Option<u32>,
    ) -> Result<f64> {
        let query_direction =
            selection_pixel_center(&self.artifact, self.artifact.healpix_nside, healpix)?;
        let mut best: Option<(f64, f64)> = None;
        for ((hpx, mag, col), completeness) in &self.index {
            if *mag != magnitude_bin {
                continue;
            }
            if let Some(wanted) = colour_bin {
                if *col != wanted {
                    continue;
                }
            }
            let candidate_direction =
                selection_pixel_center(&self.artifact, self.artifact.healpix_nside, *hpx)?;
            let distance = query_direction
                .angular_separation(&candidate_direction)
                .value();
            match best {
                None => best = Some((distance, *completeness)),
                Some((best_distance, _)) if distance < best_distance => {
                    best = Some((distance, *completeness));
                }
                _ => {}
            }
        }
        if let Some((_, completeness)) = best {
            return Ok(completeness);
        }
        if self.artifact.calibration_status == CalibrationStatus::Validated {
            bail!(
                "selection completeness cell missing for healpix={healpix} magnitude_bin={magnitude_bin} colour_bin={colour_bin:?}"
            );
        }
        Ok(1.0)
    }

    fn faint_tail_terms(&self, g_mag: f64) -> (f64, f64) {
        let faint = &self.artifact.faint_tail;
        if faint.enabled && g_mag > faint.magnitude_limit_g {
            (faint.residual_fraction_per_pixel, faint.systematic_fraction)
        } else {
            (0.0, 0.0)
        }
    }
}

fn default_selection_coordinate_frame() -> HealpixCoordinateFrame {
    HealpixCoordinateFrame::Equatorial
}

fn default_selection_ordering() -> HealpixOrderingScheme {
    HealpixOrderingScheme::Nested
}

fn m10_to_completeness(g_mag: f64, m10: f64) -> f64 {
    cantat_gaudin_m10_to_completeness(g_mag, m10).clamp(1.0e-3, 1.0)
}

/// Cantat-Gaudin et al. (2023) / GaiaUnlimited DR3 empirical completeness.
///
/// Parameters are the published posterior medians from `surveyTCG.py`.
pub fn cantat_gaudin_m10_to_completeness(g_mag: f64, m10: f64) -> f64 {
    const AX: f64 = 0.984_876_139_419_786_4;
    const BX: f64 = 0.647_315_551_023_014_6;
    const CX: f64 = 0.692_908_459_820_941_2;
    const AY: f64 = -0.003_935_382_139_847_386;
    const BY: f64 = 0.223_052_940_229_774_4;
    const CY: f64 = -0.093_318_774_681_602_35;
    const AZ: f64 = 0.006_144_107_896_473_064;
    const BZ: f64 = 0.036_817_059_337_444_38;
    const CZ: f64 = 0.351_405_645_257_228_95;
    const BREAK: f64 = 20.519_369_625_540_833;

    let predicted_g0 = if m10 > BREAK {
        CX * m10 + (AX - CX) * BREAK + BX
    } else {
        AX * m10 + BX
    };
    let predicted_invslope = if m10 > BREAK {
        CY * m10 + (AY - CY) * BREAK + BY
    } else {
        AY * m10 + BY
    };
    let predicted_shape = if m10 > BREAK {
        CZ * m10 + (AZ - CZ) * BREAK + BZ
    } else {
        AZ * m10 + BZ
    };
    cantat_gaudin_sigmoid(g_mag, predicted_g0, predicted_invslope, predicted_shape)
}

fn cantat_gaudin_sigmoid(g_mag: f64, g0: f64, invslope: f64, shape: f64) -> f64 {
    let delta = g_mag - g0;
    let tanh_term = 0.5 * ((delta / invslope).tanh() + 1.0);
    1.0 - tanh_term.powf(shape)
}

fn build_resolve_healpix_map(
    artifact: &SelectionArtifact,
    index: &BTreeMap<(u32, u32, u32), f64>,
    configured_table_nside: Option<u32>,
) -> Result<Vec<u32>> {
    let nside = artifact.healpix_nside;
    let npix = 12u64
        .checked_mul(u64::from(nside).pow(2))
        .context("healpix pixel count overflow")?;
    let npix_usize = usize::try_from(npix).context("healpix pixel count does not fit usize")?;
    if !artifact.m10_map.is_empty() || index.is_empty() {
        return Ok((0..npix_usize).map(|index| index as u32).collect());
    }
    let table_nside =
        configured_table_nside.or_else(|| infer_table_spatial_nside(artifact.healpix_nside, index));
    if let Some(table_nside) = table_nside {
        return build_hierarchical_resolve_map(artifact, nside, table_nside, index, npix_usize);
    }
    build_angular_nearest_healpix_map(artifact, index)
}

fn build_angular_nearest_healpix_map(
    artifact: &SelectionArtifact,
    index: &BTreeMap<(u32, u32, u32), f64>,
) -> Result<Vec<u32>> {
    let npix = 12u64
        .checked_mul(u64::from(artifact.healpix_nside).pow(2))
        .context("healpix pixel count overflow")?;
    let npix_usize = usize::try_from(npix).context("healpix pixel count does not fit usize")?;
    let mut samples: Vec<u32> = index.keys().map(|(hpx, _, _)| *hpx).collect();
    samples.sort_unstable();
    samples.dedup();
    let nside = artifact.healpix_nside;
    let mut nearest = vec![0_u32; npix_usize];
    for (pixel, slot) in nearest.iter_mut().enumerate() {
        let pixel = pixel as u32;
        let query = selection_pixel_center(artifact, nside, pixel)?;
        let mut best_pixel = samples[0];
        let mut best_distance = f64::INFINITY;
        for sample in &samples {
            let candidate = selection_pixel_center(artifact, nside, *sample)?;
            let distance = query.angular_separation(&candidate).value();
            if distance < best_distance {
                best_distance = distance;
                best_pixel = *sample;
            }
        }
        *slot = best_pixel;
    }
    Ok(nearest)
}

fn infer_table_spatial_nside(nside: u32, index: &BTreeMap<(u32, u32, u32), f64>) -> Option<u32> {
    let mut table_nside = 2_u32;
    while table_nside < nside {
        if !table_nside.is_power_of_two() {
            table_nside += 1;
            continue;
        }
        let mut parent_to_sample = BTreeMap::<u32, u32>::new();
        let mut valid = true;
        for (healpix, _, _) in index.keys() {
            let parent =
                healpix::nested_parent_at_coarser_nside(*healpix, nside, table_nside).ok()?;
            match parent_to_sample.get(&parent) {
                None => {
                    parent_to_sample.insert(parent, *healpix);
                }
                Some(existing) if *existing == *healpix => {}
                Some(_) => {
                    valid = false;
                    break;
                }
            }
        }
        let unique_samples = index
            .keys()
            .map(|(h, _, _)| *h)
            .collect::<std::collections::BTreeSet<_>>()
            .len();
        if valid && parent_to_sample.len() == unique_samples {
            return Some(table_nside);
        }
        table_nside += 1;
    }
    None
}

fn build_hierarchical_resolve_map(
    artifact: &SelectionArtifact,
    nside: u32,
    table_nside: u32,
    index: &BTreeMap<(u32, u32, u32), f64>,
    npix_usize: usize,
) -> Result<Vec<u32>> {
    let mut samples: Vec<u32> = index.keys().map(|(hpx, _, _)| *hpx).collect();
    samples.sort_unstable();
    samples.dedup();
    let mut parent_to_sample = BTreeMap::new();
    for healpix in samples {
        let parent = healpix::nested_parent_at_coarser_nside(healpix, nside, table_nside)?;
        if parent_to_sample.insert(parent, healpix).is_some() {
            bail!(
                "completeness_table has multiple samples for the same NSIDE={table_nside} parent"
            );
        }
    }
    let table_npix = 12_u64
        .checked_mul(u64::from(table_nside).pow(2))
        .context("table healpix pixel count overflow")?;
    let table_npix_u32 = u32::try_from(table_npix).context("table healpix pixel count overflow")?;
    let tabulated_parents: Vec<u32> = parent_to_sample.keys().copied().collect();
    let mut parent_centers = BTreeMap::new();
    for parent in &tabulated_parents {
        parent_centers.insert(
            *parent,
            selection_pixel_center(artifact, table_nside, *parent)?,
        );
    }
    let mut parent_resolve = parent_to_sample.clone();
    for parent in 0..table_npix_u32 {
        if parent_to_sample.contains_key(&parent) {
            continue;
        }
        let query = selection_pixel_center(artifact, table_nside, parent)?;
        let mut best_parent = tabulated_parents[0];
        let mut best_distance = f64::INFINITY;
        for candidate_parent in &tabulated_parents {
            let distance = query
                .angular_separation(&parent_centers[candidate_parent])
                .value();
            if distance < best_distance {
                best_distance = distance;
                best_parent = *candidate_parent;
            }
        }
        parent_resolve.insert(parent, parent_to_sample[&best_parent]);
    }
    let mut resolved = vec![0_u32; npix_usize];
    for (pixel, slot) in resolved.iter_mut().enumerate() {
        let pixel = pixel as u32;
        let parent = healpix::nested_parent_at_coarser_nside(pixel, nside, table_nside)?;
        *slot = parent_resolve
            .get(&parent)
            .copied()
            .context("parent resolve map missing NSIDE parent cell")?;
    }
    Ok(resolved)
}

fn selection_pixel_center(
    artifact: &SelectionArtifact,
    nside: u32,
    healpix: u32,
) -> Result<Direction<ICRS>> {
    match artifact.coordinate_frame {
        HealpixCoordinateFrame::Equatorial => {
            healpix::nested_pixel_center_spherical(nside, u64::from(healpix))
                .context("selection HEALPix index is outside the declared grid")
        }
        HealpixCoordinateFrame::Galactic => {
            let galactic: Direction<Galactic> =
                healpix::nested_pixel_center_spherical(nside, u64::from(healpix))
                    .context("selection HEALPix index is outside the declared grid")?;
            Ok(galactic.to_frame())
        }
    }
}

fn bin_index(label: &str, edges: &[f64], value: f64) -> Result<u32> {
    let last = edges.len() - 1;
    // Clamp out-of-range photometry onto the edge bins rather than rejecting
    // the whole Galactic population at the bright/faint extremes.
    let clamped = value.clamp(edges[0], edges[last]);
    for index in 0..last {
        let upper = edges[index + 1];
        if clamped < upper || (index + 1 == last && clamped <= upper) {
            return Ok(index as u32);
        }
    }
    bail!("{label}={value} did not resolve to a selection bin");
}

fn validate_sorted_edges(label: &str, edges: &[f64]) -> Result<()> {
    if edges.len() < 2 || edges.iter().any(|e| !e.is_finite()) {
        bail!("{label} requires >= 2 finite edges");
    }
    if edges.windows(2).any(|w| w[0] >= w[1]) {
        bail!("{label} must be strictly increasing");
    }
    Ok(())
}

fn require_text(label: &str, value: &str) -> Result<()> {
    let normalized = value.trim().to_ascii_lowercase();
    if normalized.is_empty()
        || ["placeholder", "todo", "tbd", "unknown", "unspecified"]
            .iter()
            .any(|m| normalized == *m || normalized.contains(&format!("<{m}>")))
    {
        bail!("{label} is missing or contains a placeholder");
    }
    Ok(())
}

fn require_safe_relative_path(label: &str, value: &str) -> Result<()> {
    require_text(label, value)?;
    let path = Path::new(value);
    if path.is_absolute()
        || path.components().any(|c| {
            matches!(
                c,
                std::path::Component::ParentDir
                    | std::path::Component::RootDir
                    | std::path::Component::Prefix(_)
            )
        })
    {
        bail!("{label} must be a safe relative path");
    }
    Ok(())
}

fn require_sha256(label: &str, value: &str) -> Result<()> {
    if value.len() != 64
        || !value
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    {
        bail!("{label} SHA-256 must contain 64 lowercase hexadecimal characters");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::platform::checksum_io;
    use std::path::PathBuf;
    use tempfile::TempDir;

    fn cell(h: u32, m: u32, c: u32, completeness: f64) -> CompletenessEntry {
        CompletenessEntry {
            healpix: h,
            magnitude_bin: m,
            colour_bin: c,
            completeness,
        }
    }

    fn tiny_artifact(status: CalibrationStatus) -> SelectionArtifact {
        SelectionArtifact {
            schema_version: SELECTION_ARTIFACT_SCHEMA_VERSION,
            model_id: "gaia-dr3-selection-function-test-v1".to_string(),
            calibration_status: status,
            reference_dataset: SelectionReferenceDataset {
                name: "cantat-gaudin-gaia-dr3-selection-function".to_string(),
                release: "2023".to_string(),
                licence: "CC-BY-4.0".to_string(),
                doi: "10.1051/0004-6361/202245394".to_string(),
                files: vec![SelectionReferenceFile {
                    name: "completeness.parquet".to_string(),
                    sha256: "a".repeat(64),
                }],
            },
            weight_cap: 5.0,
            magnitude_bins: vec![10.0, 15.0, 20.0],
            colour_bins: vec![0.0, 1.0, 2.0],
            healpix_nside: 1,
            coordinate_frame: HealpixCoordinateFrame::Equatorial,
            ordering: HealpixOrderingScheme::Nested,
            table_spatial_nside: None,
            completeness_table: vec![cell(0, 0, 0, 1.0), cell(0, 1, 0, 0.5), cell(0, 1, 1, 0.25)],
            m10_map: Vec::new(),
            colour_marginalisation: ColourMarginalisation::MarginaliseUniform,
            faint_tail: FaintTailModel {
                enabled: true,
                magnitude_limit_g: 19.0,
                residual_fraction_per_pixel: 0.08,
                systematic_fraction: 0.15,
            },
            training_command: "python train_selection.py --release 2023".to_string(),
            software_version: "nsb-data-tools-test".to_string(),
        }
    }

    fn load_artifact(artifact: &SelectionArtifact) -> SelectionCorrection {
        let temporary = TempDir::new().unwrap();
        let path = temporary.path().join("selection.json");
        let bytes = serde_json::to_vec_pretty(artifact).unwrap();
        std::fs::write(&path, &bytes).unwrap();
        SelectionCorrection::load(&path, &checksum_io::sha256_bytes(&bytes)).unwrap()
    }

    #[test]
    fn cantat_gaudin_m10_to_completeness_matches_gaiaunlimited_golden_cases() {
        let cases = [
            ((12.0, 15.0), 1.000_000_00),
            ((12.0, 17.0), 1.000_000_00),
            ((12.0, 20.0), 1.000_000_00),
            ((14.0, 15.0), 0.892_895_53),
            ((14.0, 17.0), 1.000_000_00),
            ((15.0, 15.0), 0.484_189_61),
            ((15.0, 17.0), 0.986_763_65),
            ((16.0, 15.0), 0.000_109_98),
            ((16.0, 17.0), 0.919_168_36),
            ((16.0, 18.0), 0.989_951_28),
            ((17.0, 15.0), 0.000_000_00),
            ((17.0, 17.0), 0.506_868_77),
            ((17.0, 18.0), 0.930_286_16),
            ((17.0, 19.0), 0.992_462_72),
            ((18.0, 17.0), 0.000_057_28),
            ((18.0, 18.0), 0.516_910_99),
            ((18.0, 19.0), 0.940_195_46),
            ((19.0, 19.0), 0.526_083_35),
            ((19.0, 20.0), 0.948_992_16),
            ((19.5, 19.0), 0.021_395_78),
            ((20.0, 19.0), 0.000_027_36),
            ((20.0, 20.0), 0.534_375_19),
            ((20.0, 21.0), 0.999_696_16),
            ((21.0, 20.0), 0.000_018_23),
            ((21.0, 21.0), 0.726_469_16),
        ];
        for ((g, m10), expected) in cases {
            let actual = cantat_gaudin_m10_to_completeness(g, m10);
            assert!(
                (actual - expected).abs() < 1.0e-6,
                "g={g} m10={m10}: actual={actual}, expected={expected}"
            );
        }
    }

    #[test]
    fn production_selection_artifact_uses_hierarchical_nside32_resolution() {
        let path = PathBuf::from("/tmp/selection-artifact.json");
        if !path.is_file() {
            return;
        }
        let bytes = std::fs::read(&path).unwrap();
        let artifact: SelectionArtifact = serde_json::from_slice(&bytes).unwrap();
        let index: BTreeMap<(u32, u32, u32), f64> = artifact
            .completeness_table
            .iter()
            .map(|entry| {
                (
                    (entry.healpix, entry.magnitude_bin, entry.colour_bin),
                    entry.completeness,
                )
            })
            .collect();
        assert_eq!(
            infer_table_spatial_nside(artifact.healpix_nside, &index),
            Some(32)
        );
    }

    #[test]
    #[ignore = "expensive production-artifact diagnostic; run with --ignored"]
    fn production_artifact_angular_nearest_creates_sharper_weight_boundaries_than_hierarchical() {
        let path = PathBuf::from("/tmp/selection-artifact.json");
        if !path.is_file() {
            return;
        }
        let bytes = std::fs::read(&path).unwrap();
        let artifact: SelectionArtifact = serde_json::from_slice(&bytes).unwrap();
        let index: BTreeMap<(u32, u32, u32), f64> = artifact
            .completeness_table
            .iter()
            .map(|entry| {
                (
                    (entry.healpix, entry.magnitude_bin, entry.colour_bin),
                    entry.completeness,
                )
            })
            .collect();
        let table_nside = infer_table_spatial_nside(artifact.healpix_nside, &index).unwrap();
        let npix = 12usize * artifact.healpix_nside as usize * artifact.healpix_nside as usize;
        let angular = build_angular_nearest_healpix_map(&artifact, &index).unwrap();
        let hierarchical = build_hierarchical_resolve_map(
            &artifact,
            artifact.healpix_nside,
            table_nside,
            &index,
            npix,
        )
        .unwrap();
        let differing = angular
            .iter()
            .zip(hierarchical.iter())
            .filter(|(left, right)| left != right)
            .count();
        assert!(
            differing > npix / 4,
            "expected angular and hierarchical lookup to disagree on most pixels, got {differing}/{npix}"
        );

        let g_mag = 17.0;
        let magnitude_bin = bin_index("G", &artifact.magnitude_bins, g_mag).unwrap();
        let colour_bin = bin_index("bp_rp", &artifact.colour_bins, 0.8).unwrap();
        let weight = |resolved: u32| -> f64 {
            let completeness = index
                .get(&(resolved, magnitude_bin, colour_bin))
                .copied()
                .unwrap_or(1.0);
            (1.0 / completeness).min(artifact.weight_cap).max(1.0)
        };
        let discontinuity_edges = |resolve: &[u32]| -> usize {
            let mut edges = 0usize;
            for pixel in 0..npix {
                let value = weight(resolve[pixel]);
                let face = (pixel as u32) / (artifact.healpix_nside * artifact.healpix_nside);
                let ipf = (pixel as u32) % (artifact.healpix_nside * artifact.healpix_nside);
                let ix = (0..16).fold(0_u32, |acc, bit| acc | (((ipf >> (2 * bit)) & 1) << bit));
                let iy = (0..16).fold(0_u32, |acc, bit| {
                    acc | (((ipf >> (2 * bit + 1)) & 1) << bit)
                });
                for (dx, dy) in [(0_i32, 1), (1, 0)] {
                    let nx = ix as i32 + dx;
                    let ny = iy as i32 + dy;
                    if nx < 0
                        || ny < 0
                        || nx >= artifact.healpix_nside as i32
                        || ny >= artifact.healpix_nside as i32
                    {
                        continue;
                    }
                    let nipf = (0..16).fold(0_u32, |acc, bit| {
                        acc | ((((nx >> bit) & 1) as u32) << bit)
                            | ((((ny >> bit) & 1) as u32) << (bit + 1))
                    });
                    let neighbour =
                        (face * artifact.healpix_nside * artifact.healpix_nside + nipf) as usize;
                    if (weight(resolve[neighbour]) - value).abs() > 1.0e-12 {
                        edges += 1;
                    }
                }
            }
            edges
        };
        let angular_edges = discontinuity_edges(&angular);
        let hierarchical_edges = discontinuity_edges(&hierarchical);
        assert!(
            angular_edges > hierarchical_edges,
            "angular lookup should create more G={g_mag} weight discontinuity edges ({angular_edges}) than hierarchical ({hierarchical_edges})"
        );
    }

    #[test]
    fn hierarchical_resolve_is_constant_within_nside32_parent_for_sparse_table() {
        let mut artifact = tiny_artifact(CalibrationStatus::Candidate);
        artifact.healpix_nside = 8;
        artifact.table_spatial_nside = Some(2);
        artifact.completeness_table = vec![
            cell(0, 0, 0, 0.9),
            cell(16, 0, 0, 0.5),
            cell(0, 1, 0, 0.4),
            cell(16, 1, 0, 0.2),
        ];
        let correction = load_artifact(&artifact);
        let weights: Vec<f64> = (0..12 * 8 * 8)
            .map(|healpix| {
                correction
                    .evaluate(healpix as u32, 12.0, Some(0.4))
                    .unwrap()
                    .weight
            })
            .collect();
        for parent in 0..48_u32 {
            let members: Vec<f64> = (0..weights.len() as u32)
                .filter(|pixel| {
                    healpix::nested_parent_at_coarser_nside(*pixel, 8, 2).unwrap() == parent
                })
                .map(|pixel| weights[pixel as usize])
                .collect();
            if members.is_empty() {
                continue;
            }
            let first = members[0];
            assert!(
                members
                    .iter()
                    .all(|weight| (*weight - first).abs() < 1.0e-12),
                "parent {parent} should share one resolved completeness weight, got {members:?}"
            );
        }
    }

    #[test]
    fn evaluate_applies_weight_cap_and_faint_tail() {
        let correction = load_artifact(&tiny_artifact(CalibrationStatus::Candidate));
        let bright = correction.evaluate(0, 12.0, Some(0.4)).unwrap();
        assert_eq!(
            (bright.completeness, bright.weight, bright.capped),
            (1.0, 1.0, false)
        );
        assert_eq!(bright.faint_tail_flux_fraction, 0.0);

        let faint = correction.evaluate(0, 17.0, Some(0.4)).unwrap();
        assert_eq!((faint.completeness, faint.weight), (0.5, 2.0));

        let mut dense = tiny_artifact(CalibrationStatus::Candidate);
        dense.completeness_table[2].completeness = 0.1;
        let correction = load_artifact(&dense);
        let capped = correction.evaluate(0, 17.0, Some(1.5)).unwrap();
        assert_eq!(
            (capped.completeness, capped.weight, capped.capped),
            (0.1, 5.0, true)
        );

        let tail = correction.evaluate(0, 19.5, Some(0.4)).unwrap();
        assert_eq!(
            (
                tail.faint_tail_flux_fraction,
                tail.systematic_uncertainty_fraction
            ),
            (0.08, 0.15)
        );
    }

    #[test]
    fn sparse_absent_cell_and_colour_marginalisation() {
        let candidate = load_artifact(&tiny_artifact(CalibrationStatus::Candidate));
        // Healpix 1 is not tabulated; nearest-neighbor maps it onto healpix 0.
        let nearest = candidate.evaluate(1, 12.0, Some(0.4)).unwrap();
        assert_eq!((nearest.completeness, nearest.weight), (1.0, 1.0));

        let avg = candidate.evaluate(0, 17.0, None).unwrap();
        assert!((avg.completeness - 0.375).abs() < 1e-12);

        let validated = load_artifact(&tiny_artifact(CalibrationStatus::Validated));
        let validated_nearest = validated.evaluate(1, 12.0, Some(0.4)).unwrap();
        assert_eq!(
            (validated_nearest.completeness, validated_nearest.weight),
            (1.0, 1.0)
        );
        validated.require_production_status().unwrap();
    }

    #[test]
    fn absent_cell_angular_nearest_runs_when_resolved_pixel_lacks_bin() {
        // Healpix 0 is tabulated only for the faint magnitude bin; healpix 2
        // carries the bright bin. Querying an untabulated pixel for the bright
        // bin therefore resolves onto healpix 0, misses the exact key, and must
        // fall through to absent_cell angular nearest (healpix 2).
        let mut artifact = tiny_artifact(CalibrationStatus::Candidate);
        artifact.completeness_table = vec![cell(0, 1, 0, 0.5), cell(2, 0, 0, 0.8)];
        let correction = load_artifact(&artifact);
        let evaluation = correction.evaluate(1, 12.0, Some(0.4)).unwrap();
        assert_eq!(
            (evaluation.completeness, evaluation.weight),
            (0.8, 1.0 / 0.8)
        );
    }

    #[test]
    fn selection_pixel_center_supports_equatorial_and_galactic_frames() {
        let mut artifact = tiny_artifact(CalibrationStatus::Candidate);
        let equatorial = selection_pixel_center(&artifact, 1, 0).unwrap();
        artifact.coordinate_frame = HealpixCoordinateFrame::Galactic;
        let galactic = selection_pixel_center(&artifact, 1, 0).unwrap();
        assert!(
            equatorial.angular_separation(&galactic).value() > 0.0,
            "Galactic frame transform must move the typed ICRS query direction"
        );
    }

    #[test]
    fn checksum_mismatch_and_placeholders_fail_closed() {
        let artifact = tiny_artifact(CalibrationStatus::TestOnly);
        let temporary = TempDir::new().unwrap();
        let path = temporary.path().join("selection.json");
        let bytes = serde_json::to_vec_pretty(&artifact).unwrap();
        std::fs::write(&path, &bytes).unwrap();
        assert!(SelectionCorrection::load(&path, &"0".repeat(64))
            .unwrap_err()
            .to_string()
            .contains("checksum mismatch"));

        let mut bad = artifact;
        bad.training_command = "TODO".to_string();
        assert!(bad.validate().is_err());
        assert!(load_artifact(&tiny_artifact(CalibrationStatus::TestOnly))
            .require_production_status()
            .is_err());
    }
}
