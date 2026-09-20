use chrono::{DateTime, Utc};
use nsb::{
    ComponentCalibrationStatus, ComponentMask, MoonlightModel, NsbComponent, NsbEvaluator,
    NsbModelConfig, PointQuery, SiteProfileId, Target, DEG,
};
use siderust::catalogs::observatories;
use siderust::coordinates::centers::Geodetic;
use siderust::coordinates::frames::ECEF;
use siderust::qtty::{Degrees, Meters};
use tempoch::{Time, UTC};

fn parse_time(value: &str) -> Time<UTC> {
    Time::<UTC>::from_chrono(
        DateTime::parse_from_rfc3339(value)
            .unwrap()
            .with_timezone(&Utc),
    )
}

fn time() -> Time<UTC> {
    parse_time("2023-09-04T01:48:00Z")
}

fn profile_time() -> Time<UTC> {
    parse_time("2023-09-29T03:00:00Z")
}

fn target() -> Target {
    Target::new(266.41683 * DEG, -29.00781 * DEG)
}

fn profile_target() -> Target {
    Target::new(270.0 * DEG, -30.0 * DEG)
}

fn paranal() -> Geodetic<ECEF> {
    observatories::EL_PARANAL.geodetic()
}

fn arbitrary_observer() -> Geodetic<ECEF> {
    Geodetic::new_raw(Degrees::new(18.4), Degrees::new(-33.9), Meters::new(120.0))
}

fn evaluate_moonlight_at(
    config: NsbModelConfig,
    observer: Geodetic<ECEF>,
    time: Time<UTC>,
    target: Target,
) -> NsbComponent {
    NsbEvaluator::with_config(config)
        .unwrap()
        .evaluate(&PointQuery::new(observer, time, target).with_components(ComponentMask::MOON))
        .unwrap()
        .components
        .into_iter()
        .next()
        .unwrap()
}

fn evaluate_moonlight(config: NsbModelConfig, observer: Geodetic<ECEF>) -> NsbComponent {
    evaluate_moonlight_at(config, observer, time(), target())
}

#[test]
fn moonlight_model_identity_and_default_are_stable() {
    let default = NsbModelConfig::default();

    assert_eq!(default.moonlight_model(), MoonlightModel::Jones2013Spectral);
    assert_eq!(
        MoonlightModel::Jones2013Spectral.as_str(),
        "jones-2013-spectral"
    );
    assert_eq!(
        MoonlightModel::KrisciunasSchaefer1991.as_str(),
        "krisciunas-schaefer-1991"
    );
}

#[test]
fn moonlight_model_builder_selects_both_supported_models() {
    for model in [
        MoonlightModel::Jones2013Spectral,
        MoonlightModel::KrisciunasSchaefer1991,
    ] {
        let config = NsbModelConfig::default().with_moonlight_model(model);
        assert_eq!(config.moonlight_model(), model);
    }
}

#[test]
fn explicit_default_selection_is_bitwise_identical_and_dispatches_to_jones() {
    let default_config = NsbModelConfig::default();
    let explicit_config =
        NsbModelConfig::default().with_moonlight_model(MoonlightModel::Jones2013Spectral);

    let default = evaluate_moonlight(default_config, paranal());
    let explicit = evaluate_moonlight(explicit_config, paranal());

    assert_eq!(
        default.integrated.value().to_bits(),
        explicit.integrated.value().to_bits()
    );
    assert_eq!(
        default.b_flux_s10.value().to_bits(),
        explicit.b_flux_s10.value().to_bits()
    );
    assert_eq!(
        default.v_flux_s10.value().to_bits(),
        explicit.v_flux_s10.value().to_bits()
    );
    assert!(explicit.metadata.provenance.contains("Jones+2013"));
}

#[test]
fn ks_selection_dispatches_to_published_reference_implementation() {
    let component = evaluate_moonlight(
        NsbModelConfig::default().with_moonlight_model(MoonlightModel::KrisciunasSchaefer1991),
        paranal(),
    );

    assert!(component.integrated.value().is_finite());
    assert_eq!(
        component.metadata.status,
        ComponentCalibrationStatus::PublishedReference
    );
    assert!(component
        .metadata
        .provenance
        .contains("Krisciunas & Schaefer 1991"));
}

#[test]
fn site_profile_selection_does_not_change_scientific_model() {
    for model in [
        MoonlightModel::Jones2013Spectral,
        MoonlightModel::KrisciunasSchaefer1991,
    ] {
        for site_profile in [
            SiteProfileId::GenericClearSky,
            SiteProfileId::CtaNorth,
            SiteProfileId::CtaSouth,
        ] {
            let config = NsbModelConfig::default()
                .with_moonlight_model(model)
                .with_site_profile(site_profile);
            assert_eq!(config.moonlight_model(), model);
        }
    }
}

