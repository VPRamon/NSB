//! Cross-check: optimized threshold search agrees with the legacy
//! uniform-scan path within tolerance.

use chrono::{DateTime, Utc};
use nsb::{ComponentMask, Location, NsbEvaluator, Site, Target, ThresholdQuery, DEG};
use qtty::radiometry::PhotonsPerSquareCentimeterNanosecondSteradian as BandPhotonRadiance;
use qtty::Second;
use tempoch::{Period, Time, UTC};

fn parse(s: &str) -> Time<UTC> {
    let dt = DateTime::parse_from_rfc3339(s).unwrap().with_timezone(&Utc);
    Time::<UTC>::from_chrono(dt)
}

#[test]
fn optimized_matches_legacy_within_tolerance() {
    let evaluator = NsbEvaluator::python_parity().expect("evaluator");
    let start = parse("2023-09-04T00:00:00Z");
    let end = parse("2023-09-05T00:00:00Z");

    // Threshold chosen so the function actually crosses inside the
    // window. ~0.21 ph/(cm² ns sr) is a typical bright-end cutoff.
    let target = Target::new(266.41683 * DEG, -29.00781 * DEG);
    let components = ComponentMask::ZODIACAL | ComponentMask::STARLIGHT | ComponentMask::AIRGLOW;

    let legacy_query = ThresholdQuery {
        location: Location::NamedSite(Site::Paranal),
        target,
        window: Period::new(start, end),
        threshold: BandPhotonRadiance::new(0.21),
        components,
        sample_step: Second::new(60.0),
        sun_altitude_ceiling: None,
        target_altitude_floor: None,
    };
    let legacy = evaluator
        .periods_below_threshold_legacy(&legacy_query)
        .expect("legacy");

    let opt_query = ThresholdQuery {
        sample_step: ThresholdQuery::DEFAULT_SAMPLE_STEP,
        sun_altitude_ceiling: Some(ThresholdQuery::DEFAULT_SUN_ALTITUDE_CEILING),
        target_altitude_floor: Some(ThresholdQuery::DEFAULT_TARGET_ALTITUDE_FLOOR),
        ..legacy_query.clone()
    };
    let opt = evaluator
        .periods_below_threshold(&opt_query)
        .expect("optimized");

    // The optimized result is a *subset* of the legacy result (legacy
    // also reports daytime / below-horizon periods that the prefilter
    // discards). Each optimized period must be contained in some legacy
    // period within a 60 s tolerance on each endpoint.
    let tol_secs = 60.0;
    for opt_p in &opt.periods {
        let opt_start = opt_p.start.to_chrono().unwrap();
        let opt_end = opt_p.end.to_chrono().unwrap();
        let mut covered = false;
        for legacy_p in &legacy.periods {
            let l_start = legacy_p.start.to_chrono().unwrap();
            let l_end = legacy_p.end.to_chrono().unwrap();
            let s_diff = (l_start - opt_start).num_milliseconds().abs() as f64 / 1000.0;
            let e_diff = (l_end - opt_end).num_milliseconds().abs() as f64 / 1000.0;
            let contained = (l_start - opt_start).num_seconds() <= tol_secs as i64
                && (opt_end - l_end).num_seconds() <= tol_secs as i64;
            if (s_diff <= tol_secs && e_diff <= tol_secs) || contained {
                covered = true;
                break;
            }
        }
        assert!(
            covered,
            "optimized period {opt_start:?}..{opt_end:?} not represented in legacy result"
        );
    }
    assert!(
        !opt.periods.is_empty(),
        "expected at least one darker-than-threshold period during the night"
    );
}
