use chrono::{DateTime, Utc};
use nsb::{
    AirglowModel, ComponentMask, MoonlightModel, NsbEvaluator, NsbModelConfig, PointQuery, Target,
    ZodiacalModel, DEG,
};
use siderust::catalogs::observatories;
use tempoch::{Time, UTC};

fn main() -> nsb::Result<()> {
    let components = ComponentMask::ZODIACAL | ComponentMask::AIRGLOW | ComponentMask::MOON;
    let config = NsbModelConfig::generic_clear_sky()
        .with_zodiacal_model(ZodiacalModel::Leinert1998)
        .with_airglow_model(AirglowModel::ParanalPalaceV1)
        .with_moonlight_model(MoonlightModel::Jones2013Spectral);
    let evaluator = NsbEvaluator::with_config(config)?;

    let time = DateTime::parse_from_rfc3339("2023-09-04T01:48:00Z")
        .expect("valid example timestamp")
        .with_timezone(&Utc);
    let query = PointQuery::new(
        observatories::EL_PARANAL.geodetic(),
        Time::<UTC>::from_chrono(time),
        Target::new(266.41683 * DEG, -29.00781 * DEG),
    )
    .with_components(components);

    let result = evaluator.evaluate(&query)?;
    for component in &result.components {
        println!(
            "{}: status={} integrated={:.6e} ph/(cm² ns sr)",
            component.name,
            component.metadata.status.as_str(),
            component.integrated.value()
        );
        println!("  provenance: {}", component.metadata.provenance);
        println!(
            "  validated domain: {}",
            component.metadata.validated_domain
        );
    }

    Ok(())
}
