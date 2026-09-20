//! Jones 2013 reference-fixture schema validation.
//!
//! Numerical spectral-model regression pins for fixed moonlight geometries live
//! in the internal `components::moonlight::jones_2013_spectral` unit tests.
//! Public model selection and evaluator dispatch are covered by
//! `moonlight_model_selection`.

use std::collections::{BTreeMap, BTreeSet};

#[test]
fn quantitative_reference_fixture_is_well_formed() {
    // Schema / tolerance contract only. Spectral-model numeric pins for these
    // geometries live in jones_2013_spectral unit tests.
    let fixture = include_str!("data/jones2013_reference_cases.csv");
    let mut checked = 0usize;
    let mut wavelengths_by_case: BTreeMap<&str, BTreeSet<u64>> = BTreeMap::new();
    let mut integrated_by_case: BTreeMap<&str, f64> = BTreeMap::new();

    for line in fixture
        .lines()
        .skip(1)
        .filter(|line| !line.trim().is_empty())
    {
        let columns: Vec<_> = line.split(',').collect();
        assert_eq!(
            columns.len(),
            12,
            "reference fixture schema changed: {line}"
        );

        let case_id = columns[0];
        let phase_angle_deg: f64 = columns[2].parse().expect("phase angle");
        let separation_deg: f64 = columns[3].parse().expect("moon-target separation");
        let moon_zenith_deg: f64 = columns[4].parse().expect("moon zenith");
        let source_zenith_deg: f64 = columns[5].parse().expect("source zenith");
        let wavelength_nm: f64 = columns[8].parse().expect("wavelength");
        let expected_density: f64 = columns[9].parse().expect("spectral density");
        let expected_integrated: f64 = columns[10].parse().expect("integrated radiance");
        let tolerance: f64 = columns[11].parse().expect("relative tolerance");

        assert!((0.0..=180.0).contains(&phase_angle_deg));
        assert!((0.0..=180.0).contains(&separation_deg));
        assert!((0.0..90.0).contains(&moon_zenith_deg));
        assert!((0.0..90.0).contains(&source_zenith_deg));
        assert!((300.0..=650.0).contains(&wavelength_nm));
        assert!(expected_density.is_finite() && expected_density >= 0.0);
        assert!(expected_integrated.is_finite() && expected_integrated >= 0.0);
        assert!(
            tolerance.is_finite() && tolerance > 0.0 && tolerance <= 0.20,
            "fixture tolerances must remain scientifically meaningful"
        );

        wavelengths_by_case
            .entry(case_id)
            .or_default()
            .insert(wavelength_nm.round() as u64);
        integrated_by_case
            .entry(case_id)
            .and_modify(|previous| {
                assert_eq!(
                    previous.to_bits(),
                    expected_integrated.to_bits(),
                    "integrated fixture target must be stable within a case"
                );
            })
            .or_insert(expected_integrated);
        checked += 1;
    }

    assert!(
        checked >= 8,
        "reference fixture should cover multiple geometries and wavelengths"
    );
    assert!(
        wavelengths_by_case.len() >= 3,
        "reference fixture should cover multiple moonlight geometries"
    );
    for (case_id, wavelengths) in wavelengths_by_case {
        assert!(
            wavelengths.len() >= 4,
            "case {case_id} should include representative spectral-density samples"
        );
    }
}
