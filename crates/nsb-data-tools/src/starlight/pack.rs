//! Deterministic packing of a frozen candidate-v5 map into a runtime HEALPix CSV.
//!
//! The candidate bytes are never rewritten. Omitted sparse pixels become zero
//! radiance and zero uncertainty. B/V S10 diagnostics are not synthesized.

use crate::platform::checksum_io;
use crate::starlight::validation::candidate_map::{self, CandidateMap};
use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::f64::consts::PI;
use std::fmt::Write as _;
use std::fs;
use std::path::PathBuf;

/// Packer identity recorded in the runtime sidecar.
pub const PACKER_ID: &str = "candidate-v5-to-healpix-v2-packed-v1";
/// Linear conversion from per-pixel `ph m-2 s-1` onto `ph cm-2 ns-1 sr-1`,
/// plus NESTED→RING index conversion required by the runtime HEALPix loader.
pub const UNIT_CONVERSION_ID: &str =
    "pixel-flux-ph-m2-s-to-band-radiance-ph-cm2-ns-sr-v1+nested-to-ring-v1";
/// Sidecar schema for a packed, not-yet-production runtime map.
pub const PACK_SIDECAR_SCHEMA: &str = "nsb-starlight-runtime-pack-v1";
const PACK_SIDECAR_SCHEMA_VERSION: u32 = 1;
const FLUX_CONSERVATION_RELATIVE_TOLERANCE: f64 = 1.0e-12;
/// `ph m-2 s-1 sr-1` → `ph cm-2 ns-1 sr-1` (1 m^2 = 1e4 cm^2, 1 s = 1e9 ns).
const PH_M2_S_SR_TO_PH_CM2_NS_SR: f64 = 1.0e-13;

/// Inputs for [`pack_candidate_map`].
#[derive(Debug, Clone)]
pub struct PackInputs {
    pub candidate_map: PathBuf,
    pub expected_candidate_sha256: String,
    pub expected_nside: u32,
    pub output_csv: PathBuf,
    pub output_sidecar: PathBuf,
    pub provenance_headers: BTreeMap<String, String>,
}

