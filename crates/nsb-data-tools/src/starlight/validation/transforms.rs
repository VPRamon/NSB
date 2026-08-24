//! Versioned physical transformations from published reference units onto
//! top-of-atmosphere Galactic stellar photon radiance integrated over 300–650 nm.
//!
//! These transformations are intentionally conservative. They convert a
//! documented interchange table (Galactic `l`,`b` plus a published brightness)
//! onto the candidate map's HEALPix nside-128 nested pixels. A transformation
//! that cannot isolate direct Galactic starlight from zodiacal, airglow,
//! extragalactic, or diffuse-galactic light is marked [`Admissibility::NotAdmissible`].

use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use std::f64::consts::PI;
use std::fs;
use std::path::Path;

/// Identifier of the currently implemented S10(V) conversion.
pub const S10V_TO_PHOTON_300_650_V1: &str = "s10v-to-ph-300-650-solar-flat-v1";

/// Planck constant times speed of light, J m.
const HC_J_M: f64 = 1.986_445_524_210_5e-25;
/// Mean wavelength used for the first-order photon conversion, m.
const LAMBDA_MEAN_M: f64 = 475e-9;
/// Band width, m.
const BAND_WIDTH_M: f64 = 350e-9;
/// Steradians in one square degree.
const SR_PER_SQ_DEG: f64 = PI * PI / 32_400.0;

