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
    // Evaluate every original native bin. Failures remain explicit records with
    // absent radiances; an incomplete seven-band run exits unsuccessfully.
    let mut failed = false;
    for edges in map.bin_edges_nm().windows(2) {
        let band = [
            Nanometers::new(f64::from(edges[0])),
            Nanometers::new(f64::from(edges[1])),
        ];
        let sky = StarlightSky::new(&map, geometry, band, WithinBinSpectrum::FlatPhotonDensity)?;
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
        let mut record = serde_json::json!({
            "schema": "nsb-multiple-scattering-example-v1",
            "source_sha256": map.sha256(),
            "band_nm": band.map(|w| w.value()),
            "units": "photon/(cm2 ns sr)",
            "within_bin": "FlatPhotonDensity",
            "tt_jd": tt, "ut1_jd": ut1,
            "observer_longitude_deg": -111.59989, "observer_latitude_deg": 31.96403,
            "observer_wgs84_ellipsoidal_height_m": 2097.0,
            "atmosphere_vertical_datum": "declared model height; no geoid conversion asserted",
            "atmosphere_edges_m": if frozen { [2097., 12097.] } else { [2097., 3097.] },
            "view_altitude_deg": view.alt().value(), "view_azimuth_deg": view.az().value(),
            "ground": "black", "experimental": true,
            "elapsed_seconds": start.elapsed().as_secs_f64(),
            "radiances": null, "error": null,
        });
        match result {
            Ok(r) => {
                record["accepted"] = true.into();
                record["radiances"] = serde_json::json!({
                    "toa": r.toa.value(), "direct": r.direct.value(),
                    "single": r.single.value(), "higher": r.higher_orders.value(),
                    "scattered": r.total_scattered.value(), "total": r.total.value(),
                    "physical_uncertainty": null,
                });
                record["planned_work"] = r.planned_work.into();
                record["boundary_nside"] = serde_json::json!(r.boundary_nside);
                record["wavelength_order"] = r.wavelength_order.into();
                record["wavelength_change"] =
                    serde_json::json!(r.wavelength_change.map(|x| x.value()));
                record["convergence"] =
                    serde_json::json!(r.samples.iter().map(|s| {
                    let c = &s.convergence;
                    serde_json::json!({
                        "wavelength_nm": s.wavelength.value(), "angular_nodes": c.angular_nodes,
                        "cells_per_layer": c.cells_per_layer, "orders": c.orders,
                        "angular_change": c.angular_change.value(),
                        "vertical_change": c.vertical_change.value(),
                        "order_tail_bound": c.order_tail_bound.value(),
                        "contraction_bound": c.contraction_bound,
                        "flux_balance_relative": c.flux_balance_relative,
                    })
                }).collect::<Vec<_>>());
            }
            Err(e) => {
                failed = true;
                record["accepted"] = false.into();
                record["error"] = e.to_string().into();
            }
        }
        println!("{}", serde_json::to_string(&record)?);
    }
    if failed {
        return Err("one or more native bins failed scientific/resource acceptance".into());
    }
    Ok(())
}
