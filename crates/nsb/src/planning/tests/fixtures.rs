// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Vallés Puig, Ramon
//! Shared physical fixtures and cached-scalar certificate regression.
use super::bounds::{prepared_bounds, BoundOutcome};
use super::discovery::{search, Limits};
use super::{prepare, threshold};
use crate::components::starlight::{StarlightMap, StarlightProvenance};
use crate::site::{AtmosphericConditions, SiteProfile, SiteProfileTag};
use crate::units::radiometry::PhotonsPerSquareCentimeterNanosecondSteradian as Radiance;
use crate::{
    ComponentMask, NsbEvaluator, NsbModelConfig, StarlightProduct, Target, ThresholdQuery, DEG,
};
use chrono::{DateTime, Utc};
use siderust::qtty::{Hectopascals, Kilometers};
use tempoch::{Period, Time, UTC};

fn parse(s: &str) -> Time<UTC> {
    Time::<UTC>::from_chrono(DateTime::parse_from_rfc3339(s).unwrap().with_timezone(&Utc))
}

#[test]
fn real_starlight_certificate_reuses_the_authoritative_cached_scalar() {
    let evaluator = evaluator(false, true);
    let start = parse("2026-12-30T00:00:00Z");
    let end = parse("2027-01-02T00:00:00Z");
    let query = ThresholdQuery::new(
        siderust::catalogs::observatories::EL_PARANAL.geodetic(),
        Target::new(83.6331 * DEG, 22.0145 * DEG),
        Period::new(start, end),
        Radiance::new(100.0),
    )
    .with_components(ComponentMask::STARLIGHT)
    .with_sun_altitude_ceiling(None)
    .with_target_altitude_floor(None);
    let context = evaluator.prepare_site_window_context(&query).unwrap();
    let prepared = prepare::prepare_target_threshold(&evaluator, &context, &query).unwrap();
    let exact = |t| threshold::evaluate_integrated(&evaluator, &prepared, t);
    let BoundOutcome::Enclosure {
        range: enclosure, ..
    } = prepared_bounds(&prepared, prepared.tt_window).unwrap()
    else {
        panic!("missing Starlight certificate")
    };
    assert_eq!(enclosure.lower, enclosure.upper);
    for i in 0..=100 {
        let t = siderust::time::ModifiedJulianDate::new(
            prepared.tt_window.start.raw().value()
                + i as f64 / 100.0 * prepared.tt_window.length().value(),
        );
        assert_eq!(
            exact(t).unwrap().value().to_bits(),
            enclosure.lower.value().to_bits()
        );
    }
    let (periods, costs) = search(
        prepared.tt_window,
        enclosure.upper,
        exact,
        prepared_bounds(&prepared, prepared.tt_window).unwrap(),
        Limits::default(),
    )
    .unwrap();
    assert_eq!(periods, vec![prepared.tt_window]);
    assert_eq!(costs.exact, 0);
    let mut strict_query = query.clone();
    strict_query.threshold = enclosure.upper;
    assert_eq!(
        evaluator
            .periods_below_threshold_certified(&strict_query)
            .unwrap()
            .periods,
        vec![query.window]
    );
    assert!(enclosure.lower.value() > 0.0);
    strict_query.threshold = Radiance::new(0.0);
    assert!(
        evaluator
            .periods_below_threshold_certified(&strict_query)
            .unwrap()
            .periods
            .is_empty(),
        "a proven rejection is a valid empty schedule, unlike incomplete coverage"
    );
    // A constant partial contribution cannot certify a selected dynamic sum.
    let mut dynamic = prepared;
    dynamic.components |= ComponentMask::ZODIACAL;
    assert!(matches!(
        prepared_bounds(&dynamic, dynamic.tt_window).unwrap(),
        BoundOutcome::Unsupported
    ));
}

fn evaluator(north: bool, starlight: bool) -> NsbEvaluator {
    evaluator_with_moon(north, starlight, crate::MoonlightModel::Jones2013Spectral)
}

pub(super) fn evaluator_with_moon(
    north: bool,
    starlight: bool,
    moon: crate::MoonlightModel,
) -> NsbEvaluator {
    struct North;
    impl SiteProfileTag for North {
        const NAME: &'static str = "ctao-north-planning";
    }
    struct South;
    impl SiteProfileTag for South {
        const NAME: &'static str = "ctao-south-planning";
    }
    let config = if north {
        NsbModelConfig::generic_clear_sky().with_site_profile(
            SiteProfile::<North>::planning(
                Kilometers::new(2.2),
                AtmosphericConditions::clear_sky_with_pressure(Hectopascals::new(770.0)).unwrap(),
                "CTAO-N benchmark planning preset",
            )
            .unwrap(),
        )
    } else {
        NsbModelConfig::generic_clear_sky().with_site_profile(
            SiteProfile::<South>::planning(
                Kilometers::new(2.1),
                AtmosphericConditions::paranal_average(),
                "CTAO-S benchmark planning preset",
            )
            .unwrap(),
        )
    };
    let config = if starlight {
        let p = StarlightProvenance::new(
            "synthetic benchmark fixture",
            "fixture",
            "2026-06-21",
            "synthetic",
            "test-only",
            "test-only",
            "integrated 300-650 nm photon radiance",
            "HEALPix nside=1 ring 12 pixels",
            None::<String>,
        );
        let map = StarlightMap::from_csv_str(
            include_str!("../../../tests/data/starlight_fixture_map.csv"),
            p,
        )
        .unwrap();
        config.with_starlight_product(StarlightProduct::with_experimental_map(map))
    } else {
        config
    };
    NsbEvaluator::with_config(config.with_moonlight_model(moon)).unwrap()
}
