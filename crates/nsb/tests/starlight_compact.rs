// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Vallés Puig, Ramon
mod common;
use common::starlight_test_provenance;
use nsb::components::starlight::StarlightMap;
use siderust::checksum::sha256;

const CSV: &str = include_str!("data/starlight_fixture_map.csv");

#[test]
fn compact_roundtrip_is_exact_and_deterministic_in_both_orderings() {
    for ordering in ["ring", "nested", "RING", "NESTED"] {
        let csv = CSV
            .replace("ordering=ring", &format!("ordering={ordering}"))
            .replace("coordinate_frame=galactic", "coordinate_frame=Galactic");
        let original = StarlightMap::from_csv_str(&csv, starlight_test_provenance()).unwrap();
        let first = StarlightMap::compact_csv(&csv, starlight_test_provenance()).unwrap();
        assert_eq!(
            first,
            StarlightMap::compact_csv(&csv, starlight_test_provenance()).unwrap()
        );
        let decoded =
            StarlightMap::from_compact_bytes(&first, starlight_test_provenance()).unwrap();
        assert_eq!(original, decoded);
        assert!(decoded
            .pixels()
            .iter()
            .all(|p| !p.s10_diagnostics_provided && p.total_uncertainty.is_some()));
        for (a, b) in original.pixels().iter().zip(decoded.pixels()) {
            assert_eq!(
                a.integrated.value().to_bits(),
                b.integrated.value().to_bits()
            );
        }
    }
}

#[test]
fn compact_rejects_corruption_truncation_unknown_version_and_wrong_lengths() {
    let bytes = StarlightMap::compact_csv(CSV, starlight_test_provenance()).unwrap();
    for length in [0, 7, 12, 43, bytes.len() - 1] {
        assert!(
            StarlightMap::from_compact_bytes(&bytes[..length], starlight_test_provenance())
                .is_err()
        );
    }
    for offset in [0, 7, 8, 12, bytes.len() / 2, bytes.len() - 1] {
        let mut corrupt = bytes.clone();
        corrupt[offset] ^= 1;
        assert!(StarlightMap::from_compact_bytes(&corrupt, starlight_test_provenance()).is_err());
    }
    let mut invalid_length = bytes.clone();
    invalid_length[8..12].copy_from_slice(&u32::MAX.to_le_bytes());
    let end = invalid_length.len() - 32;
    let digest = sha256(&invalid_length[..end]);
    invalid_length[end..].copy_from_slice(&digest);
    assert!(
        StarlightMap::from_compact_bytes(&invalid_length, starlight_test_provenance()).is_err()
    );
    let mut missing_pixel = bytes[..bytes.len() - 64].to_vec();
    let digest = sha256(&missing_pixel);
    missing_pixel.extend_from_slice(&digest);
    assert!(StarlightMap::from_compact_bytes(&missing_pixel, starlight_test_provenance()).is_err());
}

#[test]
fn approved_bundled_default_evaluates_all_four_components() {
    use nsb::components::starlight::StarlightProduct;
    use nsb::{ComponentMask, NsbEvaluator, PointQuery, Target, DEG};
    // In the pending tree the absence is explicit; the same test exercises the
    // embedded path once promotion has registered an approved product.
    assert_eq!(
        ComponentMask::ALL.contains(ComponentMask::STARLIGHT),
        StarlightProduct::bundled_production_available()
    );
    if !StarlightProduct::bundled_production_available() {
        return;
    }
    let time = tempoch::Time::<tempoch::UTC>::from_chrono(
        chrono::DateTime::parse_from_rfc3339("2023-09-04T01:48:00Z")
            .unwrap()
            .with_timezone(&chrono::Utc),
    );
    let query = PointQuery::new(
        siderust::catalogs::observatories::EL_PARANAL.geodetic(),
        time,
        Target::new(266.41683 * DEG, -29.00781 * DEG),
    );
    let first = NsbEvaluator::new().unwrap().evaluate(&query).unwrap();
    let second = NsbEvaluator::new().unwrap().evaluate(&query).unwrap();
    assert_eq!(first.components.len(), 4);
    assert_eq!(first.integrated, second.integrated);
    assert_eq!(
        first.integrated.value(),
        first
            .components
            .iter()
            .map(|c| c.integrated.value())
            .sum::<f64>()
    );
    assert!(
        first
            .components
            .iter()
            .find(|c| c.name == "starlight")
            .unwrap()
            .integrated
            .value()
            > 0.0
    );
}

