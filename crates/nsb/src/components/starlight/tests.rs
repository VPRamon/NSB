// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Vallés Puig, Ramon
use super::provenance::test_support::fixture as fixture_provenance;
use super::*;
use crate::evaluator::Target;
use crate::units::radiometry::{
    PhotonsPerSquareCentimeterNanosecondSteradian as BandPhotonRadiance, S10s,
};
use crate::DEG;
use siderust::coordinates::cartesian::Direction as CartesianDirection;
use siderust::coordinates::frames::Galactic;
use siderust::coordinates::spherical;
use siderust::healpix::{HealpixGrid, HealpixIndex, HealpixOrdering, Nside};
use siderust::qtty::Degrees;

const FIXTURE: &str = include_str!("../../../tests/data/starlight_fixture_map.csv");

const HEALPIX_FIXTURE: &str = r#"# map_type=healpix
# nside=1
# ordering=ring
# coordinate_frame=galactic
# s10_diagnostics=not_provided
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
# band_definition=integrated 300-650 nm photon radiance
# smoothing_fwhm_deg=none
# generated_by=unit test
healpix_index,integrated_ph_cm2_ns_sr,statistical_uncertainty_ph_cm2_ns_sr,systematic_uncertainty_ph_cm2_ns_sr,total_uncertainty_ph_cm2_ns_sr
0,1.0,0.1,0.2,0.25
1,2.0,0.2,0.4,0.5
2,3.0,0.3,0.6,0.75
3,4.0,0.4,0.8,1.0
4,5.0,0.5,1.0,1.25
5,6.0,0.6,1.2,1.5
6,7.0,0.7,1.4,1.75
7,8.0,0.8,1.6,2.0
8,9.0,0.9,1.8,2.25
9,10.0,1.0,2.0,2.5
10,11.0,1.1,2.2,2.75
11,12.0,1.2,2.4,3.0
"#;

fn fixture_map() -> StarlightMap {
    StarlightMap::from_csv_str(FIXTURE, fixture_provenance()).unwrap()
}

fn galactic_direction(lon_deg: f64, lat_deg: f64) -> CartesianDirection<Galactic> {
    spherical::Direction::<Galactic>::new(Degrees::new(lon_deg), Degrees::new(lat_deg))
        .to_cartesian()
}

fn packed_uncertainty_fixture(
    first_statistical: f64,
    first_systematic: f64,
    first_total: f64,
) -> String {
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
        let integrated = index as f64 + 1.0;
        let (statistical, systematic, total) = if index == 0 {
            (first_statistical, first_systematic, first_total)
        } else {
            (integrated * 0.1, integrated * 0.2, integrated * 0.25)
        };
        raw.push_str(&format!(
            "{index},{integrated},{statistical},{systematic},{total}\n",
        ));
    }
    raw
}

fn target(ra: f64, dec: f64) -> Target {
    Target::new(ra * DEG, dec * DEG)
}

#[test]
fn healpix_experimental_map_is_explicitly_labelled() {
    let map = StarlightMap::from_csv_str(HEALPIX_FIXTURE, fixture_provenance()).unwrap();
    let model = Starlight::with_shared_map(std::sync::Arc::new(map));
    let provenance = model.map().provenance();

    assert_eq!(
        provenance.dataset_name,
        "NSB synthetic test-only HEALPix starlight fixture"
    );
    assert_eq!(provenance.version, "fixture");
    assert_eq!(provenance.photometry_model.as_deref(), Some("fixture"));
    assert_eq!(model.map().pixels().len(), 12);

    let output = model.compute(target(266.4051, -28.936175)).unwrap();
    assert!(output.integrated.value().is_finite());
    assert!(output.integrated.value() > 0.0);
}

