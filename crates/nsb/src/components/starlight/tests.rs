use super::*;
use crate::evaluator::Target;
use crate::DEG;
use qtty::radiometry::{PhotonsPerSquareCentimeterNanosecondSteradian as BandPhotonRadiance, S10s};
use qtty::solid_angle::Steradians;
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

fn healpix_uncertainty_fixture(
    first_statistical: f64,
    first_systematic: f64,
    first_total: f64,
) -> String {
    let mut raw = String::from(concat!(
        "# map_type=healpix\n",
        "# nside=1\n",
        "# ordering=ring\n",
        "# coordinate_frame=galactic\n",
        "healpix_index,integrated_ph_cm2_ns_sr,b_s10,v_s10,",
        "statistical_uncertainty_ph_cm2_ns_sr,",
        "systematic_uncertainty_ph_cm2_ns_sr,",
        "total_uncertainty_ph_cm2_ns_sr\n",
    ));
    for index in 0..12 {
        let integrated = index as f64 + 1.0;
        let (statistical, systematic, total) = if index == 0 {
            (first_statistical, first_systematic, first_total)
        } else {
            (integrated * 0.1, integrated * 0.2, integrated * 0.25)
        };
        raw.push_str(&format!(
            "{index},{integrated},{},{},{statistical},{systematic},{total}\n",
            integrated * 10.0,
            integrated * 5.0,
        ));
    }
    raw
}

fn target(ra: f64, dec: f64) -> Target {
    Target::new(ra * DEG, dec * DEG)
}

#[test]
fn experimental_seed_is_explicitly_labelled() {
    let model = Starlight::experimental_seed_model().unwrap();
    let provenance = model.map().provenance();

    assert_eq!(
        provenance.dataset_name,
        "NSB experimental manual Galactic starlight seed"
    );
    assert_eq!(provenance.version, "v1-experimental-seed");
    assert_eq!(
        provenance.photometry_model.as_deref(),
        Some("v_s10_scaled_integrated_proxy_v1")
    );
    assert_eq!(model.map().pixels().len(), 12);

    let output = model.compute(target(266.4051, -28.936175)).unwrap();
    assert!(output.integrated.value().is_finite());
    assert!(output.integrated.value() > 0.0);
}

#[test]
fn bundled_production_model_is_available_only_with_registered_release_assets() {
    if Starlight::bundled_production_available() {
        let model = Starlight::bundled_production_model().unwrap();
        let provenance = model.map().provenance();
        assert_eq!(provenance.calibration_status.as_deref(), Some("production"));
        assert_eq!(
            provenance.photometry_model.as_deref(),
            Some("gaia_dr3_xp_photon_radiance_336_650nm_v1")
        );
        assert!(model.map().pixels().len() > 12);
    } else {
        let error = Starlight::bundled_production_model().unwrap_err();
        assert!(error
            .to_string()
            .contains("bundled production starlight asset is not registered"));
    }
}

#[test]
fn healpix_csv_fixture_loads_from_test_data_only() {
    let map =
        StarlightMap::from_csv_str(HEALPIX_FIXTURE, StarlightProvenance::test_fixture()).unwrap();

    assert_eq!(map.pixels().len(), 12);
    assert_eq!(
        map.provenance().dataset_name,
        "NSB synthetic test-only HEALPix starlight fixture"
    );
    assert_eq!(
        map.provenance().photometry_model.as_deref(),
        Some("fixture")
    );

    let output = map.lookup(Degrees::new(0.0), Degrees::new(0.0));
    assert!(output.integrated.value().is_finite());
    assert!(output.integrated.value() > 0.0);
    assert!(output.statistical_uncertainty.is_none());
    assert!(output.systematic_uncertainty.is_none());
    assert!(output.total_uncertainty.is_none());
}

#[test]
fn healpix_v2_uncertainties_parse_and_survive_lookup() {
    let raw = healpix_uncertainty_fixture(0.1, 0.2, 0.25);
    let map = StarlightMap::from_csv_str(&raw, StarlightProvenance::test_fixture()).unwrap();
    let pixel = map.pixels()[7];
    let output = map.lookup(pixel.galactic_lon, pixel.galactic_lat);

    assert_eq!(output.integrated.value(), 8.0);
    assert_eq!(output.statistical_uncertainty.unwrap().value(), 0.8);
    assert_eq!(output.systematic_uncertainty.unwrap().value(), 1.6);
    assert_eq!(output.total_uncertainty.unwrap().value(), 2.0);
    assert_eq!(output.relative_uncertainty(), Some(0.25));
}