#[test]
fn compact_rejects_semantic_corruption_even_with_valid_internal_checksum() {
    fn resign(bytes: &mut [u8]) {
        let end = bytes.len() - 32;
        let digest = sha256(&bytes[..end]);
        bytes[end..].copy_from_slice(&digest);
    }
    let original = StarlightMap::compact_csv(CSV, starlight_test_provenance()).unwrap();
    let length = u32::from_le_bytes(original[8..12].try_into().unwrap()) as usize;
    let header = std::str::from_utf8(&original[12..12 + length]).unwrap();
    for (from, to) in [
        (
            "# coordinate_frame=galactic",
            "# coordinate_frame=equatorial",
        ),
        (
            "# s10_diagnostics=not_provided",
            "# s10_diagnostics=provided",
        ),
        ("# nside=1", "# nside=invalid"),
        ("# nside=1", "# nside=0"),
        ("# nside=1", "# nside=3"),
        ("# nside=1", "# nside=2"),
        ("# nside=1\n", ""),
        ("# ordering=ring", "# ordering=unknown"),
    ] {
        let altered = header.replace(from, to);
        assert_ne!(header, altered);
        let mut bytes = original[..8].to_vec();
        bytes.extend_from_slice(&(altered.len() as u32).to_le_bytes());
        bytes.extend_from_slice(altered.as_bytes());
        bytes.extend_from_slice(&original[12 + length..]);
        resign(&mut bytes);
        assert!(StarlightMap::from_compact_bytes(&bytes, starlight_test_provenance()).is_err());
    }
    for (field, value) in [
        (0, f64::NAN),
        (0, f64::INFINITY),
        (0, -1.0),
        (1, 999.0),
        (2, -1.0),
    ] {
        let mut bytes = original.clone();
        let offset = 12 + length + field * 8;
        bytes[offset..offset + 8].copy_from_slice(&value.to_le_bytes());
        resign(&mut bytes);
        assert!(StarlightMap::from_compact_bytes(&bytes, starlight_test_provenance()).is_err());
    }
    let mut invalid_utf8 = original;
    invalid_utf8[12] = 255;
    resign(&mut invalid_utf8);
    assert!(StarlightMap::from_compact_bytes(&invalid_utf8, starlight_test_provenance()).is_err());
}

fn bv_csv() -> String {
    let mut csv = String::new();
    for line in CSV.lines() {
        if line == "# s10_diagnostics=not_provided" {
            csv.push_str("# s10_diagnostics=monochromatic_445_551_nm_nsb_s10");
        } else {
            csv.push_str(line);
            if line.starts_with("healpix_index,") {
                csv.push_str(",b_w_m2_sr_nm,v_w_m2_sr_nm");
            } else if line.starts_with("0,") {
                csv.push_str(",0,0");
            } else if !line.starts_with('#') {
                csv.push_str(",1.6e-10,3.2e-10");
            }
        }
        csv.push('\n');
    }
    csv
}

