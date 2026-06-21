use super::*;
use crate::evaluator::Target;
use crate::DEG;
use qtty::radiometry::{PhotonsPerSquareCentimeterNanosecondSteradian as BandPhotonRadiance, S10s};
use siderust::qtty::Degrees;

const FIXTURE: &str = include_str!("../../../tests/data/starlight_fixture_map.csv");

const HEALPIX_FIXTURE: &str = r#"# map_type=healpix
# nside=1
# ordering=ring
# coordinate_frame=galactic
# dataset_name=NSB synthetic test-only HEALPix starlight fixture
# version=fixture
# generation_date_utc=2026-06-21T00:00:00Z
# source_catalogue=synthetic unit-test fixture
# source_catalogue_release=test
# source_catalogue_license=test-only
# source_catalogue_checksum=sha256:fixture
# magnitude_limit=test-only
# map_resolution=HEALPix nside=1 ring 12 pixels
# photometry_model=fixture
# band_definition=integrated 300-650 nm photon radiance plus B/V S10 diagnostics
# smoothing_fwhm_deg=none
# generated_by=unit test
healpix_index,integrated_ph_cm2_ns_sr,b_s10,v_s10
0,1.0,10.0,5.0
1,2.0,20.0,10.0
2,3.0,30.0,15.0
3,4.0,40.0,20.0
4,5.0,50.0,25.0
5,6.0,60.0,30.0
6,7.0,70.0,35.0
7,8.0,80.0,40.0
8,9.0,90.0,45.0
9,10.0,100.0,50.0
10,11.0,110.0,55.0
11,12.0,120.0,60.0
"#;

fn fixture_map() -> StarlightMap {
    StarlightMap::from_csv_str(FIXTURE, StarlightProvenance::test_fixture()).unwrap()
}

fn target(ra: f64, dec: f64) -> Target {
    Target::new(ra * DEG, dec * DEG)
}

#[test]
fn catalogue_model_fails_until_real_map_is_bundled() {
    let err = Starlight::catalogue_galactic_model().unwrap_err();
    match err {
        crate::NsbError::DataMissing { file, .. } => {
            assert_eq!(file, "data/starlight_galactic_map_v1.csv");
        }
        other => panic!("expected missing catalogue map, got {other:?}"),
    }
}

#[test]
fn healpix_csv_fixture_loads_from_test_data_only() {
    let map = StarlightMap::from_csv_str(HEALPIX_FIXTURE, StarlightProvenance::test_fixture())
        .unwrap();

    assert_eq!(map.pixels().len(), 12);
    assert_eq!(
        map.provenance().dataset_name,
        "NSB synthetic test-only HEALPix starlight fixture"
    );
    assert_eq!(map.provenance().photometry_model.as_deref(), Some("fixture"));

    let output = map.lookup(Degrees::new(0.0), Degrees::new(0.0));
    assert!(output.integrated.value().is_finite());
    assert!(output.integrated.value() > 0.0);
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
