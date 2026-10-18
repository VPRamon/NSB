// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Vallés Puig, Ramon
//! Offline synthetic demo; optional frozen-map diagnostic (never exact pilot replay).
use nsb::components::starlight::StarlightMap;
use nsb::transport::single::starlight::{
    starlight_single_scattering, StarlightGeometry, StarlightSky, WavelengthSettings,
    WithinBinSpectrum,
};
use nsb::transport::single::{AerosolPhase, SolverSettings};
use nsb::units::qtty::{Albedos, Degrees, Hectopascals, Meters, Nanometers, OpticalDepths};
use nsb::Observer;
use siderust::atmosphere::{AtmosphereProfile, LayeredAtmosphere, MieParams};
use siderust::coordinates::spherical::direction::Horizontal;
use siderust::optica::phase::HenyeyGreensteinPhaseFunction;
use tempoch::JulianDate;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    let (map, tt, ut1) = if args.is_empty() {
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
        return Err("usage: starlight_transport [FROZEN_MAP JD_TT JD_UT1]; diagnostic only, Bodhaine/full-bin kernels differ from pilot".into());
    };
    let geometry = StarlightGeometry {
        observer: Observer::new(
            Degrees::new(-111.59989),
            Degrees::new(31.96403),
            Meters::new(2097.0),
        ),
        tt: JulianDate::new(tt),
        ut1: JulianDate::new(ut1),
    };
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
    let medium = LayeredAtmosphere::from_profile(
        profile,
        &[Meters::new(2097.), Meters::new(12097.)],
        [Nanometers::new(360.), Nanometers::new(570.)],
        &[1.],
        &[1.],
        Albedos::new(1.),
    )?;
    let phase = AerosolPhase::HenyeyGreenstein(HenyeyGreensteinPhaseFunction::try_new(0.65)?);
    let solver = if args.is_empty() {
        SolverSettings {
            refinements: 2,
            ..SolverSettings::default()
        }
    } else {
        // Dense angular diagnostic: full spectral plan still fits 250M work.
        SolverSettings {
            path_nodes: 2,
            refinements: 4,
            ..SolverSettings::default()
        }
    };
    println!("model=experimental; map={}; sha256={:?}; geometry={geometry:?}; containing-pixel averages; flat photon density; no physical uncertainty estimate; angular resolution changes do not certify arbitrary pixel structure",map.provenance().dataset_name,map.sha256());
    let mut failed = false;
    for edges in map.bin_edges_nm().windows(2) {
        let band = [edges[0], edges[1]].map(|w| Nanometers::new(f64::from(w)));
        let sky = StarlightSky::new(&map, geometry, band, WithinBinSpectrum::FlatPhotonDensity)?;
        let start = std::time::Instant::now();
        match starlight_single_scattering(&medium,&phase,&sky,
            Horizontal::new(Degrees::new(76.9256911),Degrees::new(271.8975544)),
            solver,WavelengthSettings::default()) {
            Ok(r) => println!("band={band:?} toa={} direct={} single={} higher={:?} wavelength_order={} changes={:?} angular/path={:?} work={} elapsed={:?}",r.toa,r.direct,r.single,r.higher_orders,r.wavelength_order,r.wavelength_change,r.samples,r.planned_work,start.elapsed()),
            Err(e) => { failed=true; eprintln!("band={band:?}: {e}; elapsed={:?}",start.elapsed()); }
        }
    }
    if failed {
        return Err(
            "unresolved transport; no partial-band total or pilot agreement claimed".into(),
        );
    }
    Ok(())
}
