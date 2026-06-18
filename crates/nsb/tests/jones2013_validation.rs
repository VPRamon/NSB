use chrono::{DateTime, Utc};
use nsb::{AtmosphericConditions, Jones2013Spectral, Target, DEG};
use siderust::catalogs::observatories;
use siderust::coordinates::centers::Geodetic;
use siderust::coordinates::frames::ECEF;
use siderust::qtty::{Degrees as SiderustDegrees, Meters};
use tempoch::{Time, UTC};

fn parse_utc(input: &str) -> Time<UTC> {
    Time::<UTC>::from_chrono(
        DateTime::parse_from_rfc3339(input)
            .expect("RFC3339 timestamp")
            .with_timezone(&Utc),
    )
}

fn target_sagittarius() -> Target {
    Target::new(270.0 * DEG, -30.0 * DEG)
}

fn low_altitude_site() -> Geodetic<ECEF> {
    Geodetic::<ECEF>::new_raw(
        SiderustDegrees::new(-70.0),
        SiderustDegrees::new(-24.0),
        Meters::new(0.0),
    )
}

fn high_altitude_site() -> Geodetic<ECEF> {
    Geodetic::<ECEF>::new_raw(
        SiderustDegrees::new(-70.0),
        SiderustDegrees::new(-24.0),
        Meters::new(2_500.0),
    )
}

#[test]
fn generic_clear_sky_pressure_tracks_location_altitude() {
    let low = AtmosphericConditions::generic_clear_sky(low_altitude_site());
    let high = AtmosphericConditions::generic_clear_sky(high_altitude_site());

    assert!(low.surface_pressure.value().is_finite());
    assert!(high.surface_pressure.value().is_finite());
    assert!(low.surface_pressure > high.surface_pressure);
}

#[test]
fn site_presets_are_explicitly_distinct_from_generic_clear_sky() {
    let paranal_site = observatories::EL_PARANAL.geodetic();
    let generic = AtmosphericConditions::generic_clear_sky(paranal_site);
    let paranal = AtmosphericConditions::paranal_average();
    let cta_s = AtmosphericConditions::cta_s_clear_sky();
    let cta_n = AtmosphericConditions::cta_n_clear_sky();

    assert_eq!(paranal, cta_s, "CTA-S currently aliases the explicit Paranal-like preset");
    assert_ne!(generic.surface_pressure, cta_n.surface_pressure);
    assert_ne!(cta_n.surface_pressure, cta_s.surface_pressure);
}

#[test]
fn jones2013_computes_with_all_explicit_atmosphere_presets() {
    let location = observatories::EL_PARANAL.geodetic();
    let time = parse_utc("2023-09-04T02:00:00Z");
    let target = target_sagittarius();

    for conditions in [
        AtmosphericConditions::generic_clear_sky(location),
        AtmosphericConditions::paranal_average(),
        AtmosphericConditions::cta_s_clear_sky(),
        AtmosphericConditions::cta_n_clear_sky(),
    ] {
        let out = Jones2013Spectral::new(location, conditions)
            .compute(time, target)
            .expect("Jones 2013 moonlight computation");
        assert!(out.integrated.value().is_finite());
        assert!(out.b_flux_s10.value().is_finite());
        assert!(out.v_flux_s10.value().is_finite());
        assert!(out.integrated.value() >= 0.0);
    }
}

#[test]
fn quantitative_reference_fixture_is_well_formed() {
    let fixture = include_str!("data/jones2013_reference_cases.csv");
    let mut checked = 0usize;

    for line in fixture.lines().skip(1).filter(|line| !line.trim().is_empty()) {
        let columns: Vec<_> = line.split(',').collect();
        assert_eq!(columns.len(), 12, "reference fixture schema changed: {line}");

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
        checked += 1;
    }

    assert!(checked >= 8, "reference fixture should cover multiple geometries and wavelengths");
}