#[test]
fn bundled_production_model_is_available_only_with_registered_release_assets() {
    if StarlightProduct::bundled_production_available() {
        let model = Starlight::bundled_production_model().unwrap();
        let provenance = model.map().provenance();
        assert_eq!(provenance.calibration_status.as_deref(), Some("production"));
        assert_eq!(
            provenance.photometry_model.as_deref(),
            Some("gaia_dr3_xp_photon_radiance_300_650nm_packed_v1")
        );
        assert!(model.map().pixels().len() > 12);

        let map_meta =
            crate::data::bundled::bundled_asset("starlight_nside128.compact.release.bin")
                .expect("production map metadata");
        let sidecar_meta =
            crate::data::bundled::bundled_asset("starlight_nside128.compact.manifest.toml")
                .expect("production sidecar metadata");
        assert_eq!(map_meta.schema, "nsb-healpix-starlight-binary-v1");
        assert_eq!(sidecar_meta.schema, "nsb-starlight-runtime-manifest-v1");
        assert_eq!(map_meta.calibration_status, "production");
        assert_eq!(sidecar_meta.calibration_status, "production");
        assert!(!map_meta.sha256.is_empty());
        assert!(!sidecar_meta.sha256.is_empty());
    } else {
        let error = Starlight::bundled_production_model().unwrap_err();
        assert!(error
            .to_string()
            .contains("bundled production starlight asset is not registered"));
    }
}

#[cfg(nsb_bundled_production_starlight)]
#[test]
fn bundled_production_starlight_pins_issue_207_candidate_lineage() {
    const ISSUE_207_CANDIDATE_SHA256: &str =
        "e8d31d0a04a18314e3273ffc9e2ad449014adca8710b11bda2f3b6cf2c811526";
    let sidecar = crate::data::bundled::BUNDLED_PRODUCTION_STARLIGHT_MANIFEST;
    assert!(
        sidecar.contains(ISSUE_207_CANDIDATE_SHA256),
        "bundled production sidecar must pin the issue #207 Combined300To650 candidate"
    );
    let map = crate::data::bundled::bundled_asset("starlight_nside128.compact.release.bin")
        .expect("production map metadata");
    let sidecar_meta =
        crate::data::bundled::bundled_asset("starlight_nside128.compact.manifest.toml")
            .expect("production sidecar metadata");
    assert_eq!(map.sha256.len(), 64);
    assert_eq!(sidecar_meta.sha256.len(), 64);
    assert_ne!(map.sha256, ISSUE_207_CANDIDATE_SHA256);

    let model = Starlight::bundled_production_model().unwrap();
    let supplement = model
        .map()
        .provenance()
        .bright_star_supplement
        .as_ref()
        .expect("runtime provenance must expose the bright-star supplement");
    assert_eq!(
        supplement.uv_completion_model_id,
        "ck04-hp-scaled-uv-300-336-v1"
    );
    assert!(supplement.spectral_route.contains("Hp-scaled CK04"));
    assert!(!supplement.input_catalogues.is_empty());
    assert!(!supplement.license_provenance.is_empty());
}

#[cfg(nsb_bundled_production_starlight)]
#[test]
fn bundled_production_model_is_the_canonical_admitted_compact_map() {
    let canonical = ValidatedStarlightMap::from_bytes_and_manifest(
        crate::data::bundled::BUNDLED_PRODUCTION_STARLIGHT_MAP,
        crate::data::bundled::BUNDLED_PRODUCTION_STARLIGHT_MANIFEST,
    )
    .unwrap();
    let bundled = Starlight::bundled_production_model().unwrap();

    assert_eq!(bundled.map().grid(), canonical.map().grid());
    assert_eq!(bundled.map().pixels(), canonical.map().pixels());
    assert_eq!(bundled.map().provenance(), canonical.map().provenance());
    assert_eq!(
        canonical.diagnostics().pixel_count,
        canonical.map().pixels().len()
    );
    assert!(canonical.diagnostics().flux_conservation_recomputed);
}

