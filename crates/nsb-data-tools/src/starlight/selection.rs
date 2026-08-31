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

use super::healpix::{self, nested_pixel_center, HealpixCoordinateFrame, HealpixOrderingScheme};
use super::uv::CalibrationStatus;
use siderust::coordinates::cartesian::Direction;
use siderust::coordinates::frames::{Galactic, ICRS};
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
        let nearest_tabulated_healpix = build_nearest_healpix_map(&artifact, &index)?;
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
            let distance = healpix::angular_separation_rad(query_direction, candidate_direction);
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
    // Logistic approximation of Cantat-Gaudin et al. (2023) / GaiaUnlimited
    // m10_to_completeness around the 50% completeness magnitude M10.
    let x = g_mag - m10;
    let completeness = 1.0 / (1.0 + (1.5 * x).exp());
    completeness.clamp(1.0e-3, 1.0)
}

fn build_nearest_healpix_map(
    artifact: &SelectionArtifact,
    index: &BTreeMap<(u32, u32, u32), f64>,
) -> Result<Vec<u32>> {
    let npix = 12u64
        .checked_mul(u64::from(artifact.healpix_nside).pow(2))
        .context("healpix pixel count overflow")?;
    let npix_usize = usize::try_from(npix).context("healpix pixel count does not fit usize")?;
    if !artifact.m10_map.is_empty() || index.is_empty() {
        return Ok((0..npix_usize).map(|i| i as u32).collect());
    }
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
            let distance = healpix::angular_separation_rad(query, candidate);
            if distance < best_distance {
                best_distance = distance;
                best_pixel = *sample;
            }
        }
        *slot = best_pixel;
    }
    Ok(nearest)
}

fn selection_pixel_center(
    artifact: &SelectionArtifact,
    nside: u32,
    healpix: u32,
) -> Result<Direction<ICRS>> {
    match artifact.coordinate_frame {
        HealpixCoordinateFrame::Equatorial => nested_pixel_center(nside, u64::from(healpix)),
        HealpixCoordinateFrame::Galactic => {
            let galactic: Direction<Galactic> = nested_pixel_center(nside, u64::from(healpix))?;
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
