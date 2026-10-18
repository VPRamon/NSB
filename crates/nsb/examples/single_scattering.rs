// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Vallés Puig, Ramon
//! Synthetic wavelength-resolved illumination; no empirical accuracy claim.
use nsb::transport::single::{
    single_scattering, AerosolPhase, IncidentSky, SkyCoverage, SolverSettings,
};
use nsb::transport::RadianceOrigin;
use nsb::units::qtty::{Albedos, Degrees, Meters, Nanometers, Quantity, Radian};
use nsb::units::radiometry::PhotonsPerSquareCentimeterNanosecondSteradianNanometer as Radiance;
use siderust::atmosphere::{AtmosphericOptics, LayeredAtmosphere};
use siderust::coordinates::spherical::direction::Horizontal;
use siderust::optica::phase::HenyeyGreensteinPhaseFunction;

struct SyntheticSky;
impl IncidentSky for SyntheticSky {
    fn coverage(&self) -> SkyCoverage {
        SkyCoverage {
            identity: "synthetic-linear-spectrum-dipole-v1; local Horizontal; epoch independent"
                .into(),
            wavelengths: [Nanometers::new(360.0), Nanometers::new(570.0)],
            full_hemisphere: true,
            origin: RadianceOrigin::TopOfAtmosphere,
        }
    }
    fn radiance(&self, wavelength: Nanometers, direction: Horizontal) -> nsb::Result<Radiance> {
        Ok(Radiance::new(
            0.01 * wavelength.value() / 450.0 * (1.0 + 0.4 * direction.alt().to::<Radian>().sin()),
        ))
    }
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let optics = AtmosphericOptics::from_extinction(
        Quantity::new(0.00007),
        Quantity::new(0.00012),
        Albedos::new(0.7),
        Quantity::new(0.00003),
    )?;
    let atmosphere = LayeredAtmosphere::try_new(
        &[Meters::new(2100.0), Meters::new(3100.0)],
        [Nanometers::new(360.0), Nanometers::new(570.0)],
        vec![optics],
    )?;
    let phase = AerosolPhase::HenyeyGreenstein(HenyeyGreensteinPhaseFunction::try_new(0.65)?);
    let result = single_scattering(
        &atmosphere,
        &phase,
        &SyntheticSky,
        Nanometers::new(450.0),
        Horizontal::new(Degrees::new(53.13010235415598), Degrees::new(23.0)),
        SolverSettings::default(),
    )?;
    println!(
        "direct={}; single={}; higher orders={:?}; convergence={:?}",
        result.direct, result.single, result.higher_orders, result.convergence
    );
    Ok(())
}
