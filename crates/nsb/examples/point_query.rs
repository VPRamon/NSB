use chrono::{DateTime, Utc};
use nsb::{ComponentMask, NsbEvaluator, PointQuery, Target, DEG};
use siderust::catalogs::observatories;
use tempoch::{Time, UTC};

fn main() -> nsb::Result<()> {
    let time = DateTime::parse_from_rfc3339("2023-09-04T01:48:00Z")
        .expect("valid example timestamp")
        .with_timezone(&Utc);

    let query = PointQuery::new(
        observatories::EL_PARANAL.geodetic(),
        Time::<UTC>::from_chrono(time),
        Target::new(266.41683 * DEG, -29.00781 * DEG),
    )
    .with_components(ComponentMask::ZODIACAL | ComponentMask::AIRGLOW);

    let result = NsbEvaluator::new()?.evaluate(&query)?;

    for component in &result.components {
        println!(
            "{:>10}: integrated = {:.6e}  ph/(cm² ns sr)  | B = {:.3} S10 | V = {:.3} S10",
            component.name,
            component.integrated.value(),
            component.b_flux_s10.value(),
            component.v_flux_s10.value()
        );
    }
    println!("--------------------");
    println!(
        "    total: {:.6e} ph/(cm² ns sr)",
        result.integrated.value()
    );
    println!("       B = {:.3} mag/arcsec²", result.b_mag.value());
    println!("       V = {:.3} mag/arcsec²", result.v_mag.value());

    Ok(())
}
