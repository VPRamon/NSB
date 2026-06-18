use super::*;
use crate::evaluator::Target;
use crate::DEG;
use qtty::radiometry::{PhotonsPerSquareCentimeterNanosecondSteradian as BandPhotonRadiance, S10s};
use siderust::qtty::Degrees;

const FIXTURE: &str = include_str!("../../../tests/data/starlight_fixture_map.csv");

fn fixture_map() -> StarlightMap {
    StarlightMap::from_csv_str(FIXTURE, StarlightProvenance::test_fixture()).unwrap()
}

fn target(ra: f64, dec: f64) -> Target {
    Target::new(ra * DEG, dec * DEG)
}

#[test]
fn standard_model_reports_missing_data_until_real_map_is_bundled() {
    let err = Starlight::standard_galactic_model().unwrap_err();
    assert!(matches!(err, crate::NsbError::DataMissing { .. }));
}

#[test]
fn equatorial_to_galactic_matches_known_directions() {
    let center = equatorial_to_galactic(target(266.4051, -28.936175));
    let center_l = center.lon.value().min((center.lon.value() - 360.0).abs());
    assert!(center_l < 0.2, "l={}", center.lon.value());
    assert!(center.lat.value().abs() < 0.2, "b={}", center.lat.value());

    let north_pole = equatorial_to_galactic(target(192.85948, 27.12825));
    assert!(
        (north_pole.lat.value() - 90.0).abs() < 0.2,
        "b={}",
        north_pole.lat.value()
    );
}

#[test]
fn map_lookup_is_directional_and_bilinear() {
    let map = fixture_map();
    let plane = map.lookup(Degrees::new(0.0), Degrees::new(0.0));
    let pole = map.lookup(Degrees::new(0.0), Degrees::new(90.0));
    assert!(plane.integrated > pole.integrated);

    let mid = map.lookup(Degrees::new(45.0), Degrees::new(45.0));
    assert!((mid.integrated.value() - 3.0).abs() < 1.0e-12);
    assert!((mid.b_flux_s10.value() - 30.0).abs() < 1.0e-12);
    assert!((mid.v_flux_s10.value() - 15.0).abs() < 1.0e-12);
}

#[test]
fn model_compute_depends_on_target() {
    let model = Starlight::with_map(fixture_map());
    let galactic_center = model.compute(target(266.4051, -28.936175)).unwrap();
    let north_pole = model.compute(target(192.85948, 27.12825)).unwrap();

    assert!(galactic_center.integrated > north_pole.integrated);
}

#[test]
fn custom_scale_changes_outputs() {
    let base = Starlight::with_map(fixture_map())
        .compute(target(266.4051, -28.936175))
        .unwrap();
    let scaled = Starlight::with_map(fixture_map())
        .with_scale(2.0)
        .compute(target(266.4051, -28.936175))
        .unwrap();

    assert!((scaled.integrated.value() / base.integrated.value() - 2.0).abs() < 1.0e-12);
    assert!((scaled.b_flux_s10.value() / base.b_flux_s10.value() - 2.0).abs() < 1.0e-12);
    assert!((scaled.v_flux_s10.value() / base.v_flux_s10.value() - 2.0).abs() < 1.0e-12);
}

#[test]
fn invalid_maps_are_rejected() {
    let duplicate = vec![
        StarlightPixel::new(
            Degrees::new(0.0),
            Degrees::new(0.0),
            1.0,
            BandPhotonRadiance::new(1.0),
            S10s::new(1.0),
            S10s::new(1.0),
        ),
        StarlightPixel::new(
            Degrees::new(360.0),
            Degrees::new(0.0),
            1.0,
            BandPhotonRadiance::new(2.0),
            S10s::new(2.0),
            S10s::new(2.0),
        ),
    ];
    let err =
        StarlightMap::from_pixels(duplicate, StarlightProvenance::test_fixture()).unwrap_err();
    assert!(matches!(err, crate::NsbError::InvalidMap { .. }));
}
