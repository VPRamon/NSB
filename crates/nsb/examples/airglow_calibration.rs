// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Vallés Puig, Ramon
//! Generate a fully runnable synthetic fit request and portable artifact.
use chrono::{DateTime, Utc};
use nsb::site::airglow_profile::{AirglowCalibrationProfile, AirglowCalibrationRequest};
use nsb::site::{AtmosphericConditions, SiteProfile, SiteProfileTag};
use nsb::{ComponentMask, NsbEvaluator, NsbModelConfig, PointQuery, SolarFluxUnits, Target};
use siderust::checksum::{sha256, to_hex};
use siderust::coordinates::centers::Geodetic;
use siderust::qtty::{Degrees, Hectopascals, Kilometers, Meters, Nanometers};
use tempoch::{Time, UTC};

struct Synthetic;
impl SiteProfileTag for Synthetic {
    const NAME: &'static str = "synthetic-planning";
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let output = std::path::PathBuf::from(
        std::env::args()
            .nth(1)
            .unwrap_or_else(|| "target/airglow-example".into()),
    );
    std::fs::create_dir_all(&output)?;
    let mut request: AirglowCalibrationRequest =
        toml::from_str(include_str!("airglow-calibration/request.toml"))?;
    let location = Geodetic::new_raw(
        Degrees::new(-70.4044),
        Degrees::new(-24.6275),
        Meters::new(2635.),
    );
    let mut atmosphere = AtmosphericConditions::clear_sky_with_pressure(Hectopascals::new(743.))?;
    atmosphere.rayleigh_scale_height = Kilometers::new(8.);
    atmosphere.mie_params.tau0 = siderust::qtty::OpticalDepths::new(0.03);
    atmosphere.mie_params.alpha = 1.;
    for observation in &mut request.observations {
        let profile = SiteProfile::<Synthetic>::planning(
            Kilometers::new(2.635),
            atmosphere,
            "Synthetic fixed atmosphere",
        )?;
        let evaluator = NsbEvaluator::with_config(
            NsbModelConfig::generic_clear_sky()
                .with_site_profile(profile)
                .with_solar_radio_flux(SolarFluxUnits::new(observation.solar_flux_sfu)),
        )?;
        let time = Time::<UTC>::from_chrono(
            DateTime::parse_from_rfc3339(&observation.utc)?.with_timezone(&Utc),
        );
        let target = Target::new(
            Degrees::new(observation.ra_deg),
            Degrees::new(observation.dec_deg),
        );
        observation.continuum_radiance = evaluator
            .integrate_band(
                &PointQuery::new(location, time, target).with_components(ComponentMask::AIRGLOW),
                Nanometers::new(300.),
                Nanometers::new(650.),
            )?
            .value()
            * 1.2;
    }
    let measurements = serde_json::to_string_pretty(&request.observations)?;
    let mut document = toml::Value::try_from(&request)?;
    document["definition"]["evidence"]["references"][0]["sha256"] =
        toml::Value::String(to_hex(&sha256(measurements.as_bytes())));
    request = document.try_into()?;
    std::fs::write(output.join("measurements.txt"), measurements)?;
    std::fs::write(output.join("request.toml"), toml::to_string(&request)?)?;
    let profile = AirglowCalibrationProfile::fit(request)?;
    let artifact = profile.to_toml_string()?;
    assert_eq!(
        profile,
        AirglowCalibrationProfile::from_toml_str(&artifact)?
    );
    std::fs::write(output.join("calibration.toml"), artifact)?;
    println!(
        "SYNTHETIC ONLY scale={} variance={} sha256={}",
        profile.scale().value(),
        profile.variance(),
        profile.checksum_sha256()
    );
    Ok(())
}
