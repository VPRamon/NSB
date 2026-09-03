//! Deterministic packing of a frozen candidate-v5 map into a runtime HEALPix CSV.
//!
//! The candidate bytes are never rewritten. Omitted sparse pixels become zero
//! radiance and zero uncertainty. B/V S10 diagnostics are not synthesized.

use crate::platform::checksum_io;
use crate::starlight::validation::candidate_map::{self, CandidateMap};
use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use siderust::healpix::{HealpixGrid, HealpixOrdering, Nside};
use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::fs;
use std::path::PathBuf;

/// Packer identity recorded in the runtime sidecar.
pub const PACKER_ID: &str = "candidate-v5-to-healpix-v2-packed-v1";
/// Frozen UV-v2 candidate SHA-256.
pub const CANONICAL_CANDIDATE_SHA256: &str =
    "76191c8b682d96adfc3a017f44f3fcfd0bec5dcb9a958d31668250b8a0ba396a";

/// SHA-256 of the minimal HEALPix anomaly regression fixture used to verify
/// issue #116 diagnostic detection without retaining the historical 20 MB map.
pub const LEGACY_HEALPIX_ANOMALY_REGRESSION_FIXTURE_SHA256: &str =
    "09cac5a58d0089529c8b8967cca02e893152cc51eeec0417864e8c04e9c0a1f0";
/// Repository-relative path to the minimal regression fixture.
pub const LEGACY_HEALPIX_ANOMALY_REGRESSION_FIXTURE_PATH: &str =
    "crates/nsb-data-tools/tests/fixtures/healpix_legacy_anomaly_regression.csv";
/// Packed RING runtime map SHA-256 for the canonical nside=128 candidate
/// after siderust NESTED→RING conversion **and** production admission CSV
/// headers required by `ValidatedStarlightMap`. The same digest without those
/// headers (packer comments only) is
/// `4a9275fd98d8565a33a7db29bce5f0544819387a970782f06c0f480b25877698`.
/// The pre-siderust handwritten nest2ring digest was
/// `c87db972717959962ab590ce71eb90506cbfd73ccb108a3d3851a3e9ecff8f90`.
pub const CANONICAL_RUNTIME_MAP_SHA256: &str =
    "c777917b7c9aceab5d3e0e25bb6ab0e0b75ee21357097c2ca4abe6a097a2243b";
/// Gaia DR3 GaiaSource `_MD5SUM.txt` acquisition-manifest SHA-256.
pub const GAIA_SOURCE_CHECKSUM_MANIFEST_SHA256: &str =
    "9ec782f9c83b29885924c7d47bba18d70c86b8cbefbc408b19090b6a76e8e369";
/// Gaia DR3 XP continuous `_MD5SUM.txt` acquisition-manifest SHA-256.
pub const XP_CONTINUOUS_CHECKSUM_MANIFEST_SHA256: &str =
    "f23df1ffb45b19fc3f34d6f37791179cef1ebec6c5b9fd613a488b3be580fccd";
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

    let npix = crate::starlight::healpix::gaia_nested_npix(candidate.nside)?;
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
    let grid = ring_grid(nside)?;
    let npix = grid.npix();
    let omega = grid.pixel_area_sr();
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
    out.push_str("# source_candidate_sha256=sha256:");
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

/// Convert a NESTED HEALPix index to RING by taking the nested pixel centre
/// and asking `siderust` to assign the RING pixel for that direction.
fn nest2ring(nside: u32, ipnest: u64) -> Result<u64> {
    crate::starlight::healpix::galactic_nested_to_ring(nside, ipnest)
}

