// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Vallés Puig, Ramon
//! Complete native-bin multiple scattering; optional checksum-frozen Gaia input.
use nsb::components::starlight::StarlightMap;
use nsb::transport::multiple::starlight::{
    projected_starlight_multiple_scattering, starlight_multiple_scattering, ProjectedSettings,
};
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
    let mut args: Vec<_> = std::env::args().skip(1).collect();
    let mut flag = |name: &str| {
        if let Some(i) = args.iter().position(|a| a == name) {
            args.remove(i);
            true
        } else {
            false
        }
    };
    let projected = flag("--projected");
    let reference_settings = flag("--reference-settings");
    let reference_input = flag("--reference-input");
    let pilot_centers = flag("--pilot-bin-centers");
    let current_centers = flag("--current-bin-centers");
    if pilot_centers && current_centers {
        return Err("choose one optical fixture".into());
    }
    let mut number = |name: &str, default: usize| -> Result<usize, Box<dyn std::error::Error>> {
        if let Some(i) = args.iter().position(|a| a.starts_with(name)) {
            Ok(args.remove(i)[name.len()..].parse()?)
        } else {
            Ok(default)
        }
    };
    let only_band = number("--band=", usize::MAX)?;
    let cells = number("--cells=", 4)?;
    let mu_nodes = number("--mu=", 8)?;
    let max_orders = number("--orders=", 24)?;
    let kernel_mu = number("--kernel-mu=", 128)?;
    let kernel_azimuth = number("--kernel-azimuth=", 64)?;
    let frozen = !args.is_empty();
    if !frozen && (pilot_centers || current_centers || reference_input) {
        return Err(
            "reference optical/geometry fixtures require the frozen map and explicit TT/UT1".into(),
        );
    }
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
    if reference_input {
        use siderust::coordinates::cartesian::Direction;
        use siderust::coordinates::frames::{Galactic, Horizontal as HorizontalFrame, ICRS};
        use siderust::coordinates::transform::{DirectionAstroExt, FromHorizontal};
        let basis: [[f64; 3]; 3] = std::array::from_fn(|i| {
            let local = Direction::<HorizontalFrame>::new_unchecked(
                f64::from(i == 0),
                f64::from(i == 1),
                f64::from(i == 2),
            );
            let eq = FromHorizontal::to_equatorial(
                &local,
                &geometry.ut1,
                &geometry.tt,
                &geometry.observer,
            );
            let icrs: Direction<ICRS> = eq.to_frame(&geometry.tt);
            let gal: Direction<Galactic> =
                siderust::coordinates::transform::TransformFrame::to_frame(&icrs);
            [gal.x(), gal.y(), gal.z()]
        });
        let nodes: Vec<Vec<f64>> = serde_json::from_reader(std::io::stdin())?;
        if nodes.len() + 1 != map.bin_edges_nm().len() {
            return Err("expected one wavelength-node list per original bin".into());
        }
        let mut bins = Vec::new();
        for (edges, nodes) in map.bin_edges_nm().windows(2).zip(nodes) {
            let band = edges.iter().map(|x| f64::from(*x)).collect::<Vec<_>>();
            let center = 0.5 * (band[0] + band[1]);
            let column = optical_fixture(&medium, center, pilot_centers, current_centers)?;
            let mut optics = Vec::new();
            for w in nodes {
                if !w.is_finite() || w < band[0] || w > band[1] {
                    return Err("reference wavelength outside native bin".into());
                }
                let wave = Nanometers::new(w);
                let c = column.coefficients(wave, column.edges()[0])?;
                let length = (*column.edges().last().unwrap() - column.edges()[0]).value();
                let ext = column
                    .vertical_optical_depth(
                        wave,
                        column.edges()[0],
                        *column.edges().last().unwrap(),
                    )?
                    .extinction()
                    .value();
                optics.push([
                    w,
                    c.rayleigh().sigma_s.value() * length,
                    c.aerosol().sigma_s.value() * length,
                    ext,
                ]);
            }
            bins.push(serde_json::json!({"band_nm":band,"optics":optics}));
        }
        println!(
            "{}",
            serde_json::json!({"source_sha256":map.sha256(),"galactic_to_horizontal_basis":basis,
            "tt_jd":tt,"ut1_jd":ut1,"view_altitude_deg":view.alt().value(),"view_azimuth_deg":view.az().value(),
            "optical_mode":if pilot_centers {"historical_centers"} else if current_centers {"current_centers"} else {"current_full_bin"},
            "bins":bins})
        );
        return Ok(());
    }
    // Evaluate every original native bin. Failures remain explicit records with
    // absent radiances; an incomplete seven-band run exits unsuccessfully.
    if only_band != usize::MAX && only_band >= map.bin_edges_nm().len().saturating_sub(1) {
        return Err("requested native bin index is unavailable".into());
    }
    let mut failed = false;
    for (bin, edges) in map.bin_edges_nm().windows(2).enumerate() {
        if only_band != usize::MAX && only_band != bin {
            continue;
        }
        let band = [
            Nanometers::new(f64::from(edges[0])),
            Nanometers::new(f64::from(edges[1])),
        ];
        let sky = StarlightSky::new(&map, geometry, band, WithinBinSpectrum::FlatPhotonDensity)?;
        let start = std::time::Instant::now();
        let center = 0.5 * (band[0].value() + band[1].value());
        let column = optical_fixture(&medium, center, pilot_centers, current_centers)?;
        let mut projection_info = serde_json::Value::Null;
        let profile_heap = std::env::var_os("NSB_PROFILE_HEAP").is_some();
        if profile_heap {
            eprintln!("NSB_WORKSPACE_BEGIN");
        }
        let result = if projected {
            projected_starlight_multiple_scattering(&column,&phase,&sky,view,ProjectedSettings {
                solver: if reference_settings {MultipleScatteringSettings {mu_nodes:4,azimuth_nodes:4,cells_per_layer:2,max_orders:8,angular_tolerance:0.05,vertical_tolerance:0.05,..MultipleScatteringSettings::default()}}
                    else {MultipleScatteringSettings { mu_nodes,azimuth_nodes:32,cells_per_layer:cells,max_orders,
                    angular_tolerance:0.01,vertical_tolerance:0.01,order_tolerance:1e-6,..MultipleScatteringSettings::default() }},
                boundary:HealpixSettings {subdivision:if frozen {1} else if reference_settings {8} else {32},refinements:if reference_settings {4} else {2},..HealpixSettings::default()},
                wavelength:WavelengthSettings {nodes:if reference_settings {2} else {4},refinements:1,..WavelengthSettings::default()},kernel_mu,kernel_azimuth,
            }).map(|r| { projection_info=serde_json::json!({"kernel_grid":r.kernel_grid,"workspace_bound_bytes":r.workspace_bytes,
                "kernel_changes":r.kernel_changes.iter().map(|x|x.map(|v|v.map(|q|q.value()))).collect::<Vec<_>>()}); r.transport })
        } else {
            starlight_multiple_scattering(
                &column,
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
            )
        };
        if profile_heap {
            eprintln!("NSB_WORKSPACE_END");
        }
        let mut record = serde_json::json!({
            "schema": "nsb-multiple-scattering-example-v1",
            "source_sha256": map.sha256(),
            "partial_band_diagnostic":only_band != usize::MAX,
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
            "projection":projection_info,
            "frame":"Galactic", "ordering":"RING", "source_origin":"TOA",
            "phase":"Optica HG", "hg_g":if frozen {0.65} else {0.0},
            "pressure_hpa":if frozen {serde_json::json!(779.5)} else {serde_json::Value::Null},
            "aerosol_tau550":if frozen {0.03} else {0.0},
            "angstrom_exponent":if frozen {1.38} else {0.0},
            "aerosol_ssa":if frozen {1.0} else {0.0},
            "angular_tolerance":if projected && !reference_settings {0.01} else {0.05},
            "vertical_tolerance":if projected && !reference_settings {0.01} else {0.05},
            "order_tolerance":if projected && !reference_settings {1e-6} else {1e-5},
            "boundary_tolerance":1e-4,"kernel_tolerance":1e-4,"wavelength_tolerance":1e-5,
            "model":if projected {"nsb-plane-parallel-scalar-sos-fourier-projected-v1-experimental"} else {"nsb-plane-parallel-scalar-multiple-sos-v0-experimental"},
            "solver_cells_per_layer": if projected && !reference_settings {cells} else {2},
            "optical_mode":if pilot_centers {"historical_centers"} else if current_centers {"current_centers"} else {"current_full_bin"},
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
                        "direct":s.direct.value(),"single":s.single.value(),"higher":s.higher_orders.value(),
                        "diffuse_grid_work":c.planned_work,
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

// Explicit reference fixture only: never changes the production atmosphere.
fn optical_fixture(
    medium: &LayeredAtmosphere,
    center: f64,
    historical: bool,
    current: bool,
) -> Result<LayeredAtmosphere, Box<dyn std::error::Error>> {
    if !historical && !current {
        return Ok(medium.clone());
    }
    let optics = if historical {
        let u = center / 1000.;
        let ray = (779.5 / 1013.25)
            * 0.008569
            * u.powi(-4)
            * (1. + 0.0113 * u.powi(-2) + 0.00013 * u.powi(-4));
        let aer = 0.03 * (center / 550.).powf(-1.38);
        AtmosphericOptics::from_extinction(
            Quantity::new(ray / 10000.),
            Quantity::new(aer / 10000.),
            Albedos::new(1.),
            Quantity::new(0.),
        )?
    } else {
        medium.coefficients(Nanometers::new(center), medium.edges()[0])?
    };
    Ok(LayeredAtmosphere::try_new(
        medium.edges(),
        medium.wavelength_domain(),
        vec![optics],
    )?)
}
