// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Vallés Puig, Ramon
//! XP-anchored photon-normalized shape priors for research candidates.

use super::{calibration_splits, check_covariance, is_sha256, BvFlux, BvSupport, ColourRoute};
use crate::starlight::healpix::{galactic_nested_pixel_from_icrs_position, IcrsSkyPosition};
use crate::starlight::photometric::PhotometricFluxEstimate;
use crate::starlight::uv::PartitionEvidence;
use crate::starlight::xp::calibrate::BV_REFERENCE_NM;
use anyhow::{bail, Result};
use serde::Deserialize;

const SHAPE_SCALE: f64 = 1e-21; // J nm^-1 per integrated photon
const SKY_PIXELS: usize = 12;

#[cfg_attr(test, derive(serde::Serialize))]
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ShapeCalibration {
    pub schema_version: u32,
    calibration_status: String,
    method: String,
    reference_nm: [f64; 2],
    shape_unit: String,
    pub xp_design_sha256: String,
    pub xp_shared_calibration_fraction: [f64; 2],
    pub photometric_sha256: String,
    software_commit: String,
    references_sha256: String,
    held_out_report_sha256: String,
    partitions: PartitionEvidence,
    g_edges: Vec<f64>,
    xp_g_support: [f64; 2],
    bp_nodes: Vec<f64>,
    rp_nodes: Vec<f64>,
    contexts: Vec<Context>,
}

#[cfg_attr(test, derive(serde::Serialize))]
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Context {
    full: Vec<Moment>,
    g_and_bp: Vec<Moment>,
    g_and_rp: Vec<Moment>,
    g_only: Moment,
}

#[cfg_attr(test, derive(serde::Serialize))]
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Moment {
    mean: [f64; 2],
    covariance: [f64; 3],
    /// Absolute change in mean under alternate XP population priors.
    transfer_sigma: [f64; 2],
    effective_local_sources: f64,
}

impl ShapeCalibration {
    pub(super) fn validate(&self) -> Result<()> {
        self.partitions.validate()?;
        calibration_splits(&self.partitions)?;
        if self.schema_version != 2
            || self.calibration_status != "research_candidate"
            || self.method != "xp-photon-normalized-population-mixture-v1"
            || self.reference_nm != BV_REFERENCE_NM
            || self.shape_unit != "1e-21 J nm-1 photon-1"
            || ![
                &self.xp_design_sha256,
                &self.photometric_sha256,
                &self.held_out_report_sha256,
                &self.references_sha256,
            ]
            .into_iter()
            .all(|s| is_sha256(s))
            || self.software_commit.len() != 40
            || !self.software_commit.bytes().all(|c| c.is_ascii_hexdigit())
            || self
                .xp_shared_calibration_fraction
                .iter()
                .any(|x| !x.is_finite() || *x <= 0.0)
            || [&self.g_edges, &self.bp_nodes, &self.rp_nodes]
                .into_iter()
                .any(|axis| {
                    axis.len() < 2
                        || axis.len() > 64
                        || axis.iter().any(|x| !x.is_finite())
                        || axis.windows(2).any(|w| w[0] >= w[1])
                })
            || !self.xp_g_support.iter().all(|x| x.is_finite())
            || self.xp_g_support[0] >= self.xp_g_support[1]
            || self.contexts.len() != SKY_PIXELS * (self.g_edges.len() - 1)
        {
            bail!("invalid B/V research shape calibration identity or axes");
        }
        let (bp, rp) = (self.bp_nodes.len(), self.rp_nodes.len());
        for context in &self.contexts {
            if context.full.len() != bp * rp
                || context.g_and_bp.len() != bp
                || context.g_and_rp.len() != rp
            {
                bail!("incomplete B/V shape lookup table");
            }
            for m in context
                .full
                .iter()
                .chain(&context.g_and_bp)
                .chain(&context.g_and_rp)
                .chain(std::iter::once(&context.g_only))
            {
                check_covariance(m.covariance)?;
                if m.mean.iter().any(|x| !x.is_finite() || *x <= 0.0)
                    || m.transfer_sigma.iter().any(|x| !x.is_finite() || *x <= 0.0)
                    || m.covariance[..2].iter().any(|x| *x <= 0.0)
                    || !m.effective_local_sources.is_finite()
                    || m.effective_local_sources < 0.0
                {
                    bail!("invalid B/V shape moments or population support");
                }
            }
        }
        Ok(())
    }