#[test]
fn healpix_csv_fixture_loads_from_test_data_only() {
    let map = StarlightMap::from_csv_str(HEALPIX_FIXTURE, fixture_provenance()).unwrap();

    assert_eq!(map.pixels().len(), 12);
    assert_eq!(
        map.provenance().dataset_name,
        "NSB synthetic test-only HEALPix starlight fixture"
    );
    assert_eq!(
        map.provenance().photometry_model.as_deref(),
        Some("fixture")
    );

    let output = map.lookup(galactic_direction(0.0, 0.0));
    assert!(output.integrated.value().is_finite());
    assert!(output.integrated.value() > 0.0);
    assert!(output.statistical_uncertainty.is_some());
    assert!(!output.s10_diagnostics_provided);
}

#[test]
fn packed_uncertainties_parse_and_survive_lookup() {
    let raw = packed_uncertainty_fixture(0.1, 0.2, 0.25);
    let map = StarlightMap::from_csv_str(&raw, fixture_provenance()).unwrap();
    let output = map.lookup(
        map.pixel_direction(HealpixIndex::new(7))
            .unwrap()
            .to_cartesian(),
    );

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
    let map = StarlightMap::from_csv_str(&raw, fixture_provenance()).unwrap();
    let occupied = map.lookup(
        map.pixel_direction(HealpixIndex::new(0))
            .unwrap()
            .to_cartesian(),
    );
    assert_eq!(occupied.integrated.value(), 1.0);
    assert!(!occupied.s10_diagnostics_provided);
    assert_eq!(occupied.b_flux_s10.value(), 0.0);
    let omitted = &map.pixels()[11];
    assert_eq!(omitted.integrated.value(), 0.0);
    assert_eq!(omitted.total_uncertainty.unwrap().value(), 0.0);
}

#[test]
fn packed_rejects_negative_or_inconsistent_uncertainties() {
    let negative = packed_uncertainty_fixture(-0.1, 0.2, 0.25);
    let error = StarlightMap::from_csv_str(&negative, fixture_provenance()).unwrap_err();
    assert!(error.to_string().contains("uncertainty triplet"));

    let total_below_component = packed_uncertainty_fixture(0.1, 0.3, 0.2);
    let error =
        StarlightMap::from_csv_str(&total_below_component, fixture_provenance()).unwrap_err();
    assert!(error.to_string().contains("total >= statistical"));
}

#[test]
fn map_lookup_is_directional_nearest_neighbor() {
    let map = fixture_map();
    let plane = map.lookup(galactic_direction(0.0, 0.0));
    let pole = map.lookup(galactic_direction(0.0, 90.0));
    assert!(plane.integrated > pole.integrated);
    assert!(!plane.s10_diagnostics_provided);

    let equatorial = map.lookup(galactic_direction(45.0, 0.0));
    assert!((equatorial.integrated.value() - 4.0).abs() < 1.0e-12);
}

#[test]
fn model_compute_depends_on_target() {
    let model = Starlight::with_shared_map(std::sync::Arc::new(fixture_map()));
    let galactic_center = model.compute(target(266.4051, -28.936175)).unwrap();
    let north_pole = model.compute(target(192.85948, 27.12825)).unwrap();

    assert!(galactic_center.integrated > north_pole.integrated);
}

#[test]
fn custom_scale_changes_outputs() {
    let base = Starlight::with_shared_map(std::sync::Arc::new(fixture_map()))
        .compute(target(266.4051, -28.936175))
        .unwrap();
    let scaled = super::model::test_support::with_scale(
        Starlight::with_shared_map(std::sync::Arc::new(fixture_map())),
        crate::units::ScaleFactors::new(2.0),
    )
    .compute(target(266.4051, -28.936175))
    .unwrap();

    assert!((scaled.integrated.value() / base.integrated.value() - 2.0).abs() < 1.0e-12);
}