#[test]
fn compact_v2_preserves_legacy_fields_and_reports_available_zero() {
    let provenance = starlight_test_provenance;
    let v1 = StarlightMap::compact_csv(CSV, provenance()).unwrap();
    let csv = bv_csv();
    let v2 = StarlightMap::compact_csv(&csv, provenance()).unwrap();
    assert_eq!(&v2[..8], b"NSBSTL02");
    assert_eq!(v2, StarlightMap::compact_csv(&csv, provenance()).unwrap());
    let v1_map = StarlightMap::from_compact_bytes(&v1, provenance()).unwrap();
    let v2_map = StarlightMap::from_compact_bytes(&v2, provenance()).unwrap();
    assert_eq!(
        StarlightMap::from_csv_str(&csv, provenance()).unwrap(),
        v2_map
    );
    for (i, (before, after)) in v1_map.pixels().iter().zip(v2_map.pixels()).enumerate() {
        assert_eq!(
            before.integrated.value().to_bits(),
            after.integrated.value().to_bits()
        );
        assert_eq!(
            before.statistical_uncertainty,
            after.statistical_uncertainty
        );
        assert_eq!(before.systematic_uncertainty, after.systematic_uncertainty);
        assert_eq!(before.total_uncertainty, after.total_uncertainty);
        assert!(!before.s10_diagnostics_provided);
        assert!(after.s10_diagnostics_provided);
        assert_eq!(
            after.b_flux_s10.value(),
            if i == 0 { 0.0 } else { 1.6e-10 / 1.28e-11 }
        );
        assert_eq!(
            after.v_flux_s10.value(),
            if i == 0 { 0.0 } else { 3.2e-10 / 1.28e-11 }
        );
    }
    for value in [f64::NAN, f64::INFINITY, -1.0] {
        let mut corrupted = v2.clone();
        let header_len = u32::from_le_bytes(v2[8..12].try_into().unwrap()) as usize;
        let b_offset = 12 + header_len + 32;
        corrupted[b_offset..b_offset + 8].copy_from_slice(&value.to_le_bytes());
        let digest_offset = corrupted.len() - 32;
        let digest = sha256(&corrupted[..digest_offset]);
        corrupted[digest_offset..].copy_from_slice(&digest);
        assert!(StarlightMap::from_compact_bytes(&corrupted, provenance()).is_err());
    }
    assert!(
        StarlightMap::compact_csv(&csv.replace(",1.6e-10,3.2e-10", ",1.6e-10"), provenance())
            .is_err()
    );
}

#[test]
fn compact_v2_flows_through_public_starlight_evaluator() {
    use nsb::{
        ComponentMask, NsbEvaluator, NsbModelConfig, PointQuery, StarlightProduct, Target, DEG,
    };

    let provenance = starlight_test_provenance;
    let time = tempoch::Time::<tempoch::UTC>::from_chrono(
        chrono::DateTime::parse_from_rfc3339("2023-09-04T01:48:00Z")
            .unwrap()
            .with_timezone(&chrono::Utc),
    );
    let query = PointQuery::new(
        siderust::catalogs::observatories::EL_PARANAL.geodetic(),
        time,
        Target::new(266.41683 * DEG, -29.00781 * DEG),
    )
    .with_components(ComponentMask::STARLIGHT);
    let evaluate = |csv| {
        let compact = StarlightMap::compact_csv(csv, provenance()).unwrap();
        let map = StarlightMap::from_compact_bytes(&compact, provenance()).unwrap();
        NsbEvaluator::with_config(
            NsbModelConfig::generic_clear_sky()
                .with_starlight_product(StarlightProduct::with_experimental_map(map)),
        )
        .unwrap()
        .evaluate(&query)
        .unwrap()
    };
    let legacy = evaluate(CSV);
    let result = evaluate(&bv_csv());
    assert_eq!(legacy.integrated, result.integrated);
    assert!(legacy.b_mag.value().is_nan());
    assert!(!legacy.components[0].bv_diagnostics_available);
    let star = &result.components[0];
    assert!(star.bv_diagnostics_available);
    assert!(star.b_flux_s10.value() > 0.0);
    assert_eq!(star.v_flux_s10.value(), 2.0 * star.b_flux_s10.value());
    let expected = 27.78 - 2.5 * star.b_flux_s10.value().log10();
    assert_eq!(result.b_mag.value(), expected);
    let expected_colour = 2.5 * 2.0_f64.log10();
    assert!((result.b_mag.value() - result.v_mag.value() - expected_colour).abs() < 1e-12);
}

#[test]
fn v1_production_manifest_cannot_admit_v2_even_with_matching_checksum() {
    use nsb::components::starlight::ValidatedStarlightMap;
    use siderust::checksum::to_hex;

    let v2 = StarlightMap::compact_csv(&bv_csv(), starlight_test_provenance()).unwrap();
    let existing = include_bytes!("../data/starlight_nside128.compact.release.bin");
    let manifest = include_str!("../data/starlight_nside128.compact.manifest.toml");
    let matching_manifest = manifest.replace(&to_hex(&sha256(existing)), &to_hex(&sha256(&v2)));
    assert_ne!(manifest, matching_manifest);
    let error =
        ValidatedStarlightMap::from_bytes_and_manifest(&v2, &matching_manifest).unwrap_err();
    assert!(
        error
            .to_string()
            .contains("production admission does not attest to B/V"),
        "{error}"
    );
}
