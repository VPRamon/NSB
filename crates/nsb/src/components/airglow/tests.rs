use super::calibration::load_builtin_standard;
use super::extinction::{effective_airglow_airmass, noll_scattering_factors};
use super::*;
use crate::components::moonlight::AtmosphericConditions;
use crate::site::SiteProfileId;
use chrono::{DateTime, Utc};
use qtty::radiometry::PhotonsPerSquareCentimeterNanosecondSteradian as BandPhotonRadiance;
use siderust::catalogs::observatories;
use siderust::coordinates::centers::Geodetic;
use siderust::coordinates::frames::{EquatorialMeanJ2000, ECEF};
use siderust::coordinates::spherical::Direction as SphericalDirection;
use siderust::qtty::{Degrees, Kilometers, Meters, Nanometers};
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
    Geodetic::new_raw(
        Degrees::new(0.0),
        Degrees::new(latitude_deg),
        Meters::new(0.0),
    )
}

fn synthetic_vertical_profile() -> VerticalEmissionProfile {
    VerticalEmissionProfile::new(VerticalEmissionProfileDefinition {
        schema_version: VERTICAL_EMISSION_PROFILE_SCHEMA_VERSION,
        profile_id: "synthetic-airglow-integration-test".into(),
        altitude_km: vec![
            Kilometers::new(75.0),
            Kilometers::new(85.0),
            Kilometers::new(95.0),
            Kilometers::new(110.0),
        ],
        relative_emissivity: vec![0.0, 0.7, 1.0, 0.0],
        normalization: VerticalProfileNormalization::UnitVerticalIntegral,
        wavelength: AirglowWavelengthApplicability {
            min: Nanometers::new(300.0),
            max: Nanometers::new(650.0),
            band: "synthetic-300-650-nm".into(),
        },
        assumptions: "synthetic broad optical layer for software integration tests".into(),
        provenance: "deterministic NSB test fixture; not observational data".into(),
        license: "CC0-1.0 synthetic fixture".into(),
        validated_zenith: ValidatedZenithDomain {
            min: Degrees::new(0.0),
            max: Degrees::new(90.0),
        },
    })
    .unwrap()
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
        .with_scale(crate::ScaleFactors::new(2.0))
        .compute(time, target)
        .unwrap();

    let ratio = scaled.integrated.value() / base.integrated.value();
    assert!((ratio - 2.0).abs() < 1e-12);
}

