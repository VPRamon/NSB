// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Vallés Puig, Ramon
//! Receipt generator using public NSB/dependency operations; no copied sky model.
use chrono::{DateTime, Utc};
use nsb::transport::{AirmassModel, DirectPathGeometry, DirectTransmission, ExtinctionIngredients};
use nsb::units::radiometry::PhotonsPerSquareCentimeterNanosecondSteradian;
use nsb::units::unit;
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

fn check(
    rows: &mut Vec<Value>,
    family: &str,
    input: Value,
    actual: f64,
    reference: f64,
    atol: f64,
    units: &str,
) {
    let error = (actual - reference).abs();
    rows.push(json!({"family":family,"input":input,"actual":actual,"reference":reference,
        "absolute_error":error,"relative_error":if reference == 0.0 { None } else { Some(error/reference.abs()) },
        "absolute_tolerance":atol,"units":units,"result":if error <= atol && actual.is_finite() {"PASS"} else {"FAIL"}}));
}

fn numerical() -> Value {
    let mut rows = Vec::new();
    // This is exactly the public upstream kernel called by nsb::band::integrate_linear.
    // Analytic constant/linear spectra on irregular nodes and clipped endpoints.
    let x = [300.0, 351.0, 469.0, 650.0];
    for (slope, intercept) in [(0.0, 2.0), (0.002, 0.4)] {
        let y = x.map(|w| slope * w + intercept);
        for (a, b) in [(300.0, 650.0), (350.25, 570.75), (469.0, 650.0)] {
            let reference = (slope * (a + b) / 2.0 + intercept) * (b - a);
            let actual = siderust::optica::spectrum::integrate::trapz_range(&x, &y, a, b);
            check(
                &mut rows,
                "clipped_linear",
                json!({"slope":slope,"intercept":intercept,"lower_nm":a,"upper_nm":b}),
                actual,
                reference,
                1e-10,
                "photons cm^-2 ns^-1 sr^-1 (synthetic density per nm)",
            );
        }
    }
    // Quadratic composite trapezoid has exact error L*h^2/6. This tests
    // convergence of the kernel, not a nonexistent sky-model resolution knob.
    for n in [8, 16, 32, 64] {
        let x: Vec<_> = (0..=n)
            .map(|i| 300.0 + 350.0 * i as f64 / n as f64)
            .collect();
        let y: Vec<_> = x.iter().map(|w| ((w - 300.0) / 350.0).powi(2)).collect();
        let actual = siderust::optica::spectrum::integrate::trapz_range(&x, &y, 300.0, 650.0);
        let reference = 350.0 / 3.0;
        let error = 350.0 / (6.0 * (n as f64).powi(2));
        check(
            &mut rows,
            "quadratic_convergence",
            json!({"intervals":n,"analytic_integral":reference,"analytic_discretization_error":error,"computed_discretization_error":actual-reference}),
            actual,
            reference + error,
            1e-10,
            "photons cm^-2 ns^-1 sr^-1 (synthetic)",
        );
    }
    // 1 ph cm^-2 ns^-1 sr^-1 = 10^4 * 10^9 ph m^-2 s^-1 sr^-1.
    let si = PhotonsPerSquareCentimeterNanosecondSteradian::new(1.0)
        .to::<unit::PhotonPerSquareMeterSecondSteradian>()
        .value();
    check(
        &mut rows,
        "radiance_units",
        json!({"input":1.0,"input_units":"ph cm^-2 ns^-1 sr^-1"}),
        si,
        1e13,
        0.01,
        "ph m^-2 s^-1 sr^-1",
    );
    let atmosphere = nsb::site::AtmosphericConditions::paranal_average();
    let direct = DirectTransmission::new(
        ExtinctionIngredients::RAYLEIGH_MIE,
        AirmassModel::PlaneParallel,
    );
    for wavelength in [360.0, 500.0, 650.0] {
        let wavelength = Nanometers::new(wavelength);
        // Tau is deliberately treated as the input to the algebraic contract.
        // This is not an independent oracle for the empirical optical depths.
        let tau = direct
            .optical_depth(wavelength, atmosphere)
            .unwrap()
            .total
            .value();
        for zenith in [0.0_f64, 30.0, 60.0, 80.0] {
            let reference = (-tau / zenith.to_radians().cos()).exp();
            let actual = direct
                .transmission(
                    wavelength,
                    DirectPathGeometry::new(Degrees::new(zenith)).unwrap(),
                    atmosphere,
                )
                .unwrap()
                .value();
            check(
                &mut rows,
                "beer_lambert_composition",
                json!({"wavelength_nm":wavelength.value(),"zenith_deg":zenith,"vertical_tau":tau}),
                actual,
                reference,
                2e-14,
                "dimensionless transmission",
            );
        }
    }
    json!({"schema":"nsb-numerical-receipt-v1","checks":rows})
}

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

fn main() -> Result<()> {
    let args: Vec<_> = std::env::args().collect();
    if args.get(1).map(String::as_str) == Some("numerical") {
        let result = numerical();
        println!("{}", serde_json::to_string_pretty(&result)?);
        if result["checks"]
            .as_array()
            .unwrap()
            .iter()
            .any(|r| r["result"] != "PASS")
        {
            return Err("numerical contract failed".into());
        }
    } else {
        let document: Value = serde_json::from_slice(&fs::read(
            args.get(1)
                .ok_or("usage: publication_probe numerical | fixtures.json")?,
        )?)?;
        let results: Result<Vec<_>> = document["cases"]
            .as_array()
            .ok_or("missing cases")?
            .iter()
            .map(|case| point(&document, case))
            .collect();
        println!("{}", serde_json::to_string_pretty(&results?)?);
    }
    Ok(())
}