    pub(super) fn predict(
        &self,
        position: IcrsSkyPosition,
        route: ColourRoute,
        g: f64,
        colours: [Option<f64>; 2],
        normalization: &PhotometricFluxEstimate,
    ) -> Result<(BvFlux, BvSupport)> {
        let sky = galactic_nested_pixel_from_icrs_position(position.ra_deg, position.dec_deg, 1)?;
        let g_bin = self
            .g_edges
            .partition_point(|x| *x <= g)
            .saturating_sub(1)
            .min(self.g_edges.len() - 2);
        let context = &self.contexts[sky as usize * (self.g_edges.len() - 1) + g_bin];
        let bp = bracket(&self.bp_nodes, colours[0]);
        let rp = bracket(&self.rp_nodes, colours[1]);
        let mut mean = [0.0; 2];
        let mut raw = [0.0; 3];
        let mut transfer = [0.0; 2];
        let mut support = 0.0;
        for (i, a) in bp {
            for (j, b) in rp {
                let w = a * b;
                if w == 0.0 {
                    continue;
                }
                let m = match route {
                    ColourRoute::Full => &context.full[i * self.rp_nodes.len() + j],
                    ColourRoute::GAndBp => &context.g_and_bp[i],
                    ColourRoute::GAndRp => &context.g_and_rp[j],
                    ColourRoute::GOnly => &context.g_only,
                };
                for band in 0..2 {
                    mean[band] += w * m.mean[band];
                    raw[band] += w * (m.covariance[band] + m.mean[band].powi(2));
                    transfer[band] += w * m.transfer_sigma[band];
                }
                raw[2] += w * (m.covariance[2] + m.mean[0] * m.mean[1]);
                support += w * m.effective_local_sources;
            }
        }
        let colour_extrapolated = colours
            .into_iter()
            .zip([&self.bp_nodes, &self.rp_nodes])
            .any(|(x, nodes)| x.is_some_and(|x| x < nodes[0] || x > nodes[nodes.len() - 1]));
        let status = if colour_extrapolated {
            BvSupport::ColourExtrapolation
        } else if g < self.xp_g_support[0] || g > self.xp_g_support[1] {
            BvSupport::MagnitudeExtrapolation
        } else if support < 20.0 {
            BvSupport::SparseXpPrior
        } else if route != ColourRoute::Full {
            BvSupport::MissingColourPrior
        } else {
            BvSupport::XpColourSupport
        };
        let products = [mean[0].powi(2), mean[1].powi(2), mean[0] * mean[1]];
        let photon = normalization.flux_336_650_ph_m2_s * SHAPE_SCALE;
        let stat = (normalization.statistical_uncertainty_336_650_ph_m2_s * SHAPE_SCALE).powi(2);
        let sys = normalization.systematic_uncertainty_336_650_ph_m2_s * SHAPE_SCALE;
        let multiplier = match status {
            BvSupport::XpColourSupport => 1.0,
            BvSupport::MissingColourPrior => 1.5,
            _ => 2.0,
        };
        let estimate = BvFlux {
            flux_w_m2_nm: mean.map(|m| m * photon),
            statistical_covariance: std::array::from_fn(|i| {
                (photon.powi(2) + stat) * (raw[i] - products[i]) + stat * products[i]
            }),
            independent_systematic_sigma: [0.0; 2],
            shared_systematic_sigma: std::array::from_fn(|i| {
                let transport = if multiplier > 1.0 {
                    transfer[i].max((raw[i] - products[i]).max(0.0).sqrt())
                } else {
                    transfer[i]
                };
                sys * mean[i] + photon * transport * multiplier
            }),
        };
        estimate.validate()?;
        if estimate.flux_w_m2_nm.iter().any(|x| *x <= 0.0) {
            bail!("B/V photon-normalized shape is outside numeric range");
        }
        Ok((estimate, status))
    }
}

