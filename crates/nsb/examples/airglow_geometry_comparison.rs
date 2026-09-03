//! Deterministic cross-model Airglow geometry comparison (#110).
//!
//! Run with:
//!
//! ```text
//! cargo run -p nsb --example airglow_geometry_comparison
//! ```
//!
//! The profiles are synthetic mathematical fixtures, not recommended physical
//! models. Output is CSV so reviewers can inspect or plot the large-zenith
//! behavior without committing generated plots.

use nsb::{
    AirglowGeometryModel, AirglowWavelengthApplicability, ValidatedZenithDomain,
    VerticalEmissionProfile, VerticalEmissionProfileDefinition, VerticalProfileNormalization,
    VERTICAL_EMISSION_PROFILE_SCHEMA_VERSION,
};
use siderust::coordinates::centers::Geodetic;
use siderust::coordinates::frames::ECEF;
use siderust::qtty::{Degrees, Kilometers, Meters, Nanometers};

fn profile(id: &str, altitude_km: &[f64], relative_emissivity: &[f64]) -> VerticalEmissionProfile {
    VerticalEmissionProfile::new(VerticalEmissionProfileDefinition {
        schema_version: VERTICAL_EMISSION_PROFILE_SCHEMA_VERSION,
        profile_id: id.into(),
        altitude_km: altitude_km.iter().copied().map(Kilometers::new).collect(),
        relative_emissivity: relative_emissivity.to_vec(),
        normalization: VerticalProfileNormalization::UnitVerticalIntegral,
        wavelength: AirglowWavelengthApplicability {
            min: Nanometers::new(300.0),
            max: Nanometers::new(650.0),
            band: "synthetic-300-650-nm".into(),
        },
        assumptions: "synthetic comparison shape; not observational ground truth".into(),
        provenance: "NSB issue #110 deterministic comparison example".into(),
        license: "CC0-1.0 synthetic fixture".into(),
        validated_zenith: ValidatedZenithDomain {
            min: Degrees::new(0.0),
            max: Degrees::new(90.0),
        },
    })
    .expect("static profile is valid")
}

fn observer(height_m: f64) -> Geodetic<ECEF> {
    Geodetic::new_raw(
        Degrees::new(23.5),
        Degrees::new(-17.25),
        Meters::new(height_m),
    )
}

fn main() -> nsb::Result<()> {
    let profiles = [
        profile("thin-90km", &[89.99, 90.0, 90.01], &[0.0, 1.0, 0.0]),
        profile(
            "broad-80-110km",
            &[75.0, 82.0, 92.0, 110.0],
            &[0.0, 0.6, 1.0, 0.0],
        ),
        profile(
            "synthetic-two-layer",
            &[75.0, 82.0, 88.0, 98.0, 108.0, 120.0],
            &[0.0, 0.8, 0.1, 0.2, 0.7, 0.0],
        ),
    ];
    let van_rhijn = AirglowGeometryModel::default();
    println!(
        "observer_altitude_m,profile_id,zenith_deg,van_rhijn_factor,vertical_profile_factor,relative_difference"
    );
    for height_m in [0.0, 2_500.0, 5_000.0] {
        let location = observer(height_m);
        for profile in &profiles {
            let vertical = AirglowGeometryModel::VerticalProfile(profile.clone());
            for zenith_deg in [0.0, 30.0, 60.0, 75.0, 85.0, 89.0, 90.0] {
                let zenith = Degrees::new(zenith_deg);
                let baseline = van_rhijn.geometry_factor(location, zenith)?.value();
                let alternative = vertical.geometry_factor(location, zenith)?.value();
                let relative = (alternative - baseline) / baseline;
                println!(
                    "{height_m:.0},{},{zenith_deg:.0},{baseline:.12},{alternative:.12},{relative:.12}",
                    profile.profile_id()
                );
            }
        }
    }
    Ok(())
}