#[test]
fn season_bin_changes_with_longitude_near_month_boundary() {
    // `season_bin` uses local-solar month derived from longitude; we pick a time where
    // rounding pushes the local month across a boundary.
    let time = t("2023-03-31T18:00:00Z");
    let east = Geodetic::new_raw(Degrees::new(179.0), Degrees::new(0.0), Meters::new(0.0));
    let west = Geodetic::new_raw(Degrees::new(-179.0), Degrees::new(0.0), Meters::new(0.0));

    let east_bin = super::temporal::season_bin(time, east);
    let west_bin = super::temporal::season_bin(time, west);

    assert_ne!(east_bin, west_bin);
    // 2023-03 => season bin 2, 2023-04 => season bin 3 per `temporal.rs::season_bin`.
    assert_eq!(west_bin, 2);
    assert_eq!(east_bin, 3);
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
        .with_atmosphere(profile.atmosphere)
        .with_scale(profile.airglow.scale)
        .compute(time, target)
        .unwrap();

    assert_eq!(profile.airglow.scale, crate::ScaleFactors::new(1.0));
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
fn geometry_selection_changes_only_the_geometry_multiplier() {
    let location = Geodetic::new_raw(
        Degrees::new(44.0),
        Degrees::new(-11.0),
        Meters::new(1_234.0),
    );
    let altitude = Degrees::new(15.0);
    let zenith = Degrees::new(75.0);
    let continuum = load_builtin_standard().unwrap();
    let atmosphere = AtmosphericConditions::generic_clear_sky(location);
    let van = AirglowGeometryModel::default();
    let vertical = AirglowGeometryModel::VerticalProfile(synthetic_vertical_profile());
    let van_factor = van.geometry_factor(location, zenith).unwrap().value();
    let vertical_factor = vertical.geometry_factor(location, zenith).unwrap().value();

    let evaluate = |geometry: AirglowGeometryModel| {
        super::continuum::evaluate_continuum_with_time_bin(
            &continuum,
            t("2023-09-04T01:48:00Z"),
            altitude,
            super::continuum::AirglowEvaluationContext {
                location,
                atmosphere,
                geometry: geometry.clone(),
                solar_radio_flux: DEFAULT_SOLAR_RADIO_FLUX,
                user_scale: crate::ScaleFactors::new(1.0),
            },
            1,
        )
        .unwrap()
    };
    let van_output = evaluate(van);
    let vertical_output = evaluate(vertical);
    let expected_ratio = vertical_factor / van_factor;

    for ratio in [
        vertical_output.integrated.value() / van_output.integrated.value(),
        vertical_output.b_flux_s10.value() / van_output.b_flux_s10.value(),
        vertical_output.v_flux_s10.value() / van_output.v_flux_s10.value(),
    ] {
        assert!((ratio - expected_ratio).abs() < 1.0e-12);
    }
    assert_eq!(
        van_output.relative_uncertainty,
        vertical_output.relative_uncertainty,
        "multiplicative geometry must propagate through signal and absolute uncertainty consistently"
    );
}

#[test]
fn vertical_profile_runs_through_normal_airglow_api_at_arbitrary_location() {
    let location = Geodetic::new_raw(Degrees::new(18.4), Degrees::new(-33.9), Meters::new(120.0));
    let model = Airglow::standard_clear_sky(location)
        .unwrap()
        .with_geometry(AirglowGeometryModel::VerticalProfile(
            synthetic_vertical_profile(),
        ));
    assert_eq!(model.geometry().model_id(), "vertical_profile");
    let out = model
        .compute(t("2023-06-21T22:00:00Z"), target(200.0, -45.0))
        .unwrap();
    assert!(out.integrated.value().is_finite() && out.integrated.value() >= 0.0);
}

#[test]
fn below_horizon_contract_clamps_to_horizon_for_both_geometry_models() {
    let location = cta_s();
    let continuum = load_builtin_standard().unwrap();
    let atmosphere = AtmosphericConditions::generic_clear_sky(location);
    for geometry in [
        AirglowGeometryModel::default(),
        AirglowGeometryModel::VerticalProfile(synthetic_vertical_profile()),
    ] {
        let evaluate = |altitude| {
            super::continuum::evaluate_continuum_with_time_bin(
                &continuum,
                t("2023-09-04T04:00:00Z"),
                Degrees::new(altitude),
                super::continuum::AirglowEvaluationContext {
                    location,
                    atmosphere,
                    geometry: geometry.clone(),
                    solar_radio_flux: DEFAULT_SOLAR_RADIO_FLUX,
                    user_scale: crate::ScaleFactors::new(1.0),
                },
                1,
            )
            .unwrap()
        };
        let horizon = evaluate(0.0);
        let below = evaluate(-20.0);
        assert_eq!(
            horizon.integrated.value().to_bits(),
            below.integrated.value().to_bits()
        );
    }
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
        super::temporal::time_of_night_bin_for_test(t("2023-06-21T12:00:00Z"), high_arctic(78.0),),
        None
    );
}

fn airglow_ctx(
    location: Geodetic<ECEF>,
    atmosphere: AtmosphericConditions,
) -> super::continuum::AirglowEvaluationContext {
    super::continuum::AirglowEvaluationContext {
        location,
        atmosphere,
        geometry: AirglowGeometryModel::default(),
        solar_radio_flux: DEFAULT_SOLAR_RADIO_FLUX,
        user_scale: crate::ScaleFactors::new(1.0),
    }
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
        Degrees::new(60.0),
        airglow_ctx(
            location,
            AtmosphericConditions::generic_clear_sky(paranal()),
        ),
    )
    .unwrap();

    assert!(out.integrated > BandPhotonRadiance::zero());
}

#[test]
fn daytime_airglow_continuum_is_zero_outside_calibration_domain() {
    let continuum = load_builtin_standard().unwrap();
    let out = super::continuum::evaluate_continuum(
        &continuum,
        t("2023-09-04T16:00:00Z"),
        Degrees::new(60.0),
        airglow_ctx(cta_s(), AtmosphericConditions::generic_clear_sky(cta_s())),
    )
    .unwrap();

    assert_eq!(out.integrated, BandPhotonRadiance::zero());
}

