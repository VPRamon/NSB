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

fn time() -> Time<UTC> {
    Time::<UTC>::from_chrono(
        DateTime::parse_from_rfc3339("2023-09-04T01:48:00Z")
            .unwrap()
            .with_timezone(&Utc),
    )
}

fn target() -> Target {
    Target::new(266.41683 * DEG, -29.00781 * DEG)
}

fn paranal() -> Geodetic<ECEF> {
    observatories::EL_PARANAL.geodetic()
}

fn arbitrary_observer() -> Geodetic<ECEF> {
    Geodetic::new_raw(
        Degrees::new(18.4),
        Degrees::new(-33.9),
        Meters::new(120.0),
    )
}

fn evaluate_moonlight(config: NsbModelConfig, observer: Geodetic<ECEF>) -> NsbComponent {
    NsbEvaluator::with_config(config)
        .unwrap()
        .evaluate(
            &PointQuery::new(observer, time(), target()).with_components(ComponentMask::MOON),
        )
        .unwrap()
        .components
        .into_iter()
        .next()
        .unwrap()
}

#[test]
fn moonlight_model_identity_and_default_are_stable() {
    let default = NsbModelConfig::default();

    assert_eq!(
        default.moonlight_model(),
        MoonlightModel::Jones2013Spectral
    );
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
        NsbModelConfig::default()
            .with_moonlight_model(MoonlightModel::KrisciunasSchaefer1991),
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
fn site_profile_and_observer_do_not_change_selected_scientific_model() {
    let jones = NsbModelConfig::default()
        .with_site_profile(SiteProfileId::CtaSouth)
        .with_moonlight_model(MoonlightModel::Jones2013Spectral);
    let ks = NsbModelConfig::default()
        .with_site_profile(SiteProfileId::CtaNorth)
        .with_moonlight_model(MoonlightModel::KrisciunasSchaefer1991);

    assert_eq!(jones.moonlight_model(), MoonlightModel::Jones2013Spectral);
    assert_eq!(
        ks.moonlight_model(),
        MoonlightModel::KrisciunasSchaefer1991
    );

    let jones_paranal = evaluate_moonlight(jones.clone(), paranal());
    let jones_arbitrary = evaluate_moonlight(jones, arbitrary_observer());
    assert!(jones_paranal.metadata.provenance.contains("Jones+2013"));
    assert!(jones_arbitrary.metadata.provenance.contains("Jones+2013"));
}
