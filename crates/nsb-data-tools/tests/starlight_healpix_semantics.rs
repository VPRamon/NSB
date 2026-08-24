use nsb_data_tools::starlight::{
    pack::{pack_candidate_map, PackInputs, CANONICAL_CANDIDATE_SHA256},
    validation::candidate_map,
};
use siderust::coordinates::frames::Galactic;
use siderust::healpix::{HealpixGrid, HealpixIndex, HealpixOrdering, Nside};
use std::collections::BTreeMap;
use std::f64::consts::PI;
use std::fs;
use std::path::PathBuf;
use tempfile::TempDir;

const NSIDE: u32 = 128;
const PH_M2_S_SR_TO_PH_CM2_NS_SR: f64 = 1.0e-13;

fn assert_close(actual: f64, expected: f64, label: &str) {
    let scale = actual.abs().max(expected.abs()).max(1.0);
    let relative = (actual - expected).abs() / scale;
    assert!(
        relative <= 1.0e-12,
        "{label}: actual={actual:.17e}, expected={expected:.17e}, relative={relative:.3e}"
    );
}

#[test]
fn canonical_pack_preserves_siderust_nested_sky_identity_exhaustively() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let candidate_path = root.join("crates/nsb/data/starlight_nside128.csv");
    assert!(
        candidate_path.is_file(),
        "canonical Starlight candidate must be present for the nside=128 semantic gate"
    );

    let candidate = candidate_map::load(&candidate_path, NSIDE, Some(CANONICAL_CANDIDATE_SHA256))
        .expect("load frozen candidate");

    let dir = TempDir::new().expect("temporary pack directory");
    let runtime_path = dir.path().join("starlight.release.csv");
    let pack_sidecar = dir.path().join("starlight.pack.toml");
    pack_candidate_map(&PackInputs {
        candidate_map: candidate_path,
        expected_candidate_sha256: CANONICAL_CANDIDATE_SHA256.to_string(),
        expected_nside: NSIDE,
        output_csv: runtime_path.clone(),
        output_sidecar: pack_sidecar,
        provenance_headers: BTreeMap::new(),
    })
    .expect("pack canonical candidate");

    let runtime = fs::read_to_string(&runtime_path).expect("read packed runtime map");
    let mut rows = Vec::<[f64; 4]>::new();
    for line in runtime
        .lines()
        .filter(|line| !line.trim().is_empty() && !line.starts_with('#'))
    {
        if line.starts_with("healpix_index,") {
            continue;
        }
        let fields: Vec<&str> = line.split(',').collect();
        assert_eq!(fields.len(), 5, "unexpected packed runtime row: {line}");
        let index: usize = fields[0].parse().expect("runtime HEALPix index");
        assert_eq!(index, rows.len(), "runtime map must be dense RING order");
        rows.push([
            fields[1].parse().expect("integrated radiance"),
            fields[2].parse().expect("statistical uncertainty"),
            fields[3].parse().expect("systematic uncertainty"),
            fields[4].parse().expect("total uncertainty"),
        ]);
    }

    let npix = 12_u64 * u64::from(NSIDE) * u64::from(NSIDE);
    assert_eq!(rows.len(), usize::try_from(npix).unwrap());

    let nside = Nside::new(NSIDE).expect("valid nside");
    let nested_grid =
        HealpixGrid::new(nside, HealpixOrdering::Nested).expect("Siderust NESTED grid");
    let ring_grid = HealpixGrid::new(nside, HealpixOrdering::Ring).expect("Siderust RING grid");
    let mut seen_ring = vec![false; rows.len()];
    let pixel_sr = 4.0 * PI / npix as f64;
    let to_radiance = |value: f64| (value / pixel_sr) * PH_M2_S_SR_TO_PH_CM2_NS_SR;

    for nested_index in 0..npix {
        let direction = nested_grid
            .pixel_center::<Galactic>(HealpixIndex::new(nested_index))
            .expect("Siderust NESTED pixel centre");
        let ring_index = ring_grid
            .direction_to_pixel(direction)
            .expect("Siderust RING assignment")
            .get();
        let ring_slot = usize::try_from(ring_index).unwrap();
        assert!(
            !seen_ring[ring_slot],
            "Siderust NESTED->RING collision at RING pixel {ring_index}"
        );
        seen_ring[ring_slot] = true;

        let row = rows[ring_slot];
        if let Some(pixel) = candidate.pixels.get(&(nested_index as u32)) {
            assert_close(
                row[0],
                to_radiance(pixel.flux_ph_m2_s),
                &format!("nested {nested_index} integrated flux"),
            );
            assert_close(
                row[1],
                to_radiance(pixel.statistical_uncertainty_ph_m2_s),
                &format!("nested {nested_index} statistical uncertainty"),
            );
            assert_close(
                row[2],
                to_radiance(pixel.systematic_uncertainty_ph_m2_s),
                &format!("nested {nested_index} systematic uncertainty"),
            );
            assert_close(
                row[3],
                to_radiance(pixel.total_uncertainty_ph_m2_s),
                &format!("nested {nested_index} total uncertainty"),
            );
        } else {
            assert_eq!(
                row,
                [0.0; 4],
                "omitted NESTED pixel {nested_index} must remain physically zero at RING {ring_index}"
            );
        }
    }

    assert!(seen_ring.iter().all(|seen| *seen));
}
