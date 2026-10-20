// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Vallés Puig, Ramon
//! Complete native-bin multiple scattering; optional checksum-frozen Gaia input.
use nsb::components::starlight::StarlightMap;
use nsb::transport::multiple::starlight::starlight_multiple_scattering;
use nsb::transport::multiple::MultipleScatteringSettings;
use nsb::transport::single::starlight::{
    HealpixSettings, StarlightGeometry, StarlightSky, WavelengthSettings, WithinBinSpectrum,
};
use nsb::transport::single::AerosolPhase;
use nsb::units::qtty::{
    Albedos, Degrees, Hectopascals, Meters, Nanometers, OpticalDepths, Quantity,
};
use nsb::Observer;
use siderust::atmosphere::{AtmosphereProfile, AtmosphericOptics, LayeredAtmosphere, MieParams};
use siderust::coordinates::spherical::direction::Horizontal;
use siderust::optica::phase::HenyeyGreensteinPhaseFunction;
use tempoch::JulianDate;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    let frozen = !args.is_empty();
    let (map, tt, ut1) = if !frozen {
        (
            StarlightMap::from_json_bytes(
                include_bytes!("../tests/data/starlight-transport/uniform.json"),
                "35058caa944e51096e5d9ff8ca33eb19be08ef0b6c727ec37192758a64bdc24f",
            )?,
            2459680.8,
            2459680.7992,
        )
    } else if args.len() == 3 {
        (
            StarlightMap::from_json_file(
                std::path::Path::new(&args[0]),
                "fe331343cb9797a1d871ae798c88d2ae8865ae39358e8d82b464a91b3cf87d5f",
            )?,
            args[1].parse()?,
            args[2].parse()?,
        )
    } else {
        return Err("usage: starlight_multiple_scattering [FROZEN_MAP JD_TT JD_UT1]".into());
    };
    let geometry = StarlightGeometry {
        observer: Observer::new(
            Degrees::new(-111.59989),
            Degrees::new(31.96403),
            Meters::new(2097.),
        ),
        tt: JulianDate::new(tt),
        ut1: JulianDate::new(ut1),
    };
    let medium = if frozen {
        let profile = AtmosphereProfile {
            surface_pressure: Hectopascals::new(779.5),
            observer_altitude: Meters::new(2097.).to(),
            mie_params: MieParams {
                tau0: OpticalDepths::new(0.03),
                lambda_ref: Nanometers::new(550.),
                alpha: 1.38,
            },
            ..AtmosphereProfile::EL_PARANAL
        };
        LayeredAtmosphere::from_profile(
            profile,
            &[Meters::new(2097.), Meters::new(12097.)],
            [Nanometers::new(360.), Nanometers::new(570.)],
            &[1.],
            &[1.],
            Albedos::new(1.),
        )?
    } else {
        let optics = AtmosphericOptics::from_extinction(
            Quantity::new(0.08 / 1000.),
            Quantity::new(0.),
            Albedos::new(0.),
            Quantity::new(0.08 / 1000.),
        )?;
        LayeredAtmosphere::try_new(
            &[Meters::new(2097.), Meters::new(3097.)],
            [Nanometers::new(360.), Nanometers::new(570.)],
            vec![optics],
        )?
    };
    let phase =
        AerosolPhase::HenyeyGreenstein(HenyeyGreensteinPhaseFunction::try_new(if frozen {
            0.65
        } else {
            0.
        })?);
    let view = if frozen {
        Horizontal::new(
            Degrees::new(76.92569110795976),
            Degrees::new(271.8975544363482),
        )
    } else {
        Horizontal::new(Degrees::new(53.13010235415598), Degrees::new(23.))
    };
    let sky = StarlightSky::new(
        &map,
        geometry,
        [Nanometers::new(360.), Nanometers::new(390.)],
        WithinBinSpectrum::FlatPhotonDensity,
    )?;
    println!("experimental; sha256={:?}; geometry={geometry:?}; full-native-bin FlatPhotonDensity; black ground",map.sha256());
    let start = std::time::Instant::now();
    let result = starlight_multiple_scattering(
        &medium,
        &phase,
        &sky,
        view,
        MultipleScatteringSettings {
            mu_nodes: 4,
            azimuth_nodes: 4,
            cells_per_layer: 2,
            max_orders: 8,
            angular_tolerance: 0.05,
            vertical_tolerance: 0.05,
            ..MultipleScatteringSettings::default()
        },
        HealpixSettings {
            subdivision: if frozen { 1 } else { 8 },
            refinements: if frozen { 2 } else { 4 },
            ..HealpixSettings::default()
        },
        WavelengthSettings {
            refinements: 1,
            ..WavelengthSettings::default()
        },
    );
    match result {
        Ok(r)=>println!("direct={} first={} higher={} scattered={} total={} wavelength_order={} boundary_nside={:?} work={} elapsed={:?}; convergence={:?}",r.direct,r.single,r.higher_orders,r.total_scattered,r.total,r.wavelength_order,r.boundary_nside,r.planned_work,start.elapsed(),r.samples[0].convergence),
        Err(e)=>{eprintln!("explicit rejection after {:?}: {e}",start.elapsed());return Err(e.into());}
    }
    Ok(())
}
