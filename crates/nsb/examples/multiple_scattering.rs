// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Vallés Puig, Ramon
//! Smooth synthetic source, opt-in research reference only.
use nsb::transport::multiple::{multiple_scattering, MultipleScatteringSettings};
use nsb::transport::single::{AerosolPhase, IncidentSky, SkyCoverage};
use nsb::transport::RadianceOrigin;
use nsb::units::qtty::{Albedos, Degrees, Meters, Nanometers, Quantity};
use nsb::units::radiometry::PhotonsPerSquareCentimeterNanosecondSteradianNanometer as Radiance;
use siderust::atmosphere::{AtmosphericOptics, LayeredAtmosphere};
use siderust::coordinates::spherical::direction::Horizontal;
use siderust::optica::phase::HenyeyGreensteinPhaseFunction;

struct SmoothSky;
impl IncidentSky for SmoothSky {
    fn coverage(&self) -> SkyCoverage {
        SkyCoverage {
            identity: "synthetic-isotropic-multiple-example".into(),
            wavelengths: [Nanometers::new(360.0), Nanometers::new(570.0)],
            full_hemisphere: true,
            origin: RadianceOrigin::TopOfAtmosphere,
        }
    }
    fn radiance(&self, _w: Nanometers, _direction: Horizontal) -> nsb::Result<Radiance> {
        Ok(Radiance::new(0.025))
    }
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let optics = AtmosphericOptics::from_extinction(
        Quantity::new(0.000015),
        Quantity::new(0.000010),
        Albedos::new(0.7),
        Quantity::new(0.000005),
    )?;
    let atmosphere = LayeredAtmosphere::try_new(
        &[Meters::new(2100.0), Meters::new(3100.0)],
        [Nanometers::new(360.0), Nanometers::new(570.0)],
        vec![optics],
    )?;
    let phase = AerosolPhase::HenyeyGreenstein(HenyeyGreensteinPhaseFunction::try_new(0.0)?);
    let result = multiple_scattering(
        &atmosphere,
        &phase,
        &SmoothSky,
        Nanometers::new(450.0),
        Horizontal::new(Degrees::new(53.13010235415598), Degrees::new(23.0)),
        MultipleScatteringSettings {
            angular_tolerance: 0.05,
            vertical_tolerance: 0.05,
            ..MultipleScatteringSettings::default()
        },
    )?;
    println!(
        "direct={}; first={}; higher={}; total={}; convergence={:?}",
        result.direct, result.single, result.higher_orders, result.total, result.convergence
    );
    Ok(())
}