#[test]
fn packed_candidate_header_loads_without_invented_s10() {
    let mut raw = String::from(concat!(
        "# map_type=healpix\n",
        "# nside=1\n",
        "# ordering=ring\n",
        "# coordinate_frame=galactic\n",
        "# s10_diagnostics=not_provided\n",
        "healpix_index,integrated_ph_cm2_ns_sr,",
        "statistical_uncertainty_ph_cm2_ns_sr,",
        "systematic_uncertainty_ph_cm2_ns_sr,",
        "total_uncertainty_ph_cm2_ns_sr\n",
    ));
    for index in 0..12 {
        let integrated = if index == 11 { 0.0 } else { index as f64 + 1.0 };
        raw.push_str(&format!(
            "{index},{integrated},{stat},{sys},{tot}\n",
            stat = integrated * 0.1,
            sys = integrated * 0.2,
            tot = integrated * 0.25
        ));
    }
    let map = StarlightMap::from_csv_str(&raw, StarlightProvenance::test_fixture()).unwrap();
    let occupied = map.lookup(map.pixels()[0].galactic_lon, map.pixels()[0].galactic_lat);
    assert_eq!(occupied.integrated.value(), 1.0);
    assert!(!occupied.s10_diagnostics_provided);
    assert_eq!(occupied.b_flux_s10.value(), 0.0);
    let omitted = &map.pixels()[11];
    assert_eq!(omitted.integrated.value(), 0.0);
    assert_eq!(omitted.total_uncertainty.unwrap().value(), 0.0);
}

#[test]
fn healpix_v2_rejects_negative_or_inconsistent_uncertainties() {
    let negative = healpix_uncertainty_fixture(-0.1, 0.2, 0.25);
    let error =
        StarlightMap::from_csv_str(&negative, StarlightProvenance::test_fixture()).unwrap_err();
    assert!(error.to_string().contains("uncertainty triplet"));

    let total_below_component = healpix_uncertainty_fixture(0.1, 0.3, 0.2);
    let error =
        StarlightMap::from_csv_str(&total_below_component, StarlightProvenance::test_fixture())
            .unwrap_err();
    assert!(error.to_string().contains("total >= statistical"));
}

#[test]
fn rectangular_lookup_interpolates_uncertainties() {
    let make_pixel = |lon: f64, lat: f64, integrated: f64| {
        StarlightPixel::new(
            Degrees::new(lon),
            Degrees::new(lat),
            Steradians::new(1.0),
            BandPhotonRadiance::new(integrated),
            S10s::new(integrated),
            S10s::new(integrated),
        )
        .with_uncertainties(
            BandPhotonRadiance::new(integrated * 0.1),
            BandPhotonRadiance::new(integrated * 0.2),
            BandPhotonRadiance::new(integrated * 0.25),
        )
    };
    let map = StarlightMap::from_pixels(
        vec![
            make_pixel(0.0, 0.0, 1.0),
            make_pixel(90.0, 0.0, 3.0),
            make_pixel(0.0, 90.0, 5.0),
            make_pixel(90.0, 90.0, 7.0),
        ],
        StarlightProvenance::test_fixture(),
    )
    .unwrap();

    let output = map.lookup(Degrees::new(45.0), Degrees::new(45.0));
    assert!((output.integrated.value() - 4.0).abs() < 1.0e-12);
    assert!((output.statistical_uncertainty.unwrap().value() - 0.4).abs() < 1.0e-12);
    assert!((output.systematic_uncertainty.unwrap().value() - 0.8).abs() < 1.0e-12);
    assert!((output.total_uncertainty.unwrap().value() - 1.0).abs() < 1.0e-12);
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
        .with_scale(crate::ScaleFactors::new(2.0))
        .compute(target(266.4051, -28.936175))
        .unwrap();

    assert!((scaled.integrated.value() / base.integrated.value() - 2.0).abs() < 1.0e-12);
    assert!((scaled.b_flux_s10.value() / base.b_flux_s10.value() - 2.0).abs() < 1.0e-12);
    assert!((scaled.v_flux_s10.value() / base.v_flux_s10.value() - 2.0).abs() < 1.0e-12);
}

#[test]
fn custom_scale_changes_absolute_but_not_relative_uncertainty() {
    let raw = healpix_uncertainty_fixture(0.1, 0.2, 0.25);
    let map = StarlightMap::from_csv_str(&raw, StarlightProvenance::test_fixture()).unwrap();
    let base = Starlight::with_map(map.clone())
        .compute(target(266.4051, -28.936175))
        .unwrap();
    let scaled = Starlight::with_map(map)
        .with_scale(crate::ScaleFactors::new(2.0))
        .compute(target(266.4051, -28.936175))
        .unwrap();

    assert_eq!(
        scaled.statistical_uncertainty.unwrap().value(),
        base.statistical_uncertainty.unwrap().value() * 2.0
    );
    assert_eq!(
        scaled.systematic_uncertainty.unwrap().value(),
        base.systematic_uncertainty.unwrap().value() * 2.0
    );
    assert_eq!(
        scaled.total_uncertainty.unwrap().value(),
        base.total_uncertainty.unwrap().value() * 2.0
    );
    assert_eq!(scaled.relative_uncertainty(), base.relative_uncertainty());
}

#[test]
fn invalid_maps_are_rejected() {
    let duplicate = vec![
        StarlightPixel::new(
            Degrees::new(0.0),
            Degrees::new(0.0),
            Steradians::new(1.0),
            BandPhotonRadiance::new(1.0),
            S10s::new(1.0),
            S10s::new(1.0),
        ),
        StarlightPixel::new(
            Degrees::new(360.0),
            Degrees::new(0.0),
            Steradians::new(1.0),
            BandPhotonRadiance::new(2.0),
            S10s::new(2.0),
            S10s::new(2.0),
        ),
    ];
    let err =
        StarlightMap::from_pixels(duplicate, StarlightProvenance::test_fixture()).unwrap_err();
    assert!(matches!(err, crate::NsbError::InvalidMap { .. }));
}
