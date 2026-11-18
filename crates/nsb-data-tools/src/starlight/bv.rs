// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Vallés Puig, Ramon
//! Offline monochromatic Starlight photometry for candidate products.

use super::config::{ArtifactPinConfig, GaiaProductConfig};
use super::healpix::{gaia_nested_npix, galactic_nested_pixel_from_icrs_position, IcrsSkyPosition};
use super::map::accumulator::StableSum;
use super::photometric::PhotometricFeatures;
use super::uv::PartitionEvidence;
use super::xp::calibrate::BV_REFERENCE_NM;
use crate::platform::checksum_io;
use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::Path;

/// Prevent candidate shards from reusing B/V values across different source
/// inventories or integrated-population calibration pins.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct BvInputPins {
    pub calibration_sha256: String,
    pub photometric_sha256: Option<String>,
    pub selection_sha256: Option<String>,
    pub bright_star_sha256: Option<String>,
    pub gaia_inventory_sha256: BTreeMap<String, String>,
}

impl BvInputPins {
    pub(crate) fn new(
        bv: &ArtifactPinConfig,
        photometric: Option<&ArtifactPinConfig>,
        selection: Option<&ArtifactPinConfig>,
        bright_star: Option<&ArtifactPinConfig>,
        products: &[GaiaProductConfig],
    ) -> Self {
        let sha = |pin: &ArtifactPinConfig| pin.sha256.clone();
        Self {
            calibration_sha256: sha(bv),
            photometric_sha256: photometric.map(sha),
            selection_sha256: selection.map(sha),
            bright_star_sha256: bright_star.map(sha),
            gaia_inventory_sha256: products
                .iter()
                .map(|product| (product.id.clone(), product.checksum_manifest_sha256.clone()))
                .collect(),
        }
    }

    pub(crate) fn validate(&self) -> Result<()> {
        if std::iter::once(&self.calibration_sha256)
            .chain(&self.photometric_sha256)
            .chain(&self.selection_sha256)
            .chain(&self.bright_star_sha256)
            .chain(self.gaia_inventory_sha256.values())
            .any(|sha| !is_sha256(sha))
        {
            bail!("invalid B/V shard input checksum");
        }
        Ok(())
    }
}

/// Spectral energy flux density in W m⁻² nm⁻¹. The cross term is signed.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BvFlux {
    pub flux_w_m2_nm: [f64; 2],
    /// Variances for B and V and their covariance (B-var, V-var, B/V-cov).
    pub statistical_covariance: [f64; 3],
    pub independent_systematic_sigma: [f64; 2],
    pub shared_systematic_sigma: [f64; 2],
}

impl BvFlux {
    pub(crate) fn validate(self) -> Result<()> {
        check_covariance(self.statistical_covariance)?;
        if self.flux_w_m2_nm.iter().any(|x| !x.is_finite())
            || self
                .independent_systematic_sigma
                .into_iter()
                .chain(self.shared_systematic_sigma)
                .any(|x| !x.is_finite() || x < 0.0)
        {
            bail!("invalid B/V spectral flux density or systematic uncertainty");
        }
        Ok(())
    }

    pub(crate) fn weighted(self, weight: f64) -> Self {
        Self {
            flux_w_m2_nm: self.flux_w_m2_nm.map(|x| x * weight),
            statistical_covariance: self.statistical_covariance.map(|x| x * weight * weight),
            independent_systematic_sigma: self.independent_systematic_sigma.map(|x| x * weight),
            shared_systematic_sigma: self.shared_systematic_sigma.map(|x| x * weight),
        }
    }
}

