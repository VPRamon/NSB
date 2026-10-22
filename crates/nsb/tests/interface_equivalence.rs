// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Vallés Puig, Ramon
use chrono::{DateTime, Utc};
use nsb::{
    AirglowModel, ComponentMask, NsbEvaluator, NsbModelConfig, Observer, PointQuery,
    SolarFluxUnits, DEG,
};
use serde_json::{json, Value};
use siderust::coordinates::{spherical::direction, transform::TransformFrame};
use siderust::qtty::{Degrees, Meters, Nanometers};
use std::{error::Error, fs};
use tempoch::{Time, UTC};

type Result<T> = std::result::Result<T, Box<dyn Error>>;

fn point(document: &Value, case: &Value) -> Result<Value> {
    let number = |v: &Value, key: &str| -> Result<f64> {
        Ok(v[key].as_f64().ok_or("missing fixture number")?)
    };
    let observer = &document["observer"];
    let observer = Observer::new_raw(
        Degrees::new(number(observer, "lon")?),
        Degrees::new(number(observer, "lat")?),
        Meters::new(number(observer, "height")?),
    );
    let target = &document["target"];
    let target: nsb::Target =
        direction::ICRS::new(number(target, "ra")? * DEG, number(target, "dec")? * DEG).to_frame();
    let time = Time::<UTC>::from_chrono(
        DateTime::parse_from_rfc3339(document["time"].as_str().ok_or("missing time")?)?
            .with_timezone(&Utc),
    );
    let mut mask = ComponentMask::empty();
    for name in case["components"].as_array().ok_or("missing components")? {
        mask |= match name.as_str() {
            Some("zodiacal") => ComponentMask::ZODIACAL,
            Some("airglow") => ComponentMask::AIRGLOW,
            Some("moon") => ComponentMask::MOON,
            _ => return Err("unsupported fixture component".into()),
        };
    }
    let config = NsbModelConfig::generic_clear_sky()
        .with_airglow_model(AirglowModel::ParanalPalaceV1)
        .with_solar_radio_flux(SolarFluxUnits::new(number(case, "sfu")?));
    let evaluator = NsbEvaluator::with_config(config)?;
    let query = PointQuery::new(observer, time, target).with_components(mask);
    let result = evaluator.evaluate(&query)?;
    let components: serde_json::Map<_, _> = result
        .components
        .iter()
        .map(|c| (c.name.to_string(), json!(c.integrated.value())))
        .collect();
    let mut bands = Vec::new();
    for (lower, upper) in [
        (300.0, 650.0),
        (300.0, 450.25),
        (450.25, 650.0),
        (360.0, 570.0),
    ] {
        bands.push(
            evaluator
                .integrate_band(&query, Nanometers::new(lower), Nanometers::new(upper))?
                .value(),
        );
    }
    Ok(
        json!({"id":case["id"],"total":result.integrated.value(),"components":components,"bands":bands}),
    )
}

#[test]
fn shared_interface_reference() -> Result<()> {
    let document: Value = serde_json::from_str(include_str!("data/interfaces/inputs.json"))?;
    let expected: Vec<Value> = serde_json::from_str(include_str!("data/interfaces/expected.json"))?;
    let results: Vec<Value> = document["cases"]
        .as_array()
        .unwrap()
        .iter()
        .map(|case| point(&document, case))
        .collect::<Result<_>>()?;
    assert_eq!(results.len(), expected.len());
    for ((actual, reference), case) in results
        .iter()
        .zip(&expected)
        .zip(document["cases"].as_array().unwrap())
    {
        assert_eq!(actual, &point(&document, case)?, "Rust nondeterminism");
        assert_eq!(actual["id"], reference["id"]);
        for (a, b) in [(
            actual["total"].as_f64().unwrap(),
            reference["total"].as_f64().unwrap(),
        )]
        .into_iter()
        .chain(
            actual["bands"]
                .as_array()
                .unwrap()
                .iter()
                .zip(reference["bands"].as_array().unwrap())
                .map(|(a, b)| (a.as_f64().unwrap(), b.as_f64().unwrap())),
        )
        .chain(
            actual["components"]
                .as_object()
                .unwrap()
                .iter()
                .map(|(name, a)| {
                    (
                        a.as_f64().unwrap(),
                        reference["components"][name].as_f64().unwrap(),
                    )
                }),
        ) {
            assert!(
                a.is_finite()
                    && b.is_finite()
                    && (a - b).abs() <= 1e-14 + 2e-12 * a.abs().max(b.abs()),
                "{}: {a} vs {b}",
                actual["id"]
            );
        }
    }
    let mut invalid = document.clone();
    invalid["cases"][0]["sfu"] = json!(0.0);
    assert!(
        point(&invalid, &invalid["cases"][0]).is_err(),
        "Rust accepted zero solar flux"
    );
    // Optional ephemeral output for the installed-wheel pytest contract.
    if let Some(path) = std::env::var_os("NSB_INTERFACE_OUTPUT") {
        fs::write(path, serde_json::to_vec(&results)?)?;
    }
    Ok(())
}