#[test]
fn custom_scale_changes_absolute_but_not_relative_uncertainty() {
    let raw = packed_uncertainty_fixture(0.1, 0.2, 0.25);
    let map = StarlightMap::from_csv_str(&raw, fixture_provenance()).unwrap();
    let base = Starlight::with_shared_map(std::sync::Arc::new(map.clone()))
        .compute(target(266.4051, -28.936175))
        .unwrap();
    let scaled = super::model::test_support::with_scale(
        Starlight::with_shared_map(std::sync::Arc::new(map)),
        crate::units::ScaleFactors::new(2.0),
    )
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
fn incomplete_healpix_maps_are_rejected() {
    let grid = HealpixGrid::new(Nside::new(1).unwrap(), HealpixOrdering::Ring).unwrap();
    let pixel = StarlightPixel::new(BandPhotonRadiance::new(1.0), S10s::new(0.0), S10s::new(0.0))
        .without_s10_diagnostics()
        .with_uncertainties(
            BandPhotonRadiance::new(0.1),
            BandPhotonRadiance::new(0.2),
            BandPhotonRadiance::new(0.25),
        );
    let err = StarlightMap::from_healpix(grid, vec![pixel], fixture_provenance()).unwrap_err();
    assert!(matches!(err, crate::NsbError::InvalidMap { .. }));
}

#[test]
fn unsupported_csv_header_is_rejected() {
    let raw = "pixel,value\n0,1.0\n";
    let error = StarlightMap::from_csv_str(raw, fixture_provenance()).unwrap_err();
    assert!(error
        .to_string()
        .contains("unsupported starlight map header"));
}

#[test]
fn mixed_uncertainty_schema_is_rejected() {
    let grid = HealpixGrid::new(Nside::new(1).unwrap(), HealpixOrdering::Ring).unwrap();
    let with_uncertainty =
        StarlightPixel::new(BandPhotonRadiance::new(1.0), S10s::new(0.0), S10s::new(0.0))
            .without_s10_diagnostics()
            .with_uncertainties(
                BandPhotonRadiance::new(0.1),
                BandPhotonRadiance::new(0.2),
                BandPhotonRadiance::new(0.25),
            );
    let mut pixels = vec![with_uncertainty; 12];
    pixels[5] = StarlightPixel::new(BandPhotonRadiance::new(1.0), S10s::new(0.0), S10s::new(0.0))
        .without_s10_diagnostics();
    let err = StarlightMap::from_healpix(grid, pixels, fixture_provenance()).unwrap_err();
    assert!(err.to_string().contains("same uncertainty schema"));
}

#[test]
fn xp_statistical_only_sigma_survives_public_pixel_adapter() {
    use sha2::{Digest, Sha256};
    let payload = serde_json::json!({
        "schema": "nsb-starlight-spectral-map-v2",
        "product_id": "synthetic-xp-statistical-only",
        "band_nm": [360, 420], "bin_edges_nm": [360, 390, 420],
        "nside": 1, "ordering": "ring", "coordinate_frame": "galactic",
        "domain": "top-of-atmosphere", "units": "photon/(cm2 ns sr)",
        "wavelength_convention": "vacuum", "maturity": "experimental",
        "provenance": {
            "source_catalogue": "synthetic", "source_license": "test-only",
            "source_selection": "all", "generation_method": "test",
            "limitations": ["not science"]
        },
        "catalogue_rows": 12, "admitted_sources": 12,
        "excluded_sources": {},
        "pixels": (0..12).map(|i| serde_json::json!({
            "pixel": i, "sources": 1,
            "radiance": [1.0, 2.0],
            "statistical_sigma": [0.2, 0.4],
            "integrated_statistical_sigma": 0.5
        })).collect::<Vec<_>>()
    });
    let bytes = serde_json::to_vec(&payload).unwrap();
    let hash = Sha256::digest(&bytes);
    let expected = hash.iter().map(|b| format!("{b:02x}")).collect::<String>();
    let map = StarlightMap::from_json_bytes(&bytes, &expected).unwrap();
    assert_eq!(map.band_nm(), [360, 420]);
    assert_eq!(map.bin_edges_nm(), [360, 390, 420]);
    assert_eq!(map.nside(), 1);
    assert_eq!(map.admitted_sources(), Some(12));
    assert_eq!(map.maturity(), Some("experimental"));
    assert_eq!(map.sha256(), Some(expected.as_str()));
    let path =
        std::env::temp_dir().join(format!("nsb-spectral-public-{}.json", std::process::id()));
    std::fs::write(&path, &bytes).unwrap();
    let from_file = StarlightMap::from_json_file(&path, &expected).unwrap();
    std::fs::remove_file(path).unwrap();
    let direction = galactic_direction(0.0, 0.0);
    assert_eq!(
        from_file
            .top_of_atmosphere_pixel(direction)
            .unwrap()
            .0
            .value(),
        3.0
    );
    let (flux, sigma) = map
        .integrate_band(
            direction,
            siderust::qtty::Nanometers::new(360.0),
            siderust::qtty::Nanometers::new(420.0),
        )
        .unwrap();
    assert_eq!((flux.value(), sigma.unwrap().value()), (3.0, 0.5));
    let geometry =
        crate::transport::DirectPathGeometry::new(siderust::qtty::Degrees::new(0.0)).unwrap();
    let atmosphere = crate::site::AtmosphericConditions::clear_sky_with_pressure(
        siderust::qtty::Hectopascals::new(790.0),
    )
    .unwrap();
    let bounds = map
        .direct_pixel_bounds(direction, geometry, atmosphere)
        .unwrap();
    assert!(
        bounds.lower <= bounds.midpoint
            && bounds.midpoint <= bounds.upper
            && bounds.upper.value() < 3.0
    );
    let legacy = fixture_map();
    assert_eq!(legacy.band_nm(), [300, 650]);
    assert_eq!(legacy.bin_edges_nm(), [300, 650]);
    assert!(legacy.admitted_sources().is_none());
    assert!(legacy
        .integrate_band(
            direction,
            siderust::qtty::Nanometers::new(360.0),
            siderust::qtty::Nanometers::new(420.0)
        )
        .is_err());
    assert_eq!(
        legacy
            .integrate_band(
                direction,
                siderust::qtty::Nanometers::new(300.0),
                siderust::qtty::Nanometers::new(650.0)
            )
            .unwrap(),
        legacy.top_of_atmosphere_pixel(direction).unwrap()
    );
    assert!(legacy
        .direct_pixel_bounds(direction, geometry, atmosphere)
        .is_err());
    let pixel = map.try_pixel_at(galactic_direction(0.0, 0.0)).unwrap();
    assert_eq!(pixel.integrated.value(), 3.0);
    assert_eq!(pixel.statistical_uncertainty.unwrap().value(), 0.5);
    assert!(pixel.systematic_uncertainty.is_none());
    assert!(pixel.total_uncertainty.is_none());
    let output = map.lookup(galactic_direction(0.0, 0.0));
    assert_eq!(output.statistical_uncertainty.unwrap().value(), 0.5);
    assert!(output.relative_uncertainty().is_none());
    assert!(!output.s10_diagnostics_provided);
    let model = Starlight::with_shared_map(std::sync::Arc::new(map));
    assert!(matches!(
        model.compute(target(0.0, 0.0)),
        Err(crate::NsbError::Unsupported(_))
    ));
}

#[test]
fn incomplete_starlight_bv_is_visible_in_evaluator_result() {
    use crate::{ComponentMask, NsbEvaluator, NsbModelConfig, PointQuery};
    use chrono::{DateTime, Utc};
    use tempoch::{Time, UTC};
    let map = fixture_map();
    let config = NsbModelConfig::generic_clear_sky()
        .with_starlight_product(StarlightProduct::with_experimental_map(map));
    let evaluator = NsbEvaluator::with_config(config).unwrap();
    let time = Time::<UTC>::from_chrono(
        DateTime::parse_from_rfc3339("2023-09-04T01:48:00Z")
            .unwrap()
            .with_timezone(&Utc),
    );
    let observer = siderust::catalogs::observatories::EL_PARANAL.geodetic();
    let query = PointQuery::new(observer, time, target(266.4051, -28.936175))
        .with_components(ComponentMask::STARLIGHT);
    let result = evaluator.evaluate(&query).unwrap();
    assert!(result.integrated.value() > 0.0);
    assert!(result.b_mag.value().is_nan());
    assert!(result.v_mag.value().is_nan());
    assert!(!result.components[0].bv_diagnostics_available);
    assert_eq!(
        result.band_diagnostic.convention,
        "monochromatic-central-wavelength-s10-proxy-incomplete"
    );
}

#[test]
fn uncertainty_schema_rejects_mixing_full_and_statistical_only_pixels() {
    let grid = HealpixGrid::new(Nside::new(1).unwrap(), HealpixOrdering::Ring).unwrap();
    let full = StarlightPixel::new(BandPhotonRadiance::new(1.0), S10s::new(0.0), S10s::new(0.0))
        .without_s10_diagnostics()
        .with_uncertainties(
            BandPhotonRadiance::new(0.1),
            BandPhotonRadiance::new(0.2),
            BandPhotonRadiance::new(0.25),
        );
    let mut pixels = vec![full; 12];
    pixels[3].systematic_uncertainty = None;
    pixels[3].total_uncertainty = None;
    let result = StarlightMap::from_healpix(grid, pixels, fixture_provenance());
    assert!(matches!(result, Err(crate::NsbError::InvalidMap { .. })));
}

#[test]
fn try_pixel_at_matches_legacy_lookup_for_finite_directions() {
    let map = fixture_map();
    for (lon, lat) in [(0.0, 0.0), (45.0, 30.0), (359.0, -40.0)] {
        assert_eq!(
            map.try_pixel_at(galactic_direction(lon, lat)).unwrap(),
            map.pixel_at(galactic_direction(lon, lat))
        );
    }
}

#[test]
fn statistical_only_scaling_preserves_unknown_systematics_and_missing_photometry() {
    let grid = HealpixGrid::ring(Nside::new(1).unwrap()).unwrap();
    let mut pixel =
        StarlightPixel::new(BandPhotonRadiance::new(4.0), S10s::new(0.0), S10s::new(0.0))
            .without_s10_diagnostics();
    pixel.statistical_uncertainty = Some(BandPhotonRadiance::new(0.5));
    let map = StarlightMap::from_healpix(grid, vec![pixel; 12], fixture_provenance()).unwrap();
    let measured = map.lookup(galactic_direction(0.0, 0.0));
    for factor in [0.0, 0.5, 2.0] {
        let scaled =
            super::photometry::scale_outputs(measured, crate::units::ScaleFactors::new(factor));
        assert_eq!(scaled.integrated.value(), 4.0 * factor);
        assert_eq!(
            scaled.statistical_uncertainty.unwrap().value(),
            0.5 * factor
        );
        assert!(scaled.systematic_uncertainty.is_none() && scaled.total_uncertainty.is_none());
        assert!(scaled.relative_uncertainty().is_none());
        assert!(!scaled.s10_diagnostics_provided);
    }
}

#[test]
fn pixel_uncertainty_presence_states_are_validated_without_loss() {
    let grid = HealpixGrid::ring(Nside::new(1).unwrap()).unwrap();
    for mask in 0..8 {
        let mut pixel =
            StarlightPixel::new(BandPhotonRadiance::new(4.0), S10s::new(1.0), S10s::new(2.0));
        pixel.statistical_uncertainty = (mask & 1 != 0).then(|| BandPhotonRadiance::new(0.1));
        pixel.systematic_uncertainty = (mask & 2 != 0).then(|| BandPhotonRadiance::new(0.2));
        pixel.total_uncertainty = (mask & 4 != 0).then(|| BandPhotonRadiance::new(0.25));
        let result = StarlightMap::from_healpix(grid, vec![pixel; 12], fixture_provenance());
        if matches!(mask, 0 | 1 | 7) {
            let map = result.unwrap();
            assert_eq!(map.pixel_at(galactic_direction(0.0, 0.0)), pixel);
            let output = map.lookup(galactic_direction(0.0, 0.0));
            assert!(output.is_finite_non_negative());
            assert_eq!(
                output.statistical_uncertainty,
                pixel.statistical_uncertainty
            );
            assert_eq!(output.systematic_uncertainty, pixel.systematic_uncertainty);
            assert_eq!(output.total_uncertainty, pixel.total_uncertainty);
        } else {
            let error = result.unwrap_err();
            assert!(matches!(error, crate::NsbError::InvalidMap { .. }));
            assert!(
                error.to_string().contains("uncertainty"),
                "mask={mask}: {error}"
            );
        }
    }
}

#[test]
fn pixel_uncertainty_values_reject_nonfinite_negative_and_inconsistent_sigmas() {
    let grid = HealpixGrid::ring(Nside::new(1).unwrap()).unwrap();
    for bad in [f64::NAN, f64::INFINITY, -0.1] {
        for channel in 0..3 {
            let mut values = [0.1, 0.2, 0.25];
            values[channel] = bad;
            let pixel =
                StarlightPixel::new(BandPhotonRadiance::new(1.0), S10s::new(0.0), S10s::new(0.0))
                    .with_uncertainties(
                        BandPhotonRadiance::new(values[0]),
                        BandPhotonRadiance::new(values[1]),
                        BandPhotonRadiance::new(values[2]),
                    );
            assert!(
                StarlightMap::from_healpix(grid, vec![pixel; 12], fixture_provenance()).is_err()
            );
            if channel == 0 {
                let mut statistical_only = pixel;
                statistical_only.systematic_uncertainty = None;
                statistical_only.total_uncertainty = None;
                assert!(StarlightMap::from_healpix(
                    grid,
                    vec![statistical_only; 12],
                    fixture_provenance()
                )
                .is_err());
            }
        }
    }
    for (radiance, sigmas, valid) in [
        (1.0, [0.3, 0.1, 0.2], false),
        (1.0, [0.1, 0.3, 0.2], false),
        (0.0, [0.0, 0.0, 0.0], true),
    ] {
        let pixel = StarlightPixel::new(
            BandPhotonRadiance::new(radiance),
            S10s::new(0.0),
            S10s::new(0.0),
        )
        .with_uncertainties(
            BandPhotonRadiance::new(sigmas[0]),
            BandPhotonRadiance::new(sigmas[1]),
            BandPhotonRadiance::new(sigmas[2]),
        );
        let result = StarlightMap::from_healpix(grid, vec![pixel; 12], fixture_provenance());
        assert_eq!(result.is_ok(), valid);
        if let Ok(map) = result {
            assert_eq!(
                map.lookup(galactic_direction(0.0, 0.0))
                    .relative_uncertainty(),
                Some(0.0)
            );
        }
    }
}

#[test]
fn pixel_radiance_validation_rejects_nonfinite_and_negative_original_fields() {
    let grid = HealpixGrid::ring(Nside::new(1).unwrap()).unwrap();
    for bad in [f64::NAN, f64::INFINITY, -1.0] {
        for channel in 0..3 {
            let mut values = [1.0; 3];
            values[channel] = bad;
            let pixel = StarlightPixel::new(
                BandPhotonRadiance::new(values[0]),
                S10s::new(values[1]),
                S10s::new(values[2]),
            );
            assert!(
                StarlightMap::from_healpix(grid, vec![pixel; 12], fixture_provenance()).is_err()
            );
        }
    }
}