fn bracket(nodes: &[f64], value: Option<f64>) -> [(usize, f64); 2] {
    let Some(value) = value else {
        return [(0, 1.0), (0, 0.0)];
    };
    let upper = nodes
        .partition_point(|x| *x <= value)
        .clamp(1, nodes.len() - 1);
    let lower = upper - 1;
    let t = ((value - nodes[lower]) / (nodes[upper] - nodes[lower])).clamp(0.0, 1.0);
    [(lower, 1.0 - t), (upper, t)]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::starlight::photometric::PhotometricBranchId;
    use crate::starlight::uv::{ApplicabilityStatus, EvaluationDecision};

    #[test]
    fn photon_scale_preserves_covariance_and_reports_faint_transport() -> Result<()> {
        let moment = || Moment {
            mean: [1.0, 2.0],
            covariance: [1.0, 4.0, -0.5],
            transfer_sigma: [0.25, 0.5],
            effective_local_sources: 30.0,
        };
        let design = "a".repeat(64);
        let legacy = crate::starlight::bv::tests::calibration_fixture(&[0], &design);
        let mut model = ShapeCalibration {
            schema_version: 2,
            calibration_status: "research_candidate".into(),
            method: "xp-photon-normalized-population-mixture-v1".into(),
            reference_nm: BV_REFERENCE_NM,
            shape_unit: "1e-21 J nm-1 photon-1".into(),
            xp_design_sha256: design.clone(),
            xp_shared_calibration_fraction: [0.02; 2],
            photometric_sha256: design.clone(),
            software_commit: "a".repeat(40),
            references_sha256: design.clone(),
            held_out_report_sha256: design,
            partitions: serde_json::from_value(legacy["partitions"].clone())?,
            g_edges: vec![3.0, 22.0],
            xp_g_support: [5.0, 17.65],
            bp_nodes: vec![0.0, 2.0],
            rp_nodes: vec![0.0, 2.0],
            contexts: (0..12)
                .map(|_| Context {
                    full: (0..4).map(|_| moment()).collect(),
                    g_and_bp: (0..2).map(|_| moment()).collect(),
                    g_and_rp: (0..2).map(|_| moment()).collect(),
                    g_only: moment(),
                })
                .collect(),
        };
        model.validate()?;
        let normalization = PhotometricFluxEstimate {
            flux_336_650_ph_m2_s: 2e5,
            statistical_uncertainty_336_650_ph_m2_s: 1e5,
            systematic_uncertainty_336_650_ph_m2_s: 1e4,
            applicability_status: ApplicabilityStatus::InDomain,
            decision: EvaluationDecision::Applied,
            branch: PhotometricBranchId::PhotometricGOnly,
            model_id: "fixture".into(),
            artifact_sha256: "a".repeat(64),
        };
        let pos = IcrsSkyPosition::new(90.0, 0.0)?;
        let (flux, status) =
            model.predict(pos, ColourRoute::Full, 15.0, [Some(1.0); 2], &normalization)?;
        assert_eq!(status, BvSupport::XpColourSupport);
        assert!((flux.flux_w_m2_nm[0] / 2e-16 - 1.0).abs() < 1e-14);
        // E[P²] Cov(R) + Var(P) E[R]E[R]^T, including the signed cross term.
        assert!((flux.statistical_covariance[0] / 6e-32 - 1.0).abs() < 1e-14);
        assert!((flux.statistical_covariance[2] / -5e-33 - 1.0).abs() < 1e-14);
        let (faint, status) =
            model.predict(pos, ColourRoute::GOnly, 20.0, [None; 2], &normalization)?;
        assert_eq!(status, BvSupport::MagnitudeExtrapolation);
        assert!(faint.shared_systematic_sigma[0] > flux.shared_systematic_sigma[0]);
        assert_eq!(
            model
                .predict(pos, ColourRoute::GOnly, 15.0, [None; 2], &normalization)?
                .1,
            BvSupport::MissingColourPrior
        );
        assert_eq!(
            model
                .predict(
                    pos,
                    ColourRoute::GAndBp,
                    15.0,
                    [Some(10.0), None],
                    &normalization
                )?
                .1,
            BvSupport::ColourExtrapolation
        );
        for context in &mut model.contexts {
            for moment in &mut context.g_and_rp {
                moment.effective_local_sources = 0.0;
            }
        }
        assert_eq!(
            model
                .predict(
                    pos,
                    ColourRoute::GAndRp,
                    15.0,
                    [None, Some(1.0)],
                    &normalization
                )?
                .1,
            BvSupport::SparseXpPrior
        );
        let tmp = tempfile::tempdir()?;
        let path = tmp.path().join("shape.json");
        let bytes = serde_json::to_vec(&model)?;
        std::fs::write(&path, &bytes)?;
        let sha = crate::platform::checksum_io::sha256_bytes(&bytes);
        let inference = super::super::BvInference::load(&path, &sha)?;
        assert!(inference.check_normalization_pin(None).is_err());
        let pin = crate::starlight::config::ArtifactPinConfig {
            artifact_path: tmp.path().join("photon.json"),
            sha256: normalization.artifact_sha256.clone(),
        };
        inference.check_normalization_pin(Some(&pin))?;
        let (_, route, status) = inference.predict_admitted(
            pos,
            crate::starlight::photometric::PhotometricFeatures {
                phot_g_mean_mag: Some(20.0),
                phot_bp_mean_mag: None,
                phot_rp_mean_mag: None,
                bp_rp: None,
                quality_flag: true,
            },
            &normalization,
        )?;
        assert_eq!(route, super::super::BvRoute::GOnly);
        assert_eq!(status, Some(BvSupport::MagnitudeExtrapolation));
        let mut first = super::super::BvAccumulator::default();
        let mut second = super::super::BvAccumulator::default();
        first.add(faint, route, 0.0, status)?;
        second.add(faint, route, 0.0, status)?;
        first.merge(&second)?;
        first.validate(2)?;
        let evidence = first.evidence();
        assert_eq!(
            evidence.inference_support[&BvSupport::MagnitudeExtrapolation],
            2
        );
        evidence.validate(2)?;
        model.contexts[0].full.pop();
        assert!(model.validate().is_err());
        Ok(())
    }
}