#[test]
fn observer_coordinates_do_not_change_selected_scientific_model() {
    for (model, expected_provenance) in [
        (MoonlightModel::Jones2013Spectral, "Jones+2013"),
        (
            MoonlightModel::KrisciunasSchaefer1991,
            "Krisciunas & Schaefer 1991",
        ),
    ] {
        let config = NsbModelConfig::default()
            .with_site_profile(SiteProfileId::CtaSouth)
            .with_moonlight_model(model);

        for observer in [paranal(), arbitrary_observer()] {
            let evaluator = NsbEvaluator::with_config(config.clone()).unwrap();
            assert_eq!(evaluator.config().moonlight_model(), model);
            let component = evaluator
                .evaluate(
                    &PointQuery::new(observer, time(), target())
                        .with_components(ComponentMask::MOON),
                )
                .unwrap()
                .components
                .into_iter()
                .next()
                .unwrap();
            assert!(component.metadata.provenance.contains(expected_provenance));
        }
    }
}

#[test]
fn jones_uses_selected_site_profile_atmosphere() {
    let observer = paranal();
    let generic_profile = SiteProfileId::GenericClearSky.profile(observer);
    let north_profile = SiteProfileId::CtaNorth.profile(observer);
    let south_profile = SiteProfileId::CtaSouth.profile(observer);

    assert_ne!(
        generic_profile.atmosphere.surface_pressure,
        north_profile.atmosphere.surface_pressure
    );
    assert_ne!(
        generic_profile.atmosphere.surface_pressure,
        south_profile.atmosphere.surface_pressure
    );
    assert!(north_profile.atmosphere.surface_pressure > south_profile.atmosphere.surface_pressure);

    let evaluate = |site_profile| {
        evaluate_moonlight_at(
            NsbModelConfig::default()
                .with_moonlight_model(MoonlightModel::Jones2013Spectral)
                .with_site_profile(site_profile),
            observer,
            profile_time(),
            profile_target(),
        )
    };
    let generic = evaluate(SiteProfileId::GenericClearSky);
    let north = evaluate(SiteProfileId::CtaNorth);
    let south = evaluate(SiteProfileId::CtaSouth);

    for output in [&generic, &north, &south] {
        assert!(output.integrated.value().is_finite());
        assert!(output.integrated.value() > 0.0);
    }
    assert_ne!(
        generic.integrated.value().to_bits(),
        north.integrated.value().to_bits()
    );
    assert_ne!(
        generic.integrated.value().to_bits(),
        south.integrated.value().to_bits()
    );
    assert_ne!(
        north.integrated.value().to_bits(),
        south.integrated.value().to_bits()
    );
}

#[test]
fn ks_published_reference_is_independent_of_site_profile() {
    let observer = paranal();

    let evaluate = |site_profile| {
        evaluate_moonlight_at(
            NsbModelConfig::default()
                .with_moonlight_model(MoonlightModel::KrisciunasSchaefer1991)
                .with_site_profile(site_profile),
            observer,
            profile_time(),
            profile_target(),
        )
    };
    let generic = evaluate(SiteProfileId::GenericClearSky);
    let north = evaluate(SiteProfileId::CtaNorth);
    let south = evaluate(SiteProfileId::CtaSouth);

    for output in [&generic, &north, &south] {
        assert!(output.integrated.value().is_finite());
        assert!(output.integrated.value() > 0.0);
    }

    for candidate in [&north, &south] {
        assert_eq!(
            generic.integrated.value().to_bits(),
            candidate.integrated.value().to_bits()
        );
        assert_eq!(
            generic.b_flux_s10.value().to_bits(),
            candidate.b_flux_s10.value().to_bits()
        );
        assert_eq!(
            generic.v_flux_s10.value().to_bits(),
            candidate.v_flux_s10.value().to_bits()
        );
    }
}

#[test]
fn metadata_preserves_model_identity_and_records_site_assumptions_separately() {
    let jones = evaluate_moonlight_at(
        NsbModelConfig::default()
            .with_site_profile(SiteProfileId::CtaSouth)
            .with_moonlight_model(MoonlightModel::Jones2013Spectral),
        paranal(),
        profile_time(),
        profile_target(),
    );
    assert!(jones.metadata.provenance.contains("Jones+2013"));
    assert!(jones.metadata.provenance.contains("ctao-south-planning"));
    assert!(!jones
        .metadata
        .provenance
        .contains("Krisciunas & Schaefer 1991"));

    let ks = evaluate_moonlight_at(
        NsbModelConfig::default()
            .with_site_profile(SiteProfileId::CtaNorth)
            .with_moonlight_model(MoonlightModel::KrisciunasSchaefer1991),
        paranal(),
        profile_time(),
        profile_target(),
    );
    assert_eq!(
        ks.metadata.status,
        ComponentCalibrationStatus::PublishedReference
    );
    assert!(ks
        .metadata
        .provenance
        .contains("Krisciunas & Schaefer 1991"));
    assert!(ks.metadata.provenance.contains("ctao-north-planning"));
    assert!(ks
        .metadata
        .provenance
        .contains("fixed validated V-band extinction k=0.172"));
    assert!(!ks.metadata.provenance.contains("Jones+2013"));
}