fn ring_grid(nside: u32) -> Result<HealpixGrid> {
    let nside = Nside::new(nside).map_err(|error| anyhow::anyhow!("{error}"))?;
    HealpixGrid::new(nside, HealpixOrdering::Ring).map_err(|error| anyhow::anyhow!("{error}"))
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
    use siderust::coordinates::frames::Galactic;
    use siderust::coordinates::spherical::Direction;
    use siderust::qtty::Degrees;
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

    fn galactic_direction(lon_deg: f64, lat_deg: f64) -> Direction<Galactic> {
        Direction::<Galactic>::new(Degrees::new(lon_deg), Degrees::new(lat_deg))
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
        let (lon, lat) = map.pixel_lon_lat_deg(0).unwrap();
        let occupied =
            map.lookup(
                siderust::coordinates::spherical::Direction::<
                    siderust::coordinates::frames::Galactic,
                >::new(
                    siderust::qtty::Degrees::new(lon),
                    siderust::qtty::Degrees::new(lat),
                )
                .to_cartesian(),
            );
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
    fn nest2ring_matches_siderust_ring_centers_for_nside_128() {
        const NSIDE: u32 = 128;
        const MAX_SEP_RAD: f64 = 1.0e-7;
        let npix = 12 * u64::from(NSIDE) * u64::from(NSIDE);
        let grid = ring_grid(NSIDE).unwrap();
        let mut seen = vec![false; usize::try_from(npix).unwrap()];
        let mut ring_to_nest = vec![0u64; usize::try_from(npix).unwrap()];
        for nest in 0..npix {
            let nested_dir = crate::starlight::healpix::nested_pixel_center(NSIDE, nest).unwrap();
            let ring = nest2ring(NSIDE, nest).unwrap();
            assert!(ring < npix);
            let slot = usize::try_from(ring).unwrap();
            assert!(!seen[slot], "RING collision at {ring} from nested {nest}");
            seen[slot] = true;
            ring_to_nest[slot] = nest;
            let ring_dir = grid
                .pixel_center(siderust::healpix::HealpixIndex::new(ring))
                .unwrap();
            let sep = nested_dir
                .to_spherical()
                .angular_separation(&ring_dir.to_spherical())
                .value()
                .to_radians();
            assert!(
                sep < MAX_SEP_RAD,
                "nested {nest} -> RING {ring} angular error {sep} rad"
            );
        }
        assert!(seen.iter().all(|hit| *hit));

        let sky = [
            ("galactic-center", 0.0, 0.0),
            ("anti-center", 180.0, 0.0),
            ("plane", 90.0, 0.0),
            ("north-pole", 0.0, 90.0),
            ("south-pole", 0.0, -90.0),
            ("longitude-seam", 359.9, 0.0),
        ];
        for (label, lon, lat) in sky {
            let dir = galactic_direction(lon, lat).to_cartesian();
            let ring = grid.direction_to_pixel(dir).unwrap().get();
            let nest = ring_to_nest[usize::try_from(ring).unwrap()];
            let nested_dir = crate::starlight::healpix::nested_pixel_center(NSIDE, nest).unwrap();
            let ring_dir = grid
                .pixel_center(siderust::healpix::HealpixIndex::new(ring))
                .unwrap();
            assert!(
                nested_dir
                    .to_spherical()
                    .angular_separation(&ring_dir.to_spherical())
                    .value()
                    .to_radians()
                    < MAX_SEP_RAD,
                "{label} lost directional identity"
            );
        }
        let face_size = u64::from(NSIDE) * u64::from(NSIDE);
        for face in 0..12u64 {
            let nest = face * face_size;
            let _ = nest2ring(NSIDE, nest).unwrap();
        }
    }

    #[test]
    fn wrong_schema_ordering_nside_and_representation_fail_closed() {
        let dir = TempDir::new().unwrap();
        let wrong_schema = HEADER.replace(
            "nsb-healpix-starlight-candidate-v5",
            "nsb-healpix-starlight-candidate-v4",
        ) + "0,1.0,0.1,0.2,0.25,5,1\n";
        let candidate = write_candidate(&dir, &wrong_schema);
        let sha = checksum_io::sha256_file(&candidate).unwrap();
        assert!(pack_candidate_map(&PackInputs {
            candidate_map: candidate,
            expected_candidate_sha256: sha,
            expected_nside: 1,
            output_csv: dir.path().join("s.csv"),
            output_sidecar: dir.path().join("s.toml"),
            provenance_headers: BTreeMap::new(),
        })
        .is_err());

        let ring_order =
            HEADER.replace("ordering=nested", "ordering=ring") + "0,1.0,0.1,0.2,0.25,5,1\n";
        let candidate = write_candidate(&dir, &ring_order);
        let sha = checksum_io::sha256_file(&candidate).unwrap();
        assert!(pack_candidate_map(&PackInputs {
            candidate_map: candidate,
            expected_candidate_sha256: sha,
            expected_nside: 1,
            output_csv: dir.path().join("o.csv"),
            output_sidecar: dir.path().join("o.toml"),
            provenance_headers: BTreeMap::new(),
        })
        .is_err());

        let dense = HEADER.replace("representation=sparse", "representation=dense")
            + "0,1.0,0.1,0.2,0.25,5,1\n";
        let candidate = write_candidate(&dir, &dense);
        let sha = checksum_io::sha256_file(&candidate).unwrap();
        assert!(pack_candidate_map(&PackInputs {
            candidate_map: candidate,
            expected_candidate_sha256: sha,
            expected_nside: 1,
            output_csv: dir.path().join("d.csv"),
            output_sidecar: dir.path().join("d.toml"),
            provenance_headers: BTreeMap::new(),
        })
        .is_err());

        let candidate = write_candidate(&dir, &format!("{HEADER}0,1.0,0.1,0.2,0.25,5,1\n"));
        let sha = checksum_io::sha256_file(&candidate).unwrap();
        assert!(pack_candidate_map(&PackInputs {
            candidate_map: candidate,
            expected_candidate_sha256: sha,
            expected_nside: 2,
            output_csv: dir.path().join("n.csv"),
            output_sidecar: dir.path().join("n.toml"),
            provenance_headers: BTreeMap::new(),
        })
        .is_err());
    }

    #[test]
    fn out_of_domain_nan_and_inconsistent_total_fail_closed() {
        let dir = TempDir::new().unwrap();
        for body in [
            format!("{HEADER}12,1.0,0.1,0.2,0.25,5,1\n"),
            format!("{HEADER}0,NaN,0.1,0.2,0.25,5,1\n"),
            format!("{HEADER}0,inf,0.1,0.2,0.25,5,1\n"),
            format!("{HEADER}0,1.0,inf,0.2,0.25,5,1\n"),
            format!("{HEADER}1,1.0,0.1,0.2,0.25,5,1\n0,1.0,0.1,0.2,0.25,5,1\n"),
            format!("{HEADER}0,1.0,0.1,0.2,0.25,not-a-count,1\n"),
            format!("{HEADER}0,1.0,0.5,0.2,0.25,5,1\n"),
            format!("{HEADER}0,1.0,0.1,0.5,0.25,5,1\n"),
        ] {
            let candidate = write_candidate(&dir, &body);
            let sha = checksum_io::sha256_file(&candidate).unwrap();
            assert!(
                pack_candidate_map(&PackInputs {
                    candidate_map: candidate,
                    expected_candidate_sha256: sha,
                    expected_nside: 1,
                    output_csv: dir.path().join("bad.csv"),
                    output_sidecar: dir.path().join("bad.toml"),
                    provenance_headers: BTreeMap::new(),
                })
                .is_err(),
                "expected fail closed for {body:?}"
            );
        }
    }

    #[test]
    fn packing_is_byte_identical_across_two_runs() {
        let dir = TempDir::new().unwrap();
        let candidate = write_candidate(&dir, &format!("{HEADER}0,1.0,0.1,0.2,0.25,5,1\n"));
        let sha = checksum_io::sha256_file(&candidate).unwrap();
        let before = fs::read(&candidate).unwrap();
        let mut maps = Vec::new();
        let mut sidecars = Vec::new();
        for i in 0..2 {
            let csv = dir.path().join(format!("out{i}.csv"));
            let sidecar = dir.path().join(format!("out{i}.toml"));
            pack_candidate_map(&PackInputs {
                candidate_map: candidate.clone(),
                expected_candidate_sha256: sha.clone(),
                expected_nside: 1,
                output_csv: csv.clone(),
                output_sidecar: sidecar.clone(),
                provenance_headers: BTreeMap::new(),
            })
            .unwrap();
            maps.push(fs::read(&csv).unwrap());
            sidecars.push(fs::read(&sidecar).unwrap());
        }
        assert_eq!(maps[0], maps[1]);
        assert_eq!(sidecars[0], sidecars[1]);
        assert_eq!(fs::read(&candidate).unwrap(), before);
        assert!(!String::from_utf8_lossy(&maps[0]).contains("b_s10"));
        assert!(String::from_utf8_lossy(&maps[0]).contains("s10_diagnostics=not_provided"));
    }

    #[test]
    fn canonical_nside128_pack_is_deterministic_and_runtime_loadable() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
        let candidate = root.join("crates/nsb/data/starlight_nside128.csv");
        if !candidate.is_file() {
            return;
        }
        let before = checksum_io::sha256_file(&candidate).unwrap();
        assert_eq!(before, CANONICAL_CANDIDATE_SHA256);
        let dir = TempDir::new().unwrap();
        let csv = dir.path().join("starlight_nside128.release.csv");
        let sidecar = dir.path().join("starlight_nside128.pack.toml");
        let production_sidecar = dir.path().join("starlight_nside128.manifest.toml");
        let candidate_section = crate::starlight::promotion::CandidateSection {
            status: crate::starlight::promotion::CandidateStatus::Pinned,
            candidate_sha256: CANONICAL_CANDIDATE_SHA256.to_string(),
            map_path: "crates/nsb/data/starlight_nside128.csv".into(),
            map_schema: "nsb-healpix-starlight-candidate-v5".into(),
            band: "300-650 nm combined integrated photon radiance (corrected 300-336 nm UV + measured 336-650 nm)".into(),
            units: "ph_m-2_s-1".into(),
            nside: 128,
            ordering: "nested".into(),
            gaia_release: "Gaia DR3".into(),
            model_versions: BTreeMap::new(),
        };
        let headers = crate::starlight::promotion::runtime_admission_headers(&candidate_section);
        let outcome = pack_candidate_map(&PackInputs {
            candidate_map: candidate.clone(),
            expected_candidate_sha256: CANONICAL_CANDIDATE_SHA256.to_string(),
            expected_nside: 128,
            output_csv: csv.clone(),
            output_sidecar: sidecar.clone(),
            provenance_headers: headers.clone(),
        })
        .unwrap();
        assert_eq!(
            checksum_io::sha256_file(&candidate).unwrap(),
            CANONICAL_CANDIDATE_SHA256
        );
        assert_eq!(
            outcome.runtime_map_sha256, CANONICAL_RUNTIME_MAP_SHA256,
            "runtime map SHA-256 changed; update the pin only with a documented conversion reason"
        );
        assert_eq!(outcome.occupied_pixels + outcome.omitted_pixels, 196_608);
        crate::starlight::promotion::write_production_sidecar(
            &production_sidecar,
            &candidate_section,
            &outcome.runtime_map_sha256,
            outcome.all_sky_flux_sum_ph_m2_s,
        )
        .unwrap();
        let packed = fs::read_to_string(&csv).unwrap();
        let map =
            nsb::StarlightMap::from_csv_str(&packed, nsb::StarlightProvenance::test_fixture())
                .unwrap();
        assert_eq!(map.pixels().len(), 196_608);
        let (lon, lat) = map.pixel_lon_lat_deg(0).unwrap();
        let looked =
            map.lookup(
                siderust::coordinates::spherical::Direction::<
                    siderust::coordinates::frames::Galactic,
                >::new(
                    siderust::qtty::Degrees::new(lon),
                    siderust::qtty::Degrees::new(lat),
                )
                .to_cartesian(),
            );
        assert!(!looked.s10_diagnostics_provided);
        assert!(looked.statistical_uncertainty.is_some());
        nsb::ValidatedStarlightMap::from_files(&csv, &production_sidecar).unwrap();

        let csv2 = dir.path().join("second.release.csv");
        let sidecar2 = dir.path().join("second.pack.toml");
        let outcome2 = pack_candidate_map(&PackInputs {
            candidate_map: candidate,
            expected_candidate_sha256: CANONICAL_CANDIDATE_SHA256.to_string(),
            expected_nside: 128,
            output_csv: csv2.clone(),
            output_sidecar: sidecar2,
            provenance_headers: headers,
        })
        .unwrap();
        assert_eq!(fs::read(&csv).unwrap(), fs::read(&csv2).unwrap());
        assert_eq!(
            fs::read(&sidecar).unwrap(),
            fs::read(dir.path().join("second.pack.toml")).unwrap()
        );
        assert_eq!(outcome.runtime_map_sha256, outcome2.runtime_map_sha256);
        assert_eq!(
            outcome.runtime_sidecar_sha256,
            outcome2.runtime_sidecar_sha256
        );
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
