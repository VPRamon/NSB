// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Vallés Puig, Ramon
//! Tests of production certificates, without a second search implementation.
use super::{
    bounds::{prepared_bounds, BoundOutcome, RadianceBounds},
    discovery, prepare,
};
use crate::units::radiometry::PhotonsPerSquareCentimeterNanosecondSteradian as Radiance;
use crate::{ComponentMask, MoonlightModel, NsbError, Target, ThresholdQuery, DEG};
use siderust::{
    qtty::Seconds,
    time::{Interval, ModifiedJulianDate as Mjd},
};
use tempoch::{Period, Time, UTC};

#[test]
fn strict_bounds_never_substitute_point_evidence_for_an_enclosure() {
    let w = Interval::new(Mjd::new(60000.0), Mjd::new(60000.01));
    for bound in [
        BoundOutcome::Unsupported,
        BoundOutcome::NeedsSubdivision,
        BoundOutcome::enclosure(
            RadianceBounds::new(Radiance::new(0.0), Radiance::new(1.0)).unwrap(),
        ),
    ] {
        let result = discovery::search_windows(
            &[w],
            Radiance::new(0.5),
            None,
            |_| panic!("strict classification must not sample"),
            |_| Ok(bound),
            true,
        );
        assert!(matches!(result, Err(NsbError::Unsupported(_))));
    }
    for value in [0.0, 0.5, 1.0] {
        let (periods, costs) = discovery::search_windows(
            &[w],
            Radiance::new(0.5),
            None,
            |_| panic!("constant certificate needs no probes"),
            |_| {
                Ok(BoundOutcome::enclosure(RadianceBounds::constant(
                    Radiance::new(value),
                )?))
            },
            true,
        )
        .unwrap();
        assert_eq!(periods, if value <= 0.5 { vec![w] } else { vec![] });
        assert_eq!(costs.exact, 0);
    }
    assert!(discovery::search_windows(
        &[w],
        Radiance::new(0.5),
        None,
        |_| panic!("provider error must propagate"),
        |_| Err(NsbError::OutOfRange("provider failure".into())),
        true
    )
    .unwrap_err()
    .to_string()
    .contains("provider failure"));
    for (lo, hi) in [(f64::NAN, 1.0), (0.0, f64::INFINITY), (1.0, 0.0)] {
        assert!(RadianceBounds::new(Radiance::new(lo), Radiance::new(hi)).is_err());
    }
}

#[test]
fn strict_real_masks_both_models_and_inclusive_moon_boundary() {
    let start = Time::<UTC>::from_chrono(
        "2026-01-14T00:00:00Z"
            .parse::<chrono::DateTime<chrono::Utc>>()
            .unwrap(),
    );
    let base = ThresholdQuery::new(
        siderust::catalogs::observatories::EL_PARANAL.geodetic(),
        Target::new(83.6331 * DEG, 22.0145 * DEG),
        Period::new(start, start + Seconds::new(3.0 * 86400.0)),
        Radiance::new(0.25),
    )
    .with_sun_altitude_ceiling(None)
    .with_target_altitude_floor(None);
    for moon in [
        MoonlightModel::Jones2013Spectral,
        MoonlightModel::KrisciunasSchaefer1991,
    ] {
        let evaluator = super::test_fixtures::evaluator_with_moon(false, true, moon);
        for bits in 0..16 {
            let q = base
                .clone()
                .with_components(ComponentMask::from_bits(bits).unwrap());
            let result = evaluator.periods_below_threshold_certified(&q);
            if bits == 0 || bits == ComponentMask::STARLIGHT.bits() {
                assert!(result.is_ok());
            } else {
                assert!(matches!(result, Err(NsbError::Unsupported(_))));
            }
            assert_eq!(
                evaluator.periods_below_threshold(&q).unwrap().periods,
                evaluator
                    .periods_below_threshold_adaptive(&q)
                    .unwrap()
                    .periods
            );
        }
        let q = base.clone().with_components(ComponentMask::MOON);
        let context = evaluator.prepare_site_window_context(&q).unwrap();
        let p = prepare::prepare_target_threshold(&evaluator, &context, &q).unwrap();
        let visible = p.moon_visible_periods.as_ref().unwrap();
        let rise = visible[0].start;
        assert!(matches!(
            prepared_bounds(&p, Interval::new(rise, rise)).unwrap(),
            BoundOutcome::Unsupported
        ));
        let gap = visible
            .windows(2)
            .find(|pair| pair[0].end < pair[1].start)
            .unwrap();
        // Both visibility edges are inclusive; only the strict interior is zero.
        assert!(matches!(
            prepared_bounds(&p, Interval::new(gap[0].end, gap[1].start)).unwrap(),
            BoundOutcome::NeedsSubdivision
        ));
        let inside = Interval::new(
            gap[0].end + Seconds::new(1.0).to::<siderust::qtty::Day>(),
            gap[1].start - Seconds::new(1.0).to::<siderust::qtty::Day>(),
        );
        let BoundOutcome::Enclosure { range } = prepared_bounds(&p, inside).unwrap() else {
            panic!("missing Moon-down enclosure")
        };
        assert_eq!(range.lower, Radiance::new(0.0));
        assert_eq!(range.upper, range.lower);
    }
}
