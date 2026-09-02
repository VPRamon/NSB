use nsb_data_tools::starlight::{
    diagnostics::analyse_candidate_path,
    healpix::{
        gaia_source_id_equatorial_nested_pixel, galactic_nested_pixel_from_icrs_position,
        galactic_nested_to_ring, legacy_equatorial_bitshift_mislabelled_as_galactic_pixel,
        IcrsSkyPosition,
    },
    pack::{
        pack_candidate_map, PackInputs, CANONICAL_CANDIDATE_SHA256,
        LEGACY_HEALPIX_ANOMALY_REGRESSION_FIXTURE_PATH,
        LEGACY_HEALPIX_ANOMALY_REGRESSION_FIXTURE_SHA256,
    },
    validation::candidate_map,
};
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

/// Independent integer NESTED -> RING reference path.
///
/// This follows the standard HEALPix face/x/y conversion used by the
/// reference `nest2ring` algorithm. Production packing does *not* use this
/// path: it derives a NESTED pixel centre and asks Siderust to assign the RING
/// pixel. Keeping these two structurally different paths makes the exhaustive
/// comparison capable of detecting a sky-scrambling permutation.
fn reference_nest2ring(nside: u32, ipnest: u64) -> u64 {
    assert!(nside.is_power_of_two() && nside > 0);
    let nside = i64::from(nside);
    let npface = nside * nside;
    let npix = 12 * npface;
    let ipnest = i64::try_from(ipnest).expect("nested index fits i64");
    assert!((0..npix).contains(&ipnest));

    const JRLL: [i64; 12] = [2, 2, 2, 2, 3, 3, 3, 3, 4, 4, 4, 4];
    const JPLL: [i64; 12] = [1, 3, 5, 7, 0, 2, 4, 6, 1, 3, 5, 7];

    let face = usize::try_from(ipnest / npface).expect("face fits usize");
    let ipf = u64::try_from(ipnest % npface).expect("face-local index fits u64");
    let mut ix = 0_u64;
    let mut iy = 0_u64;
    for bit in 0..32_u32 {
        ix |= ((ipf >> (2 * bit)) & 1) << bit;
        iy |= ((ipf >> (2 * bit + 1)) & 1) << bit;
    }
    let ix = i64::try_from(ix).expect("x fits i64");
    let iy = i64::try_from(iy).expect("y fits i64");

    let jr = JRLL[face] * nside - ix - iy - 1;
    let nl4 = 4 * nside;
    let (nr, n_before, kshift) = if jr < nside {
        let nr = jr;
        (nr, 2 * nr * (nr - 1), 0)
    } else if jr > 3 * nside {
        let nr = nl4 - jr;
        (nr, npix - 2 * nr * (nr + 1), 0)
    } else {
        (
            nside,
            2 * nside * (nside - 1) + (jr - nside) * nl4,
            (jr - nside) & 1,
        )
    };

    let mut jp = (JPLL[face] * nr + ix - iy + 1 + kshift) / 2;
    if jp > nl4 {
        jp -= nl4;
    }
    if jp < 1 {
        jp += nl4;
    }

    u64::try_from(n_before + jp - 1).expect("RING index is non-negative")
}

#[test]
fn canonical_pack_matches_independent_healpix_nest2ring_exhaustively() {
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

    let mut seen_ring = vec![false; rows.len()];
    let pixel_sr = 4.0 * PI / npix as f64;
    let to_radiance = |value: f64| (value / pixel_sr) * PH_M2_S_SR_TO_PH_CM2_NS_SR;

    for nested_index in 0..npix {
        let ring_index = reference_nest2ring(NSIDE, nested_index);
        let ring_slot = usize::try_from(ring_index).unwrap();
        assert!(
            !seen_ring[ring_slot],
            "reference HEALPix NESTED->RING collision at RING pixel {ring_index}"
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

#[test]
fn gaia_source_id_equatorial_and_galactic_pixels_are_not_interchangeable() {
    let source_id = (98_765_u64 << 35) | 9;
    let equatorial = gaia_source_id_equatorial_nested_pixel(source_id, NSIDE).unwrap();
    let position = IcrsSkyPosition::new(123.45, -12.34).unwrap();
    let galactic =
        galactic_nested_pixel_from_icrs_position(position.ra_deg, position.dec_deg, NSIDE).unwrap();
    let legacy =
        legacy_equatorial_bitshift_mislabelled_as_galactic_pixel(source_id, NSIDE).unwrap();
    assert_ne!(equatorial, galactic);
    assert_ne!(legacy, galactic);
}

#[test]
fn legacy_candidate_nside2_anomaly_diagnostic_reproduces_issue_116() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let candidate_path = root.join(LEGACY_HEALPIX_ANOMALY_REGRESSION_FIXTURE_PATH);
    let report = analyse_candidate_path(
        &candidate_path,
        NSIDE,
        Some(LEGACY_HEALPIX_ANOMALY_REGRESSION_FIXTURE_SHA256),
    )
    .unwrap();
    assert_eq!(report.anomalous_parents.len(), 6);
    assert_eq!(report.anomalous_parents, vec![0, 16, 18, 26, 27, 43]);
}

#[test]
fn corrected_candidate_does_not_reproduce_legacy_six_parent_anomalies() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let candidate_path = root.join("crates/nsb/data/starlight_nside128.csv");
    let report =
        analyse_candidate_path(&candidate_path, NSIDE, Some(CANONICAL_CANDIDATE_SHA256)).unwrap();
    let legacy_six = [0_u32, 16, 18, 26, 27, 43];
    let still_anomalous: Vec<_> = legacy_six
        .into_iter()
        .filter(|parent| report.anomalous_parents.contains(parent))
        .collect();
    assert!(
        still_anomalous.len() <= 1,
        "expected at most one legacy parent still anomalous, got {still_anomalous:?} (all: {:?})",
        report.anomalous_parents
    );
}

#[test]
fn production_nest2ring_matches_independent_reference_on_samples() {
    for nest in [0_u64, 1, 42, 1_234, 12_345] {
        assert_eq!(
            reference_nest2ring(NSIDE, nest),
            galactic_nested_to_ring(NSIDE, nest).unwrap()
        );
    }
}