#[test]
fn invalid_altitude_returns_zero_stable_result() {
    let continuum = load_builtin_standard().unwrap();
    let out = super::continuum::evaluate_continuum(
        &continuum,
        t("2023-09-04T01:48:00Z"),
        Degrees::new(f64::NAN),
        airglow_ctx(
            paranal(),
            AtmosphericConditions::generic_clear_sky(paranal()),
        ),
    )
    .unwrap();

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
fn high_zenith_target_differs_from_zenith_due_to_geometry_and_scattering() {
    let location = paranal();
    let time = t("2023-09-04T01:48:00Z");
    let zenith_target = target(266.41683, -29.00781);
    let low_altitude_target = target(80.0, -20.0);
    let model = Airglow::standard_clear_sky(location).unwrap();
    let zenith = model.compute(time, zenith_target).unwrap();
    let low = model.compute(time, low_altitude_target).unwrap();
    assert_ne!(
        zenith.integrated.value(),
        low.integrated.value(),
        "geometry and scattering stack must change between targets"
    );
}

#[test]
fn site_profile_atmosphere_changes_airglow_at_fixed_geometry() {
    let location = paranal();
    let time = t("2023-09-04T01:48:00Z");
    let target = target(266.41683, -29.00781);
    let continuum = load_builtin_standard().unwrap();

    let low_pressure = Airglow::with_continuum(location, continuum.clone())
        .with_atmosphere(AtmosphericConditions {
            surface_pressure: siderust::qtty::Hectopascals::new(600.0),
            ..AtmosphericConditions::cta_s_clear_sky()
        })
        .compute(time, target)
        .unwrap();
    let high_pressure = Airglow::with_continuum(location, continuum)
        .with_atmosphere(AtmosphericConditions {
            surface_pressure: siderust::qtty::Hectopascals::new(900.0),
            ..AtmosphericConditions::cta_s_clear_sky()
        })
        .compute(time, target)
        .unwrap();

    assert_ne!(
        low_pressure.integrated.value(),
        high_pressure.integrated.value()
    );
}

#[test]
fn regression_noll_geometry_reference_values() {
    #[allow(clippy::approx_constant)] // Noll Eq. (25) intercept, not 1/π
    let references = [
        (0.0, 1.0, -0.146, -0.318),
        (
            30.0,
            1.149_349_365_080_909,
            -0.045_105_511_442_811,
            -0.213_297_031_647_063,
        ),
        (
            60.0,
            1.920_946_875_988_246,
            0.327_187_126_765_308,
            0.173_048_594_102_764,
        ),
        (
            75.0,
            3.277_162_524_289_61,
            0.714_366_128_417_269,
            0.574_842_501_149_617,
        ),
    ];
    for (zenith_deg, expected_x, expected_fr, expected_fm) in references {
        let zenith = Degrees::new(zenith_deg);
        let x = effective_airglow_airmass(zenith);
        let (f_r, f_m) = noll_scattering_factors(zenith);
        assert!((x - expected_x).abs() < 1e-12, "X_ag at z={zenith_deg}");
        assert!((f_r - expected_fr).abs() < 1e-12, "f_R at z={zenith_deg}");
        assert!((f_m - expected_fm).abs() < 1e-12, "f_M at z={zenith_deg}");
    }
}

#[test]
fn spectral_extinction_differs_from_unextincted_baseline_integral() {
    let continuum = load_builtin_standard().unwrap();
    let atmosphere = AtmosphericConditions::cta_s_clear_sky();
    let zenith = Degrees::new(60.0);
    let spectral = super::continuum::integrate_attenuated_continuum(&continuum, zenith, atmosphere);
    assert!(
        spectral.integrated_relative < continuum.integrated_relative_300_650,
        "60° zenith scattering should reduce the spectrally integrated continuum"
    );
}

#[test]
fn regression_paranal_integrated_values_at_representative_zeniths() {
    let location = paranal();
    let time = t("2023-09-04T01:48:00Z");
    let query_target = target(266.41683, -29.00781);
    let model = Airglow::for_site_profile(location, SiteProfileId::CtaSouth)
        .unwrap()
        .compute(time, query_target)
        .unwrap();

    // Reference from independent recomputation of the Noll scattering stack at
    // Paranal for this query geometry (CTAO-S planning atmosphere).
    assert!(
        (model.integrated.value() - 0.127_477_149_243_599_1).abs() < 1e-10,
        "zenith reference changed: {}",
        model.integrated.value()
    );

    let low = Airglow::for_site_profile(location, SiteProfileId::CtaSouth)
        .unwrap()
        .compute(time, target(80.0, -20.0))
        .unwrap();
    assert!(
        (low.integrated.value() - 0.209_872_696_340_495_5).abs() < 1e-10,
        "30° zenith reference changed: {}",
        low.integrated.value()
    );
}