/// 1 S10(V) in W m^-2 sr^-1 m^-1 at ~555 nm, from Leinert et al. 1998 Table 2
/// conversion factors (1 S10(V) = 1.26e-8 W m^-2 sr^-1 um^-1).
const S10V_SPECTRAL_RADIANCE_W_M2_SR_M: f64 = 1.26e-2;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Admissibility {
    AdmissibleDirectStarlight,
    NotAdmissible,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransformationSpec {
    pub id: String,
    pub formula: String,
    pub input_band_nm: [f64; 2],
    pub output_band_nm: [f64; 2],
    pub input_units: String,
    pub output_units: String,
    pub spectrum_assumption: String,
    pub terms_removed: Vec<String>,
    pub introduced_relative_uncertainty: f64,
    pub valid_domain: String,
    pub coefficients_sha256: String,
    pub admissibility: Admissibility,
}

/// Documented S10(V) → per-pixel photon-flux conversion used when a reference
/// tabulates integrated starlight in S10(V) units.
pub fn s10v_to_photon_300_650_spec() -> TransformationSpec {
    TransformationSpec {
        id: S10V_TO_PHOTON_300_650_V1.to_string(),
        formula: "F_pixel = S10(V) * L_s10 * Δλ * (pixel_solid_angle) / (hc/λ_mean)".to_string(),
        input_band_nm: [505.0, 595.0],
        output_band_nm: [300.0, 650.0],
        input_units: "S10(V)".to_string(),
        output_units: "ph_m-2_s-1 per nside-128 nested pixel".to_string(),
        spectrum_assumption: "flat spectral radiance at the V-band S10 conversion wavelength, extended across 300-650 nm; this is a first-order colour correction, not a passband convolution against a stellar library".to_string(),
        terms_removed: vec![
            "none automatically — callers must supply starlight-only tables".to_string(),
        ],
        introduced_relative_uncertainty: 0.25,
        valid_domain: "all-sky Galactic (l,b) samples with finite non-negative S10(V)".to_string(),
        coefficients_sha256:
            "s10v=1.26e-8_W_m-2_sr-1_um-1;lambda_mean=475nm;band=350nm;nside=128".to_string(),
        admissibility: Admissibility::AdmissibleDirectStarlight,
    }
}

/// Convert one S10(V) surface brightness into photon flux through one
/// nside-128 nested HEALPix pixel.
pub fn s10v_to_pixel_photon_flux(s10_v: f64, nside: u32) -> Result<f64> {
    if !s10_v.is_finite() || s10_v < 0.0 {
        bail!("S10(V) must be finite and non-negative");
    }
    if nside == 0 || (nside & (nside - 1)) != 0 {
        bail!("nside must be a positive power of two");
    }
    let n_pix = 12.0 * f64::from(nside) * f64::from(nside);
    let pixel_sr = 4.0 * PI / n_pix;
    let spectral = S10V_SPECTRAL_RADIANCE_W_M2_SR_M;
    let energy_flux = s10_v * spectral * BAND_WIDTH_M * pixel_sr;
    let photon_energy = HC_J_M / LAMBDA_MEAN_M;
    let flux = energy_flux / photon_energy;
    if !flux.is_finite() || flux < 0.0 {
        bail!("S10(V) conversion produced a non-finite photon flux");
    }
    Ok(flux)
}

pub const TRANSFORM_RECORD_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TransformRecord {
    pub schema_version: u32,
    pub reference_id: String,
    pub spec_id: String,
    pub admissibility: Admissibility,
    pub detail: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub grid_sha256: Option<String>,
}

/// Apply the versioned transformation for one acquired reference.
///
/// References that cannot isolate Galactic starlight write a
/// [`Admissibility::NotAdmissible`] record and no comparison grid.
pub fn transform_acquired_reference(
    reference_id: &str,
    acquired_path: &Path,
    output_dir: &Path,
    nside: u32,
) -> Result<TransformRecord> {
    fs::create_dir_all(output_dir)
        .with_context(|| format!("create transform output directory {}", output_dir.display()))?;
    let _ = (acquired_path, nside);
    let record = match reference_id {
        "leinert-1998-diffuse-night-sky-brightness" => TransformRecord {
            schema_version: TRANSFORM_RECORD_SCHEMA_VERSION,
            reference_id: reference_id.to_string(),
            spec_id: "none".to_string(),
            admissibility: Admissibility::NotAdmissible,
            detail: "Leinert et al. 1998 describe a two-dimensional Gaussian fitted to Elsässer & Haug (1960) isophotes and quote five S10 anchors. The published paper does not give the Gaussian amplitudes and widths needed to reconstruct that surface. Matching those anchors with an invented interpolation is not the registered model, so this reference is acquired for provenance only and is not an admissible comparison grid.".to_string(),
            grid_sha256: None,
        },
        "toller-1981-pioneer-background-starlight" => TransformRecord {
            schema_version: TRANSFORM_RECORD_SCHEMA_VERSION,
            reference_id: reference_id.to_string(),
            spec_id: "none".to_string(),
            admissibility: Admissibility::NotAdmissible,
            detail: "Pioneer 10 Galactic-pole photometry measures ISL+DGL+EBL; diffuse galactic light is inseparable from discrete starlight in the 2.3 deg FOV. Acquired for provenance only.".to_string(),
            grid_sha256: None,
        },
        "masana-2021-gambons-gaia-hipparcos-starlight" => TransformRecord {
            schema_version: TRANSFORM_RECORD_SCHEMA_VERSION,
            reference_id: reference_id.to_string(),
            spec_id: "none".to_string(),
            admissibility: Admissibility::NotAdmissible,
            detail: "GAMBONS all-sky products mix Gaia/Hipparcos ISL with DGL, EBL, zodiacal light and airglow. Not an admissible TOA Galactic starlight-only 300-650 nm grid.".to_string(),
            grid_sha256: None,
        },
        other => bail!("no versioned transformation is registered for reference {other}"),
    };
    let record_path = output_dir.join("transform-status-v1.json");
    fs::write(&record_path, serde_json::to_vec_pretty(&record)?)
        .with_context(|| format!("write {}", record_path.display()))?;
    Ok(record)
}

/// Solid angle of one square degree, exposed so tests can reconstruct the
/// conversion by hand.
pub fn steradians_per_square_degree() -> f64 {
    SR_PER_SQ_DEG
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zero_s10_is_zero_flux() {
        assert_eq!(s10v_to_pixel_photon_flux(0.0, 128).unwrap(), 0.0);
    }

    #[test]
    fn one_s10_at_nside128_is_hand_checkable() {
        let got = s10v_to_pixel_photon_flux(1.0, 128).unwrap();
        let n_pix = 12.0 * 128.0 * 128.0;
        let pixel_sr = 4.0 * PI / n_pix;
        let expected = (1.26e-2 * 350e-9 * pixel_sr) / (HC_J_M / LAMBDA_MEAN_M);
        assert!((got - expected).abs() / expected < 1e-12);
        assert!(got > 0.0);
    }

    #[test]
    fn two_equal_independent_s10_values_scale_linearly() {
        let one = s10v_to_pixel_photon_flux(10.0, 128).unwrap();
        let two = s10v_to_pixel_photon_flux(20.0, 128).unwrap();
        assert!((two - 2.0 * one).abs() / two < 1e-12);
    }

    #[test]
    fn rejects_negative_s10() {
        assert!(s10v_to_pixel_photon_flux(-1.0, 128).is_err());
    }

    #[test]
    fn spec_declares_admissible_starlight_only() {
        let spec = s10v_to_photon_300_650_spec();
        assert_eq!(spec.id, S10V_TO_PHOTON_300_650_V1);
        assert_eq!(spec.admissibility, Admissibility::AdmissibleDirectStarlight);
        assert!(spec.introduced_relative_uncertainty > 0.0);
    }

    #[test]
    fn acquired_literature_references_are_not_admissible_without_a_published_surface() {
        let temp = tempfile::TempDir::new().unwrap();
        let dummy = temp.path().join("dummy");
        std::fs::write(&dummy, b"x").unwrap();
        let out = temp.path().join("out");
        for id in [
            "toller-1981-pioneer-background-starlight",
            "masana-2021-gambons-gaia-hipparcos-starlight",
            "leinert-1998-diffuse-night-sky-brightness",
        ] {
            let record = transform_acquired_reference(id, &dummy, &out.join(id), 1).unwrap();
            assert_eq!(record.admissibility, Admissibility::NotAdmissible);
            assert!(!out.join(id).join("transformed-grid-v1.csv").is_file());
        }
    }
}
