use super::calibration::load_builtin_standard;
use super::*;
use crate::site::SiteProfileId;
use chrono::{DateTime, Utc};
use qtty::radiometry::PhotonsPerSquareCentimeterNanosecondSteradian as BandPhotonRadiance;
use siderust::catalogs::observatories;
use siderust::coordinates::centers::Geodetic;
use siderust::coordinates::frames::{EquatorialMeanJ2000, ECEF};
use siderust::coordinates::spherical::Direction as SphericalDirection;
use siderust::qtty::{Degrees, Meters};
use siderust::time::{ModifiedJulianDate, TT};
use tempoch::{Time, UTC};

fn t(input: &str) -> Time<UTC> {
    Time::<UTC>::from_chrono(
        DateTime::parse_from_rfc3339(input)
            .unwrap()
            .with_timezone(&Utc),
    )
}

fn target(ra: f64, dec: f64) -> SphericalDirection<EquatorialMeanJ2000> {
    SphericalDirection::<EquatorialMeanJ2000>::new(Degrees::new(ra), Degrees::new(dec))
}

fn paranal() -> Geodetic<ECEF> {
    observatories::EL_PARANAL.geodetic()
}

fn cta_s() -> Geodetic<ECEF> {
    Geodetic::new_raw(
        Degrees::new(-70.406944),
        Degrees::new(-24.627222),
        Meters::new(2100.0),
    )
}

fn cta_n() -> Geodetic<ECEF> {
    Geodetic::new_raw(
        Degrees::new(-17.892),
        Degrees::new(28.762),
        Meters::new(2396.0),
    )
}

fn high_arctic(latitude_deg: f64) -> Geodetic<ECEF> {
    Geodetic::new_raw(Degrees::new(0.0), Degrees::new(latitude_deg), Meters::new(0.0))
}

fn tt_mjd_to_utc(time: ModifiedJulianDate) -> Time<UTC> {
    Time::<TT>::from(time).to::<UTC>()
}

fn night_phase_time(seed: Time<UTC>, location: Geodetic<ECEF>, phase: f64) -> Time<UTC> {
    let night = super::temporal::astronomical_night_for_test(seed, location)
        .expect("complete astronomical night");
    let start = night.start.raw().value();
    let end = night.end.raw().value();
    tt_mjd_to_utc(ModifiedJulianDate::new(start + (end - start) * phase))
}

fn bin_at_phase(seed: Time<UTC>, location: Geodetic<ECEF>, phase: f64) -> Option<usize> {
    super::temporal::time_of_night_bin_for_test(night_phase_time(seed, location, phase), location)
}

#[test]
fn standard_clear_sky_computes_positive_airglow() {
    let model = Airglow::standard_clear_sky(paranal()).unwrap();
    let out = model
        .compute(t("2023-09-04T01:48:00Z"), target(266.41683, -29.00781))
        .unwrap();

    assert!(out.integrated > BandPhotonRadiance::zero());
    assert!(out.b_flux_s10.value() > 0.0);
    assert!(out.v_flux_s10.value() > 0.0);
}

#[test]
fn compute_is_geometry_sensitive() {
    let location = paranal();
    let time = t("2023-09-04T01:48:00Z");
    let model = Airglow::standard_clear_sky(location).unwrap();
    let high = model.compute(time, target(266.41683, -29.00781)).unwrap();
    let low = model.compute(time, target(80.0, -20.0)).unwrap();

    assert_ne!(high.integrated.value(), low.integrated.value());
}

#[test]
fn solar_radio_flux_is_typed_and_changes_result() {
    let location = paranal();
    let time = t("2023-09-04T01:48:00Z");
    let target = target(266.41683, -29.00781);

    let low = Airglow::standard_clear_sky(location)
        .unwrap()
        .with_solar_radio_flux(SolarFluxUnits::new(50.0))
        .compute(time, target)
        .unwrap();
    let high = Airglow::standard_clear_sky(location)
        .unwrap()
        .with_solar_radio_flux(SolarFluxUnits::new(250.0))
        .compute(time, target)
        .unwrap();

    assert_ne!(low.integrated.value(), high.integrated.value());
}

#[test]
fn scale_changes_result() {
    let location = paranal();
    let time = t("2023-09-04T01:48:00Z");
    let target = target(266.41683, -29.00781);

    let base = Airglow::standard_clear_sky(location)
        .unwrap()
        .compute(time, target)
        .unwrap();
    let scaled = Airglow::standard_clear_sky(location)
        .unwrap()
        .with_scale(2.0)
        .compute(time, target)
        .unwrap();

    let ratio = scaled.integrated.value() / base.integrated.value();
    assert!((ratio - 2.0).abs() < 1e-12);
}

#[test]
fn site_profile_airglow_constructor_matches_profile_scale() {
    let location = cta_n();
    let time = t("2023-09-04T02:00:00Z");
    let target = target(266.41683, -29.00781);
    let profile = SiteProfileId::CtaNorth.profile(location);

    let from_profile = Airglow::for_site_profile(location, SiteProfileId::CtaNorth)
        .unwrap()
        .compute(time, target)
        .unwrap();
    let explicit = Airglow::with_continuum(location, load_builtin_standard().unwrap())
        .with_scale(profile.airglow.scale)
        .compute(time, target)
        .unwrap();

    assert_eq!(profile.airglow.scale, 1.0);
    assert_eq!(from_profile.integrated.value(), explicit.integrated.value());
}

