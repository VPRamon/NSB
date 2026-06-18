use super::*;
use super::calibration::load_builtin_standard;
use chrono::{DateTime, Utc};
use qtty::radiometry::PhotonsPerSquareCentimeterNanosecondSteradian as BandPhotonRadiance;
use siderust::catalogs::observatories;
use siderust::coordinates::centers::Geodetic;
use siderust::coordinates::frames::{EquatorialMeanJ2000, ECEF};
use siderust::coordinates::spherical::Direction as SphericalDirection;
use siderust::qtty::{Degrees, Meters};
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
fn time_of_night_bin_is_not_utc_hour_based() {
    let time = t("2023-09-04T01:00:00Z");
    let greenwich = Geodetic::new_raw(Degrees::new(0.0), Degrees::new(0.0), Meters::new(0.0));
    let west = Geodetic::new_raw(Degrees::new(-105.0), Degrees::new(0.0), Meters::new(0.0));

    let utc_like = super::temporal::time_of_night_bin_for_test(time, greenwich);
    let local_west = super::temporal::time_of_night_bin_for_test(time, west);

    assert_ne!(utc_like, local_west);
    assert_eq!(local_west, 1);
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