fn check_covariance([b, v, cross]: [f64; 3]) -> Result<()> {
    if ![b, v, cross].into_iter().all(f64::is_finite)
        || b < 0.0
        || v < 0.0
        || cross.abs() > (b.sqrt() * v.sqrt()) * (1.0 + 8.0 * f64::EPSILON)
    {
        bail!("invalid B/V covariance");
    }
    Ok(())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum ColourRoute {
    Full,
    GAndBp,
    GAndRp,
    GOnly,
}

impl ColourRoute {
    /// Return the route and the colours used in its conditional bins.
    /// The ordered full-colour features are BP−G and G−RP.
    fn features(features: PhotometricFeatures) -> Option<(Self, f64, [Option<f64>; 2])> {
        let g = features.phot_g_mean_mag.filter(|x| x.is_finite())?;
        let bp = features.phot_bp_mean_mag;
        let rp = features.phot_rp_mean_mag;
        if [bp, rp].into_iter().flatten().any(|x| !x.is_finite()) {
            return None;
        }
        let route = match (bp, rp) {
            (Some(_), Some(_)) => Self::Full,
            (Some(_), None) => Self::GAndBp,
            (None, Some(_)) => Self::GAndRp,
            (None, None) => Self::GOnly,
        };
        features
            .quality_flag
            .then_some((route, g, [bp.map(|x| x - g), rp.map(|x| g - x)]))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct CellKey {
    route: ColourRoute,
    sky_pixel: u32,
    g_bin: usize,
    bp_g_bin: Option<usize>,
    g_rp_bin: Option<usize>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Cell {
    key: CellKey,
    /// Conditional mean and predictive covariance of f_lambda / 10^(-0.4*(G-15)).
    mean_w_m2_nm_at_g15: [f64; 2],
    residual_covariance_at_g15: [f64; 3],
    /// Shared held-out model error within this conditional cell.
    shared_fraction: [f64; 2],
    training_sources: u64,
    validation_sources: u64,
    validation_sky_cells: usize,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Calibration {
    schema_version: u32,
    calibration_status: String,
    reference_provenance: String,
    policy_sha256: String,
    min_training: u64,
    min_validation: u64,
    min_validation_sky_cells: usize,
    reference_nm: [f64; 2],
    sky_nside: u32,
    g_edges: Vec<f64>,
    bp_g_edges: Vec<f64>,
    g_rp_edges: Vec<f64>,
    cells: Vec<Cell>,
    xp_shared_calibration_fraction: [f64; 2],
    xp_design_sha256: String,
    partitions: PartitionEvidence,
    software_commit: String,
    training_command: String,
    held_out_report_sha256: String,
}

#[derive(Debug)]
pub(crate) struct BvInference {
    pub xp_shared_calibration_fraction: [f64; 2],
    pub xp_design_sha256: String,
    sky_nside: u32,
    g_edges: Vec<f64>,
    bp_g_edges: Vec<f64>,
    g_rp_edges: Vec<f64>,
    cells: BTreeMap<CellKey, Cell>,
}

impl BvInference {
    pub(crate) fn load(path: &Path, expected_sha256: &str) -> Result<Self> {
        let bytes = fs::read(path).with_context(|| format!("read {}", path.display()))?;
        if checksum_io::sha256_bytes(&bytes) != expected_sha256 {
            bail!("B/V calibration checksum mismatch");
        }
        let a: Calibration = serde_json::from_slice(&bytes)?;
        let pixels = gaia_nested_npix(a.sky_nside)?;
        a.partitions
            .validate()
            .context("B/V calibration partition evidence")?;
        if a.schema_version != 1
            || a.calibration_status != "candidate"
            || a.reference_provenance.trim().is_empty()
            || !is_sha256(&a.policy_sha256)
            || !matches!(a.sky_nside, 1 | 2 | 4 | 8)
            || a.min_training < 2
            || a.min_validation < 2
            || a.min_validation_sky_cells < 2
            || a.reference_nm != BV_REFERENCE_NM
            || a.cells.is_empty()
            || a.training_command.trim().is_empty()
            || (a.software_commit.len() != 40
                || !a.software_commit.bytes().all(|b| b.is_ascii_hexdigit()))
            || !is_sha256(&a.xp_design_sha256)
            || !is_sha256(&a.held_out_report_sha256)
            || a.xp_shared_calibration_fraction
                .iter()
                .any(|x| !x.is_finite() || *x <= 0.0)
        {
            bail!("invalid B/V calibration identity or partition evidence");
        }
        let validation_sky = calibration_splits(&a.partitions)?;
        let fine_pixels_per_parent = (16 / a.sky_nside).pow(2);
        let mut validation_by_parent = BTreeMap::new();
        for pixel in validation_sky {
            *validation_by_parent
                .entry(pixel / fine_pixels_per_parent)
                .or_insert(0_usize) += 1;
        }
        for edges in [&a.g_edges, &a.bp_g_edges, &a.g_rp_edges] {
            if edges.len() < 2
                || edges.iter().any(|x| !x.is_finite())
                || edges.windows(2).any(|x| x[0] >= x[1])
            {
                bail!("invalid B/V calibration bin edges");
            }
        }
        let mut cells = BTreeMap::new();
        for cell in a.cells {
            let key = cell.key;
            let uses_bp = matches!(key.route, ColourRoute::Full | ColourRoute::GAndBp);
            let uses_rp = matches!(key.route, ColourRoute::Full | ColourRoute::GAndRp);
            if u64::from(key.sky_pixel) >= pixels
                || cell.training_sources < a.min_training
                || cell.training_sources > a.partitions.training.source_count
                || cell.validation_sources < a.min_validation
                || cell.validation_sources > a.partitions.validation.source_count
                || cell.validation_sky_cells < a.min_validation_sky_cells
                || cell.validation_sky_cells as u64 > cell.validation_sources
                || cell.validation_sky_cells
                    > validation_by_parent
                        .get(&key.sky_pixel)
                        .copied()
                        .unwrap_or(0)
                || key.g_bin >= a.g_edges.len() - 1
                || key.bp_g_bin.is_some() != uses_bp
                || key.g_rp_bin.is_some() != uses_rp
                || key.bp_g_bin.is_some_and(|i| i >= a.bp_g_edges.len() - 1)
                || key.g_rp_bin.is_some_and(|i| i >= a.g_rp_edges.len() - 1)
                || cell
                    .mean_w_m2_nm_at_g15
                    .iter()
                    .any(|x| !x.is_finite() || *x <= 0.0)
                || cell
                    .shared_fraction
                    .iter()
                    .any(|x| !x.is_finite() || *x <= 0.0)
                || cell.residual_covariance_at_g15[..2]
                    .iter()
                    .any(|x| *x <= 0.0)
            {
                bail!("invalid B/V calibration cell");
            }
            check_covariance(cell.residual_covariance_at_g15)?;
            if cells.insert(key, cell).is_some() {
                bail!("duplicate B/V calibration cell");
            }
        }
        if cells
            .keys()
            .map(|key| key.route)
            .collect::<BTreeSet<_>>()
            .len()
            != 4
        {
            bail!("B/V calibration does not support all four photometric routes");
        }
        Ok(Self {
            xp_shared_calibration_fraction: a.xp_shared_calibration_fraction,
            xp_design_sha256: a.xp_design_sha256,
            sky_nside: a.sky_nside,
            g_edges: a.g_edges,
            bp_g_edges: a.bp_g_edges,
            g_rp_edges: a.g_rp_edges,
            cells,
        })
    }

    /// Predict a conditional population mean for the observed photometry.
    /// Missing support fails explicitly, including for G-only sources.
    pub(crate) fn predict(
        &self,
        position: IcrsSkyPosition,
        features: PhotometricFeatures,
    ) -> Result<(BvFlux, BvRoute)> {
        let (route, g, [bp_g, g_rp]) =
            ColourRoute::features(features).context("invalid Gaia photometry for B/V inference")?;
        let sky_pixel = galactic_nested_pixel_from_icrs_position(
            position.ra_deg,
            position.dec_deg,
            self.sky_nside,
        )?;
        let key = CellKey {
            route,
            sky_pixel,
            g_bin: bin(&self.g_edges, g)?,
            bp_g_bin: bp_g.map(|x| bin(&self.bp_g_edges, x)).transpose()?,
            g_rp_bin: g_rp.map(|x| bin(&self.g_rp_edges, x)).transpose()?,
        };
        let cell = self
            .cells
            .get(&key)
            .with_context(|| format!("no calibrated B/V cell for {key:?}"))?;
        let flux = BvFlux {
            flux_w_m2_nm: cell.mean_w_m2_nm_at_g15,
            statistical_covariance: cell.residual_covariance_at_g15,
            independent_systematic_sigma: [0.0; 2],
            shared_systematic_sigma: std::array::from_fn(|i| {
                cell.mean_w_m2_nm_at_g15[i] * cell.shared_fraction[i]
            }),
        }
        .weighted(10_f64.powf(-0.4 * (g - 15.0)));
        flux.validate()?;
        if flux.flux_w_m2_nm.into_iter().any(|x| x <= 0.0) {
            bail!("B/V photometric normalization is out of numeric range");
        }
        // Fully correlated envelope across cells within the same model.
        Ok((flux, route.into()))
    }
}

const SPLIT_ALGORITHM: &str = "sha256-galactic-nested-nside16-mod10-v1";

fn split_index(seed: u64, pixel: u32) -> usize {
    let hex = checksum_io::sha256_bytes(format!("{seed}:{pixel}").as_bytes());
    match u64::from_str_radix(&hex[..16], 16).expect("SHA-256 hex") % 10 {
        0..=5 => 0,
        6..=7 => 1,
        _ => 2,
    }
}

fn calibration_splits(evidence: &PartitionEvidence) -> Result<Vec<u32>> {
    let partitions = [&evidence.training, &evidence.validation, &evidence.test];
    if evidence.assignment_algorithm != SPLIT_ALGORITHM
        || partitions
            .iter()
            .map(|p| &p.source_ids_sha256)
            .collect::<BTreeSet<_>>()
            .len()
            != 3
    {
        bail!("invalid B/V training, validation or test split identity");
    }
    let mut validation = Vec::new();
    for (split, partition) in partitions.into_iter().enumerate() {
        if partition.source_count < partition.sky_regions.len() as u64 {
            bail!("B/V split lists more sky cells than sources");
        }
        for region in &partition.sky_regions {
            let pixel: u32 = region.parse().context("invalid B/V nside-16 sky cell")?;
            if pixel >= 12 * 16 * 16
                || region != &pixel.to_string()
                || split_index(evidence.seed, pixel) != split
            {
                bail!("B/V sky cell {region} disagrees with its assigned split");
            }
            if split == 1 {
                validation.push(pixel);
            }
        }
    }
    Ok(validation)
}

fn bin(edges: &[f64], value: f64) -> Result<usize> {
    if !value.is_finite() || value < edges[0] || value >= edges[edges.len() - 1] {
        bail!("B/V photometry outside calibrated domain");
    }
    Ok(edges.partition_point(|edge| *edge <= value) - 1)
}

pub(crate) fn is_sha256(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|b| b.is_ascii_hexdigit())
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct SignedSum {
    positive: StableSum,
    negative: StableSum,
}

impl SignedSum {
    fn add(&mut self, value: f64) -> Result<()> {
        if value < 0.0 {
            self.negative.add(-value)
        } else {
            self.positive.add(value)
        }
    }

    fn merge(&mut self, other: &Self) -> Result<()> {
        self.positive.merge(&other.positive)?;
        self.negative.merge(&other.negative)
    }

    fn value(&self) -> f64 {
        self.positive.difference(&self.negative)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum BvRoute {
    GaiaXp,
    FullColour,
    GAndBp,
    GAndRp,
    GOnly,
    HipparcosCk04,
}

impl From<ColourRoute> for BvRoute {
    fn from(route: ColourRoute) -> Self {
        match route {
            ColourRoute::Full => Self::FullColour,
            ColourRoute::GAndBp => Self::GAndBp,
            ColourRoute::GAndRp => Self::GAndRp,
            ColourRoute::GOnly => Self::GOnly,
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct BvAccumulator {
    flux: [SignedSum; 2],
    statistical_covariance: [SignedSum; 3],
    independent_systematic_covariance: [StableSum; 3],
    shared_systematic: BTreeMap<String, [StableSum; 2]>,
    counts: BTreeMap<BvRoute, u64>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct BvPixelEvidence {
    pub flux_w_m2_nm: [f64; 2],
    pub statistical_covariance: [f64; 3],
    pub independent_systematic_covariance: [f64; 3],
    pub shared_systematic_sigma: BTreeMap<String, [f64; 2]>,
    pub source_counts: BTreeMap<BvRoute, u64>,
}

impl BvPixelEvidence {
    pub(crate) fn validate(&self, admitted: u64) -> Result<()> {
        check_covariance(self.statistical_covariance)?;
        check_covariance(self.independent_systematic_covariance)?;
        let count = self
            .source_counts
            .values()
            .try_fold(0_u64, |sum, count| sum.checked_add(*count));
        if count != Some(admitted)
            || admitted == 0
            || self.source_counts.values().any(|&n| n == 0)
            || self
                .shared_systematic_sigma
                .keys()
                .any(|group| group.trim().is_empty())
            || self
                .shared_systematic_sigma
                .values()
                .flatten()
                .any(|x| !x.is_finite() || *x < 0.0)
        {
            bail!("B/V evidence has invalid source counts or systematic uncertainties");
        }
        Ok(())
    }
}

impl BvAccumulator {
    pub(crate) fn add(
        &mut self,
        estimate: BvFlux,
        route: BvRoute,
        selection_fraction: f64,
    ) -> Result<()> {
        estimate.validate()?;
        let group = match route {
            BvRoute::GaiaXp => "gaia-xp-bv-calibration",
            BvRoute::HipparcosCk04 => "hipparcos-hp-zero-point-bessell2000",
            _ => "gaia-bv-inference",
        };
        for i in 0..2 {
            self.flux[i].add(estimate.flux_w_m2_nm[i])?;
            self.independent_systematic_covariance[i]
                .add(estimate.independent_systematic_sigma[i].powi(2))?;
        }
        // The supported independent systematic is a common multiplicative
        // shift across both wavelengths of a single CK04 spectrum.
        let [b, v] = estimate.independent_systematic_sigma;
        self.independent_systematic_covariance[2].add(b * v)?;
        for (sum, covariance) in self
            .statistical_covariance
            .iter_mut()
            .zip(estimate.statistical_covariance)
        {
            sum.add(covariance)?;
        }
        self.add_group(group, estimate.shared_systematic_sigma)?;
        if selection_fraction > 0.0 {
            self.add_group(
                "gaia-selection",
                estimate
                    .flux_w_m2_nm
                    .map(|flux| flux.abs() * selection_fraction),
            )?;
        }
        let count = self.counts.entry(route).or_default();
        *count = count.checked_add(1).context("B/V source count overflow")?;
        Ok(())
    }

    fn add_group(&mut self, group: &str, sigmas: [f64; 2]) -> Result<()> {
        let sums = match self.shared_systematic.get_mut(group) {
            Some(sums) => sums,
            None => self.shared_systematic.entry(group.to_owned()).or_default(),
        };
        for (sum, sigma) in sums.iter_mut().zip(sigmas) {
            sum.add(sigma)?;
        }
        Ok(())
    }

    pub(crate) fn merge(&mut self, other: &Self) -> Result<()> {
        for i in 0..2 {
            self.flux[i].merge(&other.flux[i])?;
        }
        for (sum, other) in self
            .independent_systematic_covariance
            .iter_mut()
            .zip(&other.independent_systematic_covariance)
        {
            sum.merge(other)?;
        }
        for (sum, other) in self
            .statistical_covariance
            .iter_mut()
            .zip(&other.statistical_covariance)
        {
            sum.merge(other)?;
        }
        for (group, other) in &other.shared_systematic {
            let sums = self.shared_systematic.entry(group.clone()).or_default();
            for (sum, other) in sums.iter_mut().zip(other) {
                sum.merge(other)?;
            }
        }
        for (route, other) in &other.counts {
            let count = self.counts.entry(*route).or_default();
            *count = count
                .checked_add(*other)
                .context("B/V source count overflow")?;
        }
        Ok(())
    }

    pub(crate) fn validate(&self, admitted: u64) -> Result<()> {
        let count = self
            .counts
            .values()
            .try_fold(0_u64, |acc, count| acc.checked_add(*count));
        if count != Some(admitted) || admitted == 0 || self.counts.values().any(|&n| n == 0) {
            bail!("B/V source coverage disagrees with integrated admission");
        }
        for sum in self.flux.iter().chain(&self.statistical_covariance) {
            sum.positive.validate()?;
            sum.negative.validate()?;
        }
        for sum in self
            .independent_systematic_covariance
            .iter()
            .chain(self.shared_systematic.values().flatten())
        {
            sum.validate()?;
        }
        check_covariance(self.statistical_covariance.each_ref().map(SignedSum::value))?;
        check_covariance(
            self.independent_systematic_covariance
                .each_ref()
                .map(StableSum::value),
        )
    }

    pub(crate) fn evidence(&self) -> BvPixelEvidence {
        BvPixelEvidence {
            flux_w_m2_nm: self.flux.each_ref().map(SignedSum::value),
            statistical_covariance: self.statistical_covariance.each_ref().map(SignedSum::value),
            independent_systematic_covariance: self
                .independent_systematic_covariance
                .each_ref()
                .map(StableSum::value),
            shared_systematic_sigma: self
                .shared_systematic
                .iter()
                .map(|(group, sums)| (group.clone(), sums.each_ref().map(StableSum::value)))
                .collect(),
            source_counts: self.counts.clone(),
        }
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    pub(crate) fn calibration_fixture(
        sky_pixels: &[u32],
        design_sha256: &str,
    ) -> serde_json::Value {
        use serde_json::json;
        let routes = [
            ("full", Some(0), Some(0)),
            ("g_and_bp", Some(0), None),
            ("g_and_rp", None, Some(0)),
            ("g_only", None, None),
        ];
        let cells: Vec<_> = sky_pixels.iter().flat_map(|&pixel|
            routes.map(|(route, bp, rp)| json!({
                "key": {"route": route, "sky_pixel": pixel, "g_bin": 0, "bp_g_bin": bp, "g_rp_bin": rp},
                "mean_w_m2_nm_at_g15": if pixel % 2 == 0 { [1e-14, 2e-14] } else { [4e-14, 1e-14] },
                "residual_covariance_at_g15": [3e-28, 2e-28, -1e-28],
                "shared_fraction": [0.05, 0.05], "training_sources": 2,
                "validation_sources": 2, "validation_sky_cells": 2
            }))
        ).collect();
        let split = |index, id| {
            let sky: Vec<_> = sky_pixels
                .iter()
                .flat_map(|&pixel| {
                    (pixel * 256..(pixel + 1) * 256)
                        .filter(move |&p| split_index(0, p) == index)
                        .take(2)
                        .map(|p| p.to_string())
                })
                .collect();
            json!({"partition_id": id, "source_count": sky.len(), "sky_regions": sky,
                "source_ids_sha256": checksum_io::sha256_bytes(format!("{index}").as_bytes())})
        };
        json!({
            "schema_version": 1, "calibration_status": "candidate",
            "reference_provenance": "synthetic labels for CI only",
            "policy_sha256": "b".repeat(64), "min_training": 2,
            "min_validation": 2, "min_validation_sky_cells": 2,
            "reference_nm": [445.0, 551.0], "sky_nside": 1,
            "g_edges": [10.0, 18.0], "bp_g_edges": [-1.0, 4.0], "g_rp_edges": [-1.0, 4.0],
            "cells": cells, "xp_shared_calibration_fraction": [0.01, 0.01],
            "xp_design_sha256": design_sha256, "held_out_report_sha256": "b".repeat(64),
            "partitions": {
                "assignment_algorithm": SPLIT_ALGORITHM, "seed": 0, "manifest_sha256": "a".repeat(64),
                "training": split(0, "training"), "validation": split(1, "validation"), "test": split(2, "test"),
                "source_disjoint": true, "sky_disjoint": true, "disjointness_evidence_sha256": "c".repeat(64)
            },
            "software_commit": "a".repeat(40), "training_command": "synthetic test only"
        })
    }

    #[test]
    fn g_only_inference_requires_a_spatially_calibrated_cell_and_retains_scatter() {
        let pos = IcrsSkyPosition::new(0.0, 0.0).unwrap();
        let sky_pixel = galactic_nested_pixel_from_icrs_position(0.0, 0.0, 1).unwrap();
        let mut cells = BTreeMap::new();
        for route in [
            ColourRoute::Full,
            ColourRoute::GAndBp,
            ColourRoute::GAndRp,
            ColourRoute::GOnly,
        ] {
            let key = CellKey {
                route,
                sky_pixel,
                g_bin: 0,
                bp_g_bin: matches!(route, ColourRoute::Full | ColourRoute::GAndBp).then_some(0),
                g_rp_bin: matches!(route, ColourRoute::Full | ColourRoute::GAndRp).then_some(0),
            };
            cells.insert(
                key,
                Cell {
                    key,
                    mean_w_m2_nm_at_g15: [4.0, 2.0],
                    residual_covariance_at_g15: [4.0, 1.0, -1.0],
                    shared_fraction: [0.25, 0.5],
                    training_sources: 2,
                    validation_sources: 2,
                    validation_sky_cells: 2,
                },
            );
        }
        let model = BvInference {
            xp_design_sha256: "b".repeat(64),
            xp_shared_calibration_fraction: [0.01; 2],
            sky_nside: 1,
            g_edges: vec![9.0, 18.0],
            bp_g_edges: vec![-1.0, 2.0],
            g_rp_edges: vec![-1.0, 2.0],
            cells,
        };
        for (bp, rp, route) in [
            (Some(15.0), Some(14.0), BvRoute::FullColour),
            (Some(15.0), None, BvRoute::GAndBp),
            (None, Some(14.0), BvRoute::GAndRp),
            (None, None, BvRoute::GOnly),
        ] {
            let features = PhotometricFeatures {
                phot_g_mean_mag: Some(15.0),
                phot_bp_mean_mag: bp,
                phot_rp_mean_mag: rp,
                bp_rp: None,
                quality_flag: true,
            };
            let (estimate, evaluated_route) = model.predict(pos, features).unwrap();
            assert_eq!(evaluated_route, route);
            assert_eq!(estimate.flux_w_m2_nm, [4.0, 2.0]);
            assert_eq!(estimate.statistical_covariance, [4.0, 1.0, -1.0]);
            assert_eq!(estimate.shared_systematic_sigma, [1.0, 1.0]);
            let different_sky = IcrsSkyPosition::new(180.0, 0.0).unwrap();
            assert!(model.predict(different_sky, features).is_err());
        }
    }

    #[test]
    fn rejects_invalid_calibration_identity_support_and_covariance() -> Result<()> {
        use serde_json::json;

        let temp = tempfile::tempdir()?;
        let path = temp.path().join("model.json");
        let model = calibration_fixture(&[0], &"d".repeat(64));
        let load = |data: &serde_json::Value| {
            let bytes = serde_json::to_vec(data).unwrap();
            fs::write(&path, &bytes).unwrap();
            BvInference::load(&path, &checksum_io::sha256_bytes(&bytes))
        };
        assert!(load(&model).is_ok());
        assert!(BvInference::load(&path, &"0".repeat(64)).is_err());
        assert!(BvInference::load(&temp.path().join("missing"), "").is_err());

        for (key, value, error) in [
            ("/calibration_status", json!("validated"), "identity"),
            ("/bp_g_edges", json!([0, 0]), "bin edges"),
            (
                "/cells/0/validation_sky_cells",
                json!(1),
                "invalid B/V calibration cell",
            ),
            (
                "/cells/0/validation_sources",
                json!(3),
                "invalid B/V calibration cell",
            ),
            (
                "/cells/0/key/bp_g_bin",
                json!(null),
                "invalid B/V calibration cell",
            ),
            (
                "/cells/0/key/sky_pixel",
                json!(12),
                "invalid B/V calibration cell",
            ),
            (
                "/cells/0/residual_covariance_at_g15/2",
                json!(1.0),
                "covariance",
            ),
            (
                "/partitions/assignment_algorithm",
                json!("random"),
                "split identity",
            ),
            (
                "/partitions/test/sky_regions/0",
                json!("3072"),
                "assigned split",
            ),
            (
                "/partitions/test/sky_regions/0",
                json!("invalid-pixel"),
                "invalid B/V nside-16",
            ),
            (
                "/partitions/test/source_count",
                json!(1),
                "more sky cells than sources",
            ),
        ] {
            let mut invalid = model.clone();
            *invalid.pointer_mut(key).unwrap() = value;
            let actual = format!("{:#}", load(&invalid).unwrap_err());
            assert!(actual.contains(error), "{key}: {actual}");
        }

        let mut invalid = model.clone();
        invalid["cells"].as_array_mut().unwrap().pop();
        assert!(load(&invalid).unwrap_err().to_string().contains("all four"));
        let cell = invalid["cells"][0].clone();
        invalid["cells"].as_array_mut().unwrap().push(cell);
        assert!(load(&invalid)
            .unwrap_err()
            .to_string()
            .contains("duplicate"));
        assert!(bin(&[10.0, 18.0], 18.0).is_err());
        assert_eq!(bin(&[10.0, 18.0], 10.0)?, 0);
        assert!(ColourRoute::features(PhotometricFeatures {
            phot_g_mean_mag: Some(15.0),
            phot_bp_mean_mag: Some(f64::NAN),
            phot_rp_mean_mag: None,
            bp_rp: None,
            quality_flag: true,
        })
        .is_none());
        Ok(())
    }

    #[test]
    fn signed_and_correlated_contributions_merge_exactly_across_partitions() {
        let make = |b, v| BvFlux {
            flux_w_m2_nm: [b, v],
            statistical_covariance: [4.0, 9.0, -2.0],
            independent_systematic_sigma: [0.0, 0.0],
            shared_systematic_sigma: [1.5, 2.0],
        };
        let mut a = BvAccumulator::default();
        a.add(make(1e30, 1.0), BvRoute::GaiaXp, 0.25).unwrap();
        let mut b = BvAccumulator::default();
        b.add(make(-1e30, 2.0), BvRoute::GaiaXp, 0.25).unwrap();
        b.add(make(1.0, 4.0), BvRoute::GaiaXp, 0.25).unwrap();
        let mut c = b.clone();
        c.merge(&a).unwrap();
        a.merge(&b).unwrap();
        assert_eq!(a, c);
        assert!(a.validate(2).is_err());
        a.validate(3).unwrap();
        let evidence = a.evidence();
        assert_eq!(evidence.flux_w_m2_nm, [1.0, 7.0]);
        assert_eq!(evidence.statistical_covariance, [12.0, 27.0, -6.0]);
        assert_eq!(
            evidence.shared_systematic_sigma["gaia-xp-bv-calibration"],
            [4.5, 6.0]
        );
        assert_eq!(evidence.shared_systematic_sigma["gaia-selection"][1], 1.75);
    }
}
