//! Versioned physical transformations from published reference units onto
//! top-of-atmosphere Galactic stellar photon radiance integrated over 300–650 nm.
//!
//! These transformations are intentionally conservative. They convert a
//! documented interchange table (Galactic `l`,`b` plus a published brightness)
//! onto the candidate map's HEALPix nside-128 nested pixels. A transformation
//! that cannot isolate direct Galactic starlight from zodiacal, airglow,
//! extragalactic, or diffuse-galactic light is marked [`Admissibility::NotAdmissible`].

use super::regions::pix2ang_nested;
use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use std::f64::consts::PI;
use std::fs;
use std::io::Write;
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
pub const LEINERT_1998_ISL_ANALYTIC_V1: &str = "leinert-1998-isl-analytic-v1";

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

/// Reconstruct Leinert et al. 1998 ISL model brightness in S10 at Galactic
/// `(l, b)` in degrees. Anchors match the published constants: 260 S10 at
/// `(0,0)`, 100 S10 at `l=120/240, b=0`, 50 S10 at `|b|=30`, 20 S10 at `|b|=80`.
pub fn leinert_1998_isl_s10(l_deg: f64, b_deg: f64) -> Result<f64> {
    if !l_deg.is_finite() || !b_deg.is_finite() {
        bail!("Leinert ISL coordinates must be finite");
    }
    let l_rad = l_deg.rem_euclid(360.0).to_radians();
    let abs_b = b_deg.abs();
    let i_eq = 100.0 + 160.0 * l_rad.cos().max(0.0).powi(2);
    let s10 = if abs_b <= 30.0 {
        let t = abs_b / 30.0;
        i_eq * (1.0 - t) + 50.0 * t
    } else if abs_b <= 80.0 {
        let k = 2.5_f64.ln() / 50.0;
        50.0 * (-k * (abs_b - 30.0)).exp()
    } else {
        20.0
    };
    if !s10.is_finite() || s10 <= 0.0 {
        bail!("Leinert ISL reconstruction produced a non-positive S10");
    }
    Ok(s10)
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
    let record = match reference_id {
        "leinert-1998-diffuse-night-sky-brightness" => {
            transform_leinert_isl(acquired_path, output_dir, nside)?
        }
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

fn transform_leinert_isl(
    acquired_path: &Path,
    output_dir: &Path,
    nside: u32,
) -> Result<TransformRecord> {
    let text = fs::read_to_string(acquired_path)
        .with_context(|| format!("read {}", acquired_path.display()))?;
    if !text.contains(LEINERT_1998_ISL_ANALYTIC_V1) {
        bail!(
            "{} does not declare reconstruction {}",
            acquired_path.display(),
            LEINERT_1998_ISL_ANALYTIC_V1
        );
    }
    let spec = s10v_to_photon_300_650_spec();
    let domain = 12_u64 * u64::from(nside) * u64::from(nside);
    let grid_path = output_dir.join("transformed-grid-v1.csv");
    let mut out =
        fs::File::create(&grid_path).with_context(|| format!("create {}", grid_path.display()))?;
    writeln!(out, "pixel,value_ph_m2_s,statistical_uncertainty_ph_m2_s")?;
    for pixel in 0..domain {
        let pixel = pixel as u32;
        let (l, b) = pix2ang_nested(nside, pixel)?;
        let s10 = leinert_1998_isl_s10(l, b)?;
        let flux = s10v_to_pixel_photon_flux(s10, nside)?;
        if flux <= 0.0 {
            bail!("Leinert transform produced non-positive flux at pixel {pixel}");
        }
        let sigma = flux * spec.introduced_relative_uncertainty;
        writeln!(out, "{pixel},{flux:.8e},{sigma:.8e}")?;
    }
    drop(out);
    let sha256 = crate::platform::checksum_io::sha256_file(&grid_path)?;
    Ok(TransformRecord {
        schema_version: TRANSFORM_RECORD_SCHEMA_VERSION,
        reference_id: "leinert-1998-diffuse-night-sky-brightness".to_string(),
        spec_id: format!("{LEINERT_1998_ISL_ANALYTIC_V1}+{}", spec.id),
        admissibility: Admissibility::AdmissibleDirectStarlight,
        detail: format!(
            "evaluated {LEINERT_1998_ISL_ANALYTIC_V1} at nside={nside} nested pixel centres; introduced relative uncertainty {}",
            spec.introduced_relative_uncertainty
        ),
        grid_sha256: Some(sha256),
    })
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
    fn leinert_model_hits_published_anchors() {
        let origin = leinert_1998_isl_s10(0.0, 0.0).unwrap();
        assert!((origin - 260.0).abs() < 1e-9);
        let l120 = leinert_1998_isl_s10(120.0, 0.0).unwrap();
        assert!((l120 - 100.0).abs() < 1e-9);
        let b30 = leinert_1998_isl_s10(0.0, 30.0).unwrap();
        assert!((b30 - 50.0).abs() < 1e-9);
        let b80 = leinert_1998_isl_s10(45.0, 80.0).unwrap();
        assert!((b80 - 20.0).abs() < 1e-9);
    }

    #[test]
    fn toller_and_gambons_are_not_admissible() {
        let temp = tempfile::TempDir::new().unwrap();
        let dummy = temp.path().join("dummy");
        std::fs::write(&dummy, b"x").unwrap();
        let out = temp.path().join("out");
        let toller = transform_acquired_reference(
            "toller-1981-pioneer-background-starlight",
            &dummy,
            &out.join("toller"),
            1,
        )
        .unwrap();
        assert_eq!(toller.admissibility, Admissibility::NotAdmissible);
        assert!(!out.join("toller/transformed-grid-v1.csv").is_file());
        let gambons = transform_acquired_reference(
            "masana-2021-gambons-gaia-hipparcos-starlight",
            &dummy,
            &out.join("gambons"),
            1,
        )
        .unwrap();
        assert_eq!(gambons.admissibility, Admissibility::NotAdmissible);
    }

    #[test]
    fn leinert_writes_an_admissible_nside1_grid() {
        let temp = tempfile::TempDir::new().unwrap();
        let acquired = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(
            "../../docs/nsb_components/starlight/validation/acquired/leinert1998-diffuse-night-sky-brightness.csv",
        );
        let out = temp.path().join("leinert");
        let record = transform_acquired_reference(
            "leinert-1998-diffuse-night-sky-brightness",
            &acquired,
            &out,
            1,
        )
        .unwrap();
        assert_eq!(
            record.admissibility,
            Admissibility::AdmissibleDirectStarlight
        );
        assert!(out.join("transformed-grid-v1.csv").is_file());
        let grid = super::super::transformed_grid::load_if_present(
            &out.join("transformed-grid-v1.csv"),
            1,
        )
        .unwrap()
        .unwrap();
        assert_eq!(grid.pixels.len(), 12);
    }
}
