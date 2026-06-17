use anyhow::Result;
use chrono::{DateTime, Utc};
use nsb::{ComponentMask, Location, NsbEvaluator, Site, Target, ThresholdQuery, DEG};
use qtty::radiometry::PhotonsPerSquareCentimeterNanosecondSteradian as BandPhotonRadiance;
use tempoch::{Period, Time, UTC};

fn main() -> Result<()> {
    let start = parse_utc("2023-09-04T00:00:00Z")?;
    let end = parse_utc("2023-09-04T12:00:00Z")?;

    let query = ThresholdQuery {
        location: Location::NamedSite(Site::Paranal),
        target: Target::new(266.41683 * DEG, -29.00781 * DEG),
        window: Period::new(start, end),
        threshold: BandPhotonRadiance::new(0.21),
        components: ComponentMask::ZODIACAL | ComponentMask::AIRGLOW,
        sample_step: ThresholdQuery::DEFAULT_SAMPLE_STEP,
        sun_altitude_ceiling: Some(ThresholdQuery::DEFAULT_SUN_ALTITUDE_CEILING),
        target_altitude_floor: Some(ThresholdQuery::DEFAULT_TARGET_ALTITUDE_FLOOR),
    };

    let result = NsbEvaluator::new()?.periods_below_threshold(&query)?;

    println!(
        "threshold = {:.6e} ph/(cm² ns sr)",
        result.threshold.value()
    );
    if result.periods.is_empty() {
        println!("(no sub-periods in window are below threshold)");
        return Ok(());
    }

    println!("{} period(s) below threshold:", result.periods.len());
    for period in result.periods {
        println!(
            "  {} -> {}",
            format_utc(period.start),
            format_utc(period.end)
        );
    }

    Ok(())
}

fn parse_utc(input: &str) -> Result<Time<UTC>> {
    let time = DateTime::parse_from_rfc3339(input)?.with_timezone(&Utc);
    Ok(Time::<UTC>::from_chrono(time))
}

fn format_utc(time: Time<UTC>) -> String {
    match time.to_chrono() {
        Some(dt) => dt.format("%Y-%m-%dT%H:%M:%S%.3fZ").to_string(),
        None => "<unrepresentable UTC instant>".to_string(),
    }
}