/// Result of a successful pack.
#[derive(Debug, Clone)]
pub struct PackOutcome {
    pub candidate_sha256: String,
    pub runtime_map_sha256: String,
    pub runtime_sidecar_sha256: String,
    pub occupied_pixels: u64,
    pub omitted_pixels: u64,
    pub all_sky_flux_sum_ph_m2_s: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PackSidecar {
    pub schema_version: u32,
    pub schema: String,
    pub packer_id: String,
    pub unit_conversion_id: String,
    pub source_candidate_sha256: String,
    pub runtime_map_sha256: String,
    pub nside: u32,
    pub ordering: String,
    pub s10_diagnostics: String,
    pub occupied_pixels: u64,
    pub omitted_pixels_filled_zero: u64,
    pub all_sky_flux_sum_ph_m2_s: f64,
    pub flux_conservation_relative_tolerance: f64,
}

/// Pack a checksum-pinned candidate-v5 CSV into a dense runtime HEALPix CSV.
pub fn pack_candidate_map(inputs: &PackInputs) -> Result<PackOutcome> {
    let candidate = candidate_map::load(
        &inputs.candidate_map,
        inputs.expected_nside,
        Some(inputs.expected_candidate_sha256.as_str()),
    )?;
    if candidate.nside != inputs.expected_nside {
        bail!(
            "candidate nside {} does not match expected {}",
            candidate.nside,
            inputs.expected_nside
        );
    }

    let packed = render_packed_csv(&candidate, &inputs.provenance_headers)?;
    if let Some(parent) = inputs.output_csv.parent() {
        if !parent.as_os_str().is_empty() {
            fs::create_dir_all(parent).with_context(|| format!("create {}", parent.display()))?;
        }
    }
    fs::write(&inputs.output_csv, packed.as_bytes())
        .with_context(|| format!("write packed map {}", inputs.output_csv.display()))?;
    let runtime_map_sha256 = checksum_io::sha256_file(&inputs.output_csv)?;

    let npix = 12_u64 * u64::from(candidate.nside) * u64::from(candidate.nside);
    let occupied = u64::try_from(candidate.pixels.len()).expect("pixel count fits u64");
    let omitted = npix
        .checked_sub(occupied)
        .context("occupied exceeds domain")?;
    let flux_sum: f64 = candidate
        .pixels
        .values()
        .map(|pixel| pixel.flux_ph_m2_s)
        .sum();

    let sidecar = PackSidecar {
        schema_version: PACK_SIDECAR_SCHEMA_VERSION,
        schema: PACK_SIDECAR_SCHEMA.to_string(),
        packer_id: PACKER_ID.to_string(),
        unit_conversion_id: UNIT_CONVERSION_ID.to_string(),
        source_candidate_sha256: candidate.sha256.clone(),
        runtime_map_sha256: runtime_map_sha256.clone(),
        nside: candidate.nside,
        ordering: "ring".to_string(),
        s10_diagnostics: "not_provided".to_string(),
        occupied_pixels: occupied,
        omitted_pixels_filled_zero: omitted,
        all_sky_flux_sum_ph_m2_s: flux_sum,
        flux_conservation_relative_tolerance: FLUX_CONSERVATION_RELATIVE_TOLERANCE,
    };
    let sidecar_toml = toml::to_string_pretty(&sidecar).context("serialize pack sidecar")?;
    if let Some(parent) = inputs.output_sidecar.parent() {
        if !parent.as_os_str().is_empty() {
            fs::create_dir_all(parent).with_context(|| format!("create {}", parent.display()))?;
        }
    }
    fs::write(&inputs.output_sidecar, sidecar_toml.as_bytes())
        .with_context(|| format!("write pack sidecar {}", inputs.output_sidecar.display()))?;
    let runtime_sidecar_sha256 = checksum_io::sha256_file(&inputs.output_sidecar)?;

    Ok(PackOutcome {
        candidate_sha256: candidate.sha256,
        runtime_map_sha256,
        runtime_sidecar_sha256,
        occupied_pixels: occupied,
        omitted_pixels: omitted,
        all_sky_flux_sum_ph_m2_s: flux_sum,
    })
}

fn render_packed_csv(
    candidate: &CandidateMap,
    provenance_headers: &BTreeMap<String, String>,
) -> Result<String> {
    let nside = candidate.nside;
    let npix = 12_u64 * u64::from(nside) * u64::from(nside);
    let omega = pixel_solid_angle_sr(nside);
    let mut sum_in = 0.0;
    let mut sum_out = 0.0;
    let mut out = String::new();
    out.push_str("# schema=nsb-healpix-starlight-v2\n");
    out.push_str("# packer_id=");
    out.push_str(PACKER_ID);
    out.push('\n');
    out.push_str("# unit_conversion_id=");
    out.push_str(UNIT_CONVERSION_ID);
    out.push('\n');
    out.push_str("# source_candidate_sha256=");
    out.push_str(&candidate.sha256);
    out.push('\n');
    out.push_str("# map_type=healpix\n");
    out.push_str("# coordinate_frame=galactic\n");
    out.push_str(&format!("# nside={nside}\n"));
    out.push_str("# ordering=ring\n");
    out.push_str("# s10_diagnostics=not_provided\n");
    for (key, value) in provenance_headers {
        if matches!(
            key.as_str(),
            "schema"
                | "packer_id"
                | "unit_conversion_id"
                | "source_candidate_sha256"
                | "map_type"
                | "coordinate_frame"
                | "nside"
                | "ordering"
                | "s10_diagnostics"
        ) {
            continue;
        }
        writeln!(out, "# {key}={value}").expect("write packed header");
    }
    out.push_str(
        "healpix_index,integrated_ph_cm2_ns_sr,statistical_uncertainty_ph_cm2_ns_sr,systematic_uncertainty_ph_cm2_ns_sr,total_uncertainty_ph_cm2_ns_sr\n",
    );

    let mut dense = vec![(0.0, 0.0, 0.0, 0.0); usize::try_from(npix).expect("npix fits usize")];
    for (nested_index, pixel) in &candidate.pixels {
        let ring = nest2ring(nside, u64::from(*nested_index))?;
        let slot = usize::try_from(ring).expect("ring index fits usize");
        dense[slot] = (
            pixel.flux_ph_m2_s,
            pixel.statistical_uncertainty_ph_m2_s,
            pixel.systematic_uncertainty_ph_m2_s,
            pixel.total_uncertainty_ph_m2_s,
        );
    }

    for (index, (flux, statistical, systematic, total)) in dense.into_iter().enumerate() {
        if !flux.is_finite() || flux < 0.0 {
            bail!("ring pixel {index} has a non-finite or negative flux");
        }
        let radiance = flux_to_runtime_radiance(flux, omega)?;
        let statistical_r = flux_to_runtime_radiance(statistical, omega)?;
        let systematic_r = flux_to_runtime_radiance(systematic, omega)?;
        let total_r = flux_to_runtime_radiance(total, omega)?;
        sum_in += flux;
        sum_out += radiance * omega / PH_M2_S_SR_TO_PH_CM2_NS_SR;
        writeln!(
            out,
            "{index},{radiance:.16e},{statistical_r:.16e},{systematic_r:.16e},{total_r:.16e}"
        )
        .expect("write packed row");
    }

    let scale = sum_in.abs().max(sum_out.abs()).max(1.0);
    let relative = (sum_in - sum_out).abs() / scale;
    if relative > FLUX_CONSERVATION_RELATIVE_TOLERANCE {
        bail!(
            "packed map failed flux conservation: input {sum_in}, reconstructed {sum_out}, relative error {relative}"
        );
    }
    Ok(out)
}

fn pixel_solid_angle_sr(nside: u32) -> f64 {
    4.0 * PI / (12.0 * f64::from(nside) * f64::from(nside))
}

fn flux_to_runtime_radiance(flux_ph_m2_s: f64, pixel_sr: f64) -> Result<f64> {
    if !pixel_sr.is_finite() || pixel_sr <= 0.0 {
        bail!("pixel solid angle must be finite and positive");
    }
    let radiance = (flux_ph_m2_s / pixel_sr) * PH_M2_S_SR_TO_PH_CM2_NS_SR;
    if !radiance.is_finite() || radiance < 0.0 {
        bail!("unit conversion produced a non-finite radiance");
    }
    Ok(radiance)
}

/// Convert a NESTED HEALPix index to RING. Algorithm from the HEALPix primer
/// (`xyf2ring` / nested bit de-interleave).
fn nest2ring(nside: u32, ipnest: u64) -> Result<u64> {
    let nside = i64::from(nside);
    let npix = 12 * nside * nside;
    let ipnest = i64::try_from(ipnest).context("nested index fits i64")?;
    if ipnest < 0 || ipnest >= npix {
        bail!("nested pixel {ipnest} is outside the nside={nside} domain");
    }
    const JRLL: [i64; 12] = [2, 2, 2, 2, 3, 3, 3, 3, 4, 4, 4, 4];
    const JPLL: [i64; 12] = [1, 3, 5, 7, 0, 2, 4, 6, 1, 3, 5, 7];
    let npface = nside * nside;
    let face = ipnest / npface;
    let ipf = ipnest % npface;
    let mut ix = 0i64;
    let mut iy = 0i64;
    for bit in 0..16 {
        ix |= ((ipf >> (2 * bit)) & 1) << bit;
        iy |= ((ipf >> (2 * bit + 1)) & 1) << bit;
    }
    let jr = JRLL[usize::try_from(face).expect("face fits")] * nside - ix - iy - 1;
    let nl4 = 4 * nside;
    let ncap = 2 * nside * (nside - 1);
    let (nr, kshift, n_before) = if jr < nside {
        let nr = jr;
        (nr, 0, 2 * nr * (nr - 1))
    } else if jr > 3 * nside {
        let nr = nl4 - jr;
        (nr, 0, npix - 2 * (nr + 1) * nr)
    } else {
        let nr = nside;
        let kshift = (jr - nside) & 1;
        (nr, kshift, ncap + (jr - nside) * nl4)
    };
    let mut jp = JPLL[usize::try_from(face).expect("face fits")]
        .saturating_mul(nr)
        .saturating_add(ix)
        .saturating_sub(iy)
        .saturating_add(1)
        .saturating_add(kshift)
        / 2;
    jp %= nl4;
    if jp < 1 {
        jp += nl4;
    }
    u64::try_from(n_before + jp - 1).context("ring index fits u64")
}

/// First non-comment data header of a packed runtime CSV.
pub fn packed_data_header() -> &'static str {
    "healpix_index,integrated_ph_cm2_ns_sr,statistical_uncertainty_ph_cm2_ns_sr,systematic_uncertainty_ph_cm2_ns_sr,total_uncertainty_ph_cm2_ns_sr"
}

