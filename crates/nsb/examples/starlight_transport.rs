// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Vallés Puig, Ramon
//! Offline native-bin transport; optional frozen-map and labelled pilot-kernel fixture.
use nsb::components::starlight::StarlightMap;
use nsb::transport::single::starlight::{
    starlight_healpix_single_scattering, HealpixSettings, StarlightGeometry, StarlightSky,
    WavelengthSettings, WithinBinSpectrum,
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
    let pilot_centers = args.len() == 4 && args[3] == "--pilot-bin-centers";
    let current_centers = args.len() == 4 && args[3] == "--current-bin-centers";
    let (map, tt, ut1) = if args.is_empty() {
        (
            StarlightMap::from_json_bytes(
                include_bytes!("../tests/data/starlight-transport/uniform.json"),
                "35058caa944e51096e5d9ff8ca33eb19be08ef0b6c727ec37192758a64bdc24f",
            )?,
            2459680.8,
            2459680.7992,
        )
    } else if args.len() == 3 || pilot_centers || current_centers {
        (
            StarlightMap::from_json_file(
                std::path::Path::new(&args[0]),
                "fe331343cb9797a1d871ae798c88d2ae8865ae39358e8d82b464a91b3cf87d5f",
            )?,
            args[1].parse()?,
            args[2].parse()?,
        )
    } else {
        return Err("usage: starlight_transport [FROZEN_MAP JD_TT JD_UT1 [--pilot-bin-centers|--current-bin-centers]]; historical centers are a separately labelled optical-kernel fixture".into());
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
    let angular = HealpixSettings {
        refinements: if args.is_empty() { 8 } else { 2 },
        ..HealpixSettings::default()
    };
    println!("model=experimental; map={}; sha256={:?}; geometry={geometry:?}; parent-pixel means; flat photon density; HEALPix subpixel kernel integration; analytic homogeneous-layer path; no physical uncertainty estimate",map.provenance().dataset_name,map.sha256());
    println!(
        "optical_mode={}",
        if pilot_centers {
            "historical Rayleigh/HG bin-center kernels; geometric Siderust frame; pixel means"
        } else if current_centers {
            "Siderust Bodhaine profile; bin-center optical-kernel diagnostic"
        } else {
            "Siderust Bodhaine profile; full-bin wavelength quadrature"
        }
    );
    let mut failed = false;
    let mut total = 0.;
    for edges in map.bin_edges_nm().windows(2) {
        let band = [edges[0], edges[1]].map(|w| Nanometers::new(f64::from(w)));
        let sky = StarlightSky::new(&map, geometry, band, WithinBinSpectrum::FlatPhotonDensity)?;
        let start = std::time::Instant::now();
        // This fixture alone freezes the historical optical kernel at the bin
        // center. It is explicitly distinct from current-model full-bin optics.
        let center = (band[0].value() + band[1].value()) / 2.;
        let compatibility;
        let column = if pilot_centers {
            let u = center / 1000.;
            let rayleigh = (779.5 / 1013.25)
                * 0.008569
                * u.powi(-4)
                * (1. + 0.0113 * u.powi(-2) + 0.00013 * u.powi(-4));
            let aerosol = 0.03 * (center / 550.).powf(-1.38);
            let coefficients = AtmosphericOptics::from_extinction(
                Quantity::new(rayleigh / 10000.),
                Quantity::new(aerosol / 10000.),
                Albedos::new(1.),
                Quantity::new(0.),
            )?;
            compatibility = LayeredAtmosphere::try_new(
                medium.edges(),
                medium.wavelength_domain(),
                vec![coefficients],
            )?;
            &compatibility
        } else if current_centers {
            compatibility = LayeredAtmosphere::try_new(
                medium.edges(),
                medium.wavelength_domain(),
                vec![medium.coefficients(Nanometers::new(center), medium.edges()[0])?],
            )?;
            &compatibility
        } else {
            &medium
        };
        match starlight_healpix_single_scattering(
            column,
            &phase,
            &sky,
            Horizontal::new(Degrees::new(76.9256911), Degrees::new(271.8975544)),
            angular,
            WavelengthSettings::default(),
        ) {
            Ok(r) => {
                total += r.single.value();
                println!("band={band:?} toa={} direct={} single={} higher={:?} wavelength_order={} changes={:?} angular/path={:?} work={} elapsed={:?}",r.toa,r.direct,r.single,r.higher_orders,r.wavelength_order,r.wavelength_change,r.samples,r.planned_work,start.elapsed());
            }
            Err(e) => {
                failed = true;
                eprintln!("band={band:?}: {e}; elapsed={:?}", start.elapsed());
            }
        }
    }
    if failed {
        return Err(
            "unresolved transport; no partial-band total or pilot agreement claimed".into(),
        );
    }
    println!(
        "accepted integrated single={total:.17}; higher orders uncomputed; optical_mode={}",
        if pilot_centers {
            "historical bin-center fixture"
        } else if current_centers {
            "current bin-center diagnostic"
        } else {
            "current full-bin model"
        }
    );
    if pilot_centers {
        println!("relative to historical nsb2 first-order={:.12}; relative to independent first-order={:.12}; exact astrometric conventions not claimed matched",total/0.0025991471011773495-1.,total/0.0025990756849478282-1.);
    }
    Ok(())
}