#[test]
fn cta_site_profile_airglow_results_are_site_sensitive() {
    let target = target(266.41683, -29.00781);
    let north = Airglow::for_site_profile(cta_n(), SiteProfileId::CtaNorth)
        .unwrap()
        .compute(t("2023-09-04T02:00:00Z"), target)
        .unwrap();
    let south = Airglow::for_site_profile(cta_s(), SiteProfileId::CtaSouth)
        .unwrap()
        .compute(t("2023-09-04T04:00:00Z"), target)
        .unwrap();

    assert!(north.integrated > BandPhotonRadiance::zero());
    assert!(south.integrated > BandPhotonRadiance::zero());
    assert_ne!(north.integrated.value(), south.integrated.value());
}

#[test]
fn custom_continuum_path_works() {
    let location = paranal();
    let time = t("2023-09-04T01:48:00Z");
    let target = target(266.41683, -29.00781);
    let continuum = load_builtin_standard().unwrap();

    let standard = Airglow::standard_clear_sky(location)
        .unwrap()
        .compute(time, target)
        .unwrap();
    let custom = Airglow::with_continuum(location, continuum)
        .compute(time, target)
        .unwrap();

    assert_eq!(standard.integrated.value(), custom.integrated.value());
}

#[test]
fn time_of_night_bins_follow_cta_s_astronomical_night_phase() {
    let location = cta_s();
    let seed = t("2023-09-04T04:00:00Z");

    assert_eq!(bin_at_phase(seed, location, 1.0 / 6.0), Some(1));
    assert_eq!(bin_at_phase(seed, location, 0.5), Some(2));
    assert_eq!(bin_at_phase(seed, location, 5.0 / 6.0), Some(3));
}

#[test]
fn time_of_night_bins_follow_cta_n_astronomical_night_phase() {
    let location = cta_n();
    let seed = t("2023-09-04T02:00:00Z");

    assert_eq!(bin_at_phase(seed, location, 1.0 / 6.0), Some(1));
    assert_eq!(bin_at_phase(seed, location, 0.5), Some(2));
    assert_eq!(bin_at_phase(seed, location, 5.0 / 6.0), Some(3));
}

#[test]
fn twilight_edges_are_outside_airglow_calibration_domain() {
    let location = cta_s();
    let seed = t("2023-09-04T04:00:00Z");
    let night = super::temporal::astronomical_night_for_test(seed, location)
        .expect("complete astronomical night");
    let one_minute_days = 1.0 / 1440.0;

    let before_dusk = tt_mjd_to_utc(ModifiedJulianDate::new(
        night.start.raw().value() - one_minute_days,
    ));
    let after_dusk = tt_mjd_to_utc(ModifiedJulianDate::new(
        night.start.raw().value() + one_minute_days,
    ));
    let before_dawn = tt_mjd_to_utc(ModifiedJulianDate::new(
        night.end.raw().value() - one_minute_days,
    ));
    let after_dawn = tt_mjd_to_utc(ModifiedJulianDate::new(
        night.end.raw().value() + one_minute_days,
    ));

    assert_eq!(
        super::temporal::time_of_night_bin_for_test(before_dusk, location),
        None
    );
    assert_eq!(
        super::temporal::time_of_night_bin_for_test(after_dusk, location),
        Some(1)
    );
    assert_eq!(
        super::temporal::time_of_night_bin_for_test(before_dawn, location),
        Some(3)
    );
    assert_eq!(
        super::temporal::time_of_night_bin_for_test(after_dawn, location),
        None
    );
}

#[test]
fn polar_summer_without_astronomical_night_has_no_time_bin() {
    assert_eq!(
        super::temporal::time_of_night_bin_for_test(
            t("2023-06-21T12:00:00Z"),
            high_arctic(78.0),
        ),
        None
    );
}

#[test]
fn polar_winter_astronomical_night_preserves_airglow() {
    let location = high_arctic(89.0);
    let time = t("2023-12-21T12:00:00Z");

    assert!(super::temporal::time_of_night_bin_for_test(time, location).is_some());

    let continuum = load_builtin_standard().unwrap();
    let out = super::continuum::evaluate_continuum(
        &continuum,
        time,
        location,
        Degrees::new(60.0),
        DEFAULT_SOLAR_RADIO_FLUX,
        1.0,
    );

    assert!(out.integrated > BandPhotonRadiance::zero());
}

#[test]
fn daytime_airglow_continuum_is_zero_outside_calibration_domain() {
    let continuum = load_builtin_standard().unwrap();
    let out = super::continuum::evaluate_continuum(
        &continuum,
        t("2023-09-04T16:00:00Z"),
        cta_s(),
        Degrees::new(60.0),
        DEFAULT_SOLAR_RADIO_FLUX,
        1.0,
    );

    assert_eq!(out.integrated, BandPhotonRadiance::zero());
}

#[test]
fn invalid_altitude_returns_zero_stable_result() {
    let continuum = load_builtin_standard().unwrap();
    let out = super::continuum::evaluate_continuum(
        &continuum,
        t("2023-09-04T01:48:00Z"),
        paranal(),
        Degrees::new(f64::NAN),
        DEFAULT_SOLAR_RADIO_FLUX,
        1.0,
    );

    assert_eq!(out.integrated, BandPhotonRadiance::zero());
}

#[test]
fn default_solar_radio_flux_is_neutral() {
    let continuum = load_builtin_standard().unwrap();
    let correction = continuum.solar_activity_const
        + continuum.solar_activity_slope * DEFAULT_SOLAR_RADIO_FLUX.value();
    assert!((correction - 1.0).abs() < 1e-12);
}

#[test]
fn legacy_polynomial_public_api_removed() {
    let model = Airglow::standard_clear_sky(paranal()).unwrap();
    let _ = model
        .compute(t("2023-09-04T01:48:00Z"), target(266.41683, -29.00781))
        .unwrap();
}