pub fn is_packed_runtime_header(header: &str) -> bool {
    header.trim() == packed_data_header()
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    const HEADER: &str = concat!(
        "# schema=nsb-healpix-starlight-candidate-v5\n",
        "# ordering=nested\n",
        "# representation=sparse\n",
        "# nside=1\n",
        "# flux_unit=ph_m-2_s-1\n",
        "pixel,flux_ph_m2_s,statistical_uncertainty_ph_m2_s,systematic_uncertainty_ph_m2_s,total_uncertainty_ph_m2_s,admitted_sources,excluded_sources\n",
    );

    fn write_candidate(dir: &TempDir, body: &str) -> PathBuf {
        let path = dir.path().join("candidate.csv");
        fs::write(&path, body).unwrap();
        path
    }

    #[test]
    fn packs_sparse_nside1_and_fills_omitted_pixel() {
        let dir = TempDir::new().unwrap();
        let body = format!("{HEADER}0,1.0,0.1,0.2,0.25,5,1\n");
        let candidate = write_candidate(&dir, &body);
        let sha = checksum_io::sha256_file(&candidate).unwrap();
        let csv = dir.path().join("map.release.csv");
        let sidecar = dir.path().join("map.pack.toml");
        let outcome = pack_candidate_map(&PackInputs {
            candidate_map: candidate,
            expected_candidate_sha256: sha,
            expected_nside: 1,
            output_csv: csv.clone(),
            output_sidecar: sidecar.clone(),
            provenance_headers: BTreeMap::new(),
        })
        .unwrap();
        assert_eq!(outcome.occupied_pixels, 1);
        assert_eq!(outcome.omitted_pixels, 11);
        let packed = fs::read_to_string(&csv).unwrap();
        assert!(packed.contains("s10_diagnostics=not_provided"));
        let rows: Vec<&str> = packed
            .lines()
            .filter(|line| !line.starts_with('#') && !line.starts_with("healpix_index"))
            .collect();
        assert_eq!(rows.len(), 12);
        assert_eq!(
            rows.iter()
                .filter(|row| !row.contains(",0.0000000000000000e0,"))
                .count(),
            1
        );
        let record: PackSidecar = toml::from_str(&fs::read_to_string(&sidecar).unwrap()).unwrap();
        assert_eq!(record.packer_id, PACKER_ID);
        assert_eq!(record.source_candidate_sha256, outcome.candidate_sha256);

        let packed = fs::read_to_string(&csv).unwrap();
        let map =
            nsb::StarlightMap::from_csv_str(&packed, nsb::StarlightProvenance::test_fixture())
                .unwrap();
        assert_eq!(map.pixels().len(), 12);
        let occupied = map.lookup(map.pixels()[0].galactic_lon, map.pixels()[0].galactic_lat);
        assert!(occupied.integrated.value() > 0.0);
        assert!(!occupied.s10_diagnostics_provided);
        let zero = map
            .pixels()
            .iter()
            .find(|pixel| pixel.integrated.value() == 0.0)
            .unwrap();
        assert_eq!(zero.total_uncertainty.unwrap().value(), 0.0);
    }

    #[test]
    fn duplicate_pixel_index_fails_closed() {
        let dir = TempDir::new().unwrap();
        let candidate = write_candidate(
            &dir,
            &format!("{HEADER}0,1.0,0.1,0.2,0.25,5,1\n0,1.0,0.1,0.2,0.25,5,1\n"),
        );
        let sha = checksum_io::sha256_file(&candidate).unwrap();
        assert!(pack_candidate_map(&PackInputs {
            candidate_map: candidate,
            expected_candidate_sha256: sha,
            expected_nside: 1,
            output_csv: dir.path().join("out.csv"),
            output_sidecar: dir.path().join("out.toml"),
            provenance_headers: BTreeMap::new(),
        })
        .is_err());
    }

    #[test]
    fn nest2ring_is_a_bijection_for_nside_1_and_2() {
        for nside in [1_u32, 2] {
            let npix = 12 * u64::from(nside) * u64::from(nside);
            let mut seen = std::collections::BTreeSet::new();
            for nest in 0..npix {
                let ring = nest2ring(nside, nest).unwrap();
                assert!(ring < npix);
                assert!(seen.insert(ring));
            }
            assert_eq!(seen.len(), usize::try_from(npix).unwrap());
        }
    }

    #[test]
    fn checksum_drift_fails_closed() {
        let dir = TempDir::new().unwrap();
        let candidate = write_candidate(&dir, &format!("{HEADER}0,1.0,0.1,0.2,0.25,5,1\n"));
        let error = pack_candidate_map(&PackInputs {
            candidate_map: candidate,
            expected_candidate_sha256: "a".repeat(64),
            expected_nside: 1,
            output_csv: dir.path().join("out.csv"),
            output_sidecar: dir.path().join("out.toml"),
            provenance_headers: BTreeMap::new(),
        })
        .unwrap_err();
        assert!(error.to_string().contains("checksum mismatch"));
    }

    #[test]
    fn missing_uncertainty_or_negative_flux_fails_closed() {
        let dir = TempDir::new().unwrap();
        let candidate = write_candidate(&dir, &format!("{HEADER}0,-1.0,0.1,0.2,0.25,5,1\n"));
        let sha = checksum_io::sha256_file(&candidate).unwrap();
        assert!(pack_candidate_map(&PackInputs {
            candidate_map: candidate,
            expected_candidate_sha256: sha,
            expected_nside: 1,
            output_csv: dir.path().join("out.csv"),
            output_sidecar: dir.path().join("out.toml"),
            provenance_headers: BTreeMap::new(),
        })
        .is_err());
    }
}
