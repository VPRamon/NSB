// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Vallés Puig, Ramon
//! Re-integrate checksum-pinned original bright-star templates; never merge blindly.
use anyhow::{bail, Context, Result};
use nsb_data_tools::platform::checksum_io;
use nsb_data_tools::starlight::bright_stars::SpectralReconstructionModel;
use nsb_data_tools::starlight::bright_stars::{
    integrate_template_photon_flux, reconstruct_template_band_flux,
};
use std::path::Path;

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().collect();
    if args.len() != 6 {
        bail!("usage: bright_star_interval_audit MODEL MODEL_SHA256 DIAGNOSTICS DIAGNOSTICS_SHA256 OUTPUT");
    }
    for (p, s) in [(&args[1], &args[2]), (&args[3], &args[4])] {
        if checksum_io::sha256_file(Path::new(p))? != *s {
            bail!("input checksum mismatch");
        }
    }
    let model: SpectralReconstructionModel = serde_json::from_slice(&std::fs::read(&args[1])?)?;
    model.validate_for_product_band("combined-300-650")?;
    let diagnostics: serde_json::Value = serde_json::from_slice(&std::fs::read(&args[3])?)?;
    let sources = diagnostics["sources"]
        .as_array()
        .context("source diagnostics")?;
    let mut rows = Vec::new();
    for source in sources {
        let Some(template_id) = source["template_id"].as_str() else {
            continue;
        };
        let template = model
            .templates
            .iter()
            .find(|t| t.template_id == template_id)
            .context("assigned template")?;
        let hp = source["hp_mag"].as_f64().context("Hp magnitude")?;
        let mut bins = Vec::new();
        for i in 0..7 {
            bins.push(reconstruct_template_band_flux(
                template,
                &model.hp_response,
                &model.hp_calibration,
                hp,
                (360.0 + i as f64 * 30.0) * 1e-9,
                (390.0 + i as f64 * 30.0) * 1e-9,
            )?);
        }
        let flux = reconstruct_template_band_flux(
            template,
            &model.hp_response,
            &model.hp_calibration,
            hp,
            360e-9,
            570e-9,
        )?;
        let old = reconstruct_template_band_flux(
            template,
            &model.hp_response,
            &model.hp_calibration,
            hp,
            336e-9,
            650e-9,
        )?;
        let previous = source["flux_336_650_ph_m2_s"]
            .as_f64()
            .context("previous flux")?;
        if (old / previous - 1.0).abs() > 1e-12 {
            bail!(
                "original template reconstruction differs for {}",
                source["hip"]
            );
        }
        let broad = integrate_template_photon_flux(template, 300e-9, 650e-9)?;
        let narrow = integrate_template_photon_flux(template, 360e-9, 570e-9)?;
        if narrow > broad || flux <= 0.0 || !flux.is_finite() {
            bail!("bright-star band invariant failed");
        }
        // Clipped energy-density trapezoids differ slightly when a template
        // segment crosses an internal bin edge. Record that numerical difference.
        let sum: f64 = bins.iter().sum();
        rows.push(
            serde_json::json!({"hip":source["hip"],"gaia_source_id":source["gaia_source_id"],
            "classification":source["class"],"historically_admitted":source["admitted"],
            "template_id":template_id,"flux_360_570_ph_m2_s":flux,"sub_band_flux_ph_m2_s":bins,
            "sub_band_quadrature_relative_difference":sum/flux-1.0,
            "flux_336_650_ph_m2_s":old,"restricted_to_broad_ratio":flux/old,
            "merged_into_xp_candidate":false}),
        );
    }
    let report = serde_json::json!({"schema":"nsb-bright-star-interval-audit-v1","band_nm":[360,570],
        "model_sha256":args[2],"diagnostics_sha256":args[4],"sources":rows,
        "status":"template_reintegration_verified_not_merged",
        "reason":"Gaia replacement reconciliation and template wavelength-convention admission must precede a population-completed interval product",
        "uncertainty_status":model.uncertainty_calibration_status});
    let p = Path::new(&args[5]);
    if p.exists() {
        bail!("new immutable output required");
    }
    std::fs::write(p, serde_json::to_vec_pretty(&report)?)?;
    Ok(())
}
