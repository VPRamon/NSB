// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Vallés Puig, Ramon
mod common;
use common::starlight_test_provenance;
use nsb::components::starlight::StarlightMap;
use siderust::checksum::sha256;

const CSV: &str = include_str!("data/starlight_fixture_map.csv");

#[test]
fn compact_roundtrip_is_exact_and_deterministic_in_both_orderings() {
    for ordering in ["ring", "nested"] {
        let csv = CSV.replace("ordering=ring", &format!("ordering={ordering}"));
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
