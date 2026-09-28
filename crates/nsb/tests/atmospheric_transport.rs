//! Authoritative tests for the atmospheric transport foundation (#187).

use nsb::site::AtmosphericConditions;
use nsb::transport::{
    apply_monochromatic, apply_spectral_radiance, rayleigh_phase_value, AirmassModel,
    DirectPathGeometry, DirectTransmission, ExtinctionIngredients, MolecularAbsorption,
    RadianceOrigin, ScatteringGeometry, ScatteringPathStatus, TransportModel, TransportPathKind,
};
use qtty::angular::{Degrees, Radians};
use qtty::dimensionless::Transmittances;
use qtty::radiometry::{
    PhotonsPerSquareCentimeterNanosecondSteradian as BandPhotonRadiance,
    WattsPerSquareMeterSteradianNanometer,
};
use siderust::atmosphere::{airmass, mie_optical_depth};
use siderust::atmosphere::{
    transmission as beer_lambert, AtmosphereProfile, KrisciunasSchaefer1991, Young1994,
};
use siderust::qtty::{Nanometers, OpticalDepths};
use std::f64::consts::PI;

fn paranal() -> AtmosphericConditions {
    AtmosphericConditions::from_profile_without_altitude(AtmosphereProfile::EL_PARANAL)
}

#[test]
fn identity_is_exact_pass_through() {
    let transport = TransportModel::identity();
    let incident = WattsPerSquareMeterSteradianNanometer::new(1.234_567_890_123);
    let wavelength = Nanometers::new(500.0);
    let geometry = DirectPathGeometry::new(Degrees::new(45.0));
    let out = transport.apply_spectral(incident, wavelength, geometry, paranal());
    assert_eq!(out.value(), incident.value());
    assert_eq!(
        transport
            .transmission(wavelength, geometry, paranal())
            .value(),
        1.0
    );
    let meta = transport.metadata("unused");
    assert_eq!(meta.path_kind, TransportPathKind::Identity);
    assert_eq!(meta.model_id, "identity");
}

#[test]
fn identity_ignores_atmosphere_and_zenith() {
    let transport = TransportModel::identity();
    let incident = WattsPerSquareMeterSteradianNanometer::new(2.0);
    let a = transport.apply_spectral(
        incident,
        Nanometers::new(350.0),
        DirectPathGeometry::new(Degrees::new(0.0)),
        paranal(),
    );
    let b = transport.apply_spectral(
        incident,
        Nanometers::new(700.0),
        DirectPathGeometry::new(Degrees::new(80.0)),
        AtmosphericConditions::cta_n_clear_sky(),
    );
    assert_eq!(a.value(), incident.value());
    assert_eq!(b.value(), incident.value());
}

#[test]
fn beer_lambert_matches_known_optical_depth_and_airmass() {
    let tau = OpticalDepths::new(0.2);
    let x = siderust::qtty::Airmasses::new(1.5);
    let expected = (-0.2_f64 * 1.5).exp();
    let got = beer_lambert(tau, x).value();
    assert!((got - expected).abs() < 1e-15);

    // Transport direct path with synthetic zero site ingredients replaced by
    // explicit Beer–Lambert check through the same Siderust primitive.
    let transport = TransportModel::Direct(DirectTransmission::new(
        ExtinctionIngredients::new(false, false, MolecularAbsorption::None),
        AirmassModel::PlaneParallel,
    ));
    let t = transport
        .transmission(
            Nanometers::new(550.0),
            DirectPathGeometry::new(Degrees::new(0.0)),
            paranal(),
        )
        .value();
    assert_eq!(t, 1.0);
}

#[test]
fn direct_transmission_matches_manual_paranal_beer_lambert() {
    let atmosphere = paranal();
    let wavelength = Nanometers::new(550.0);
    let zenith = Degrees::new(30.0);
    let transport = TransportModel::direct_rayleigh_mie();
    let geometry = DirectPathGeometry::new(zenith);

    let breakdown = transport.optical_depth(wavelength, atmosphere);
    let x = airmass::<Young1994>(zenith.to::<qtty::angular::Radian>());
    let expected = beer_lambert(breakdown.total, x).value();
    let got = transport
        .transmission(wavelength, geometry, atmosphere)
        .value();
    assert!(
        (got - expected).abs() < 1e-14,
        "got {got}, expected {expected}"
    );

    let incident = WattsPerSquareMeterSteradianNanometer::new(3.0);
    let out = transport.apply_spectral(incident, wavelength, geometry, atmosphere);
    assert!((out.value() - incident.value() * expected).abs() < 1e-14);
}

#[test]
fn transmission_in_unit_interval() {
    let transport = TransportModel::direct_rayleigh_mie();
    for zenith_deg in [0.0, 30.0, 60.0, 75.0] {
        for wl in [350.0, 450.0, 550.0, 650.0] {
            let t = transport
                .transmission(
                    Nanometers::new(wl),
                    DirectPathGeometry::new(Degrees::new(zenith_deg)),
                    paranal(),
                )
                .value();
            assert!(
                (0.0..=1.0).contains(&t) && t.is_finite(),
                "T={t} at z={zenith_deg}, λ={wl}"
            );
        }
    }
}

#[test]
fn zero_optical_depth_is_identity() {
    let transport = TransportModel::Direct(DirectTransmission::new(
        ExtinctionIngredients::new(false, false, MolecularAbsorption::None),
        AirmassModel::Young1994,
    ));
    let incident = WattsPerSquareMeterSteradianNanometer::new(5.0);
    let out = transport.apply_spectral(
        incident,
        Nanometers::new(500.0),
        DirectPathGeometry::new(Degrees::new(50.0)),
        paranal(),
    );
    assert_eq!(out.value(), incident.value());
}

#[test]
fn increasing_optical_depth_does_not_increase_transmitted_radiance() {
    let atmosphere = paranal();
    let wavelength = Nanometers::new(450.0);
    let geometry = DirectPathGeometry::new(Degrees::new(40.0));
    let incident = WattsPerSquareMeterSteradianNanometer::new(1.0);

    let low = TransportModel::Direct(DirectTransmission::new(
        ExtinctionIngredients::new(true, false, MolecularAbsorption::None),
        AirmassModel::Young1994,
    ));
    let high = TransportModel::Direct(DirectTransmission::new(
        ExtinctionIngredients::new(true, true, MolecularAbsorption::OzoneBundledTable),
        AirmassModel::Young1994,
    ));
    let i_low = low
        .apply_spectral(incident, wavelength, geometry, atmosphere)
        .value();
    let i_high = high
        .apply_spectral(incident, wavelength, geometry, atmosphere)
        .value();
    assert!(i_high <= i_low);
    assert!(
        high.optical_depth(wavelength, atmosphere).total.value()
            >= low.optical_depth(wavelength, atmosphere).total.value()
    );
}

#[test]
fn finite_inputs_produce_finite_outputs() {
    let transport = TransportModel::direct_rayleigh_mie_ozone();
    let out = transport.apply_spectral(
        WattsPerSquareMeterSteradianNanometer::new(1.0),
        Nanometers::new(400.0),
        DirectPathGeometry::new(Degrees::new(25.0)),
        paranal(),
    );
    assert!(out.value().is_finite());
    assert!(out.value() > 0.0);
}

#[test]
fn celestial_direct_rejects_airglow_and_moonlight_origins() {
    let transport = TransportModel::direct_rayleigh_mie();
    let incident = WattsPerSquareMeterSteradianNanometer::new(1.0);
    let geometry = DirectPathGeometry::new(Degrees::new(10.0));
    for origin in [
        RadianceOrigin::AtmosphericEmission,
        RadianceOrigin::PreScatteredAtmosphere,
    ] {
        let err = transport
            .apply_spectral_for_origin(
                origin,
                incident,
                Nanometers::new(500.0),
                geometry,
                paranal(),
            )
            .expect_err("must reject non-TOA origins");
        let msg = err.to_string();
        assert!(msg.contains("unsupported") || msg.contains("top-of-atmosphere"));
    }
}

#[test]
fn starlight_band_radiance_can_be_propagated_without_mutating_product() {
    // Admitted TOA starlight radiance is an input; transport scales a copy.
    let toa = BandPhotonRadiance::new(0.42);
    let transport = TransportModel::direct_rayleigh_mie();
    let geometry = DirectPathGeometry::new(Degrees::new(20.0));
    let ground = apply_monochromatic(
        &transport,
        RadianceOrigin::TopOfAtmosphere,
        toa,
        Nanometers::new(500.0),
        geometry,
        paranal(),
    )
    .expect("TOA starlight may use celestial direct path");
    assert!(ground.value() < toa.value());
    assert!(ground.value() > 0.0);
    // Original TOA value unchanged (Copy quantity; contract is non-mutating API).
    assert_eq!(toa.value(), 0.42);
}

#[test]
fn apply_spectral_radiance_helper_matches_model() {
    let transport = TransportModel::direct_rayleigh_mie();
    let incident = WattsPerSquareMeterSteradianNanometer::new(2.5);
    let wavelength = Nanometers::new(520.0);
    let geometry = DirectPathGeometry::new(Degrees::new(15.0));
    let via_helper = apply_spectral_radiance(
        &transport,
        RadianceOrigin::TopOfAtmosphere,
        incident,
        wavelength,
        geometry,
        paranal(),
    )
    .unwrap();
    let via_model = transport.apply_spectral(incident, wavelength, geometry, paranal());
    assert_eq!(via_helper.value(), via_model.value());
}

#[test]
fn scattered_path_is_explicitly_unimplemented() {
    let transport = TransportModel::direct_rayleigh_mie();
    let scattered = transport.scattered_path(
        ScatteringGeometry::new(Degrees::new(30.0), Degrees::new(20.0), Radians::new(0.5)),
        paranal(),
    );
    assert_eq!(scattered.status, ScatteringPathStatus::NotImplemented);
    assert!(scattered.detail.message.contains("not implemented"));
}

#[test]
fn rayleigh_phase_helper_positive_and_symmetric() {
    let p0 = rayleigh_phase_value(Radians::new(0.0)).value();
    let p180 = rayleigh_phase_value(Radians::new(PI)).value();
    assert!((p0 - p180).abs() < 1e-14);
    assert!(p0 > 0.0);
}

#[test]
fn transport_metadata_does_not_claim_calibration() {
    let meta = TransportModel::direct_rayleigh_mie().metadata("cta-south-planning");
    assert_eq!(meta.atmosphere_profile_id, "cta-south-planning");
    assert!(meta.validated_domain.contains("not a site-calibrated"));
    assert_eq!(meta.uncertainty.as_str(), "absent");
    assert!(meta.extinction.rayleigh);
    assert!(meta.extinction.mie);
}

/// Cross-implementation validation against the nsb2 Beer–Lambert extinction
/// contract for matched optical depth and plane-parallel airmass.
///
/// This compares mathematical identity under shared assumptions. It is **not**
/// independent astrophysical validation of either implementation.
#[test]
fn cross_implementation_validation_nsb2_beer_lambert_plane_parallel() {
    // nsb2 SingleScatteringAtmosphere._compute_extinction:
    //   T = exp(-(τ_R + τ_M + τ_abs) * X(z)) with X(z) = sec(z) for plane-parallel.
    let tau_r = 0.1_f64;
    let tau_m = 0.05_f64;
    let tau_abs = 0.01_f64;
    let tau = tau_r + tau_m + tau_abs;
    let zenith_rad = 40.0_f64.to_radians();
    let x_nsb2 = 1.0 / zenith_rad.cos(); // plane-parallel sec(z)
    let t_nsb2 = (-tau * x_nsb2).exp();

    let t_nsb = beer_lambert(
        OpticalDepths::new(tau),
        airmass::<siderust::atmosphere::PlaneParallel>(Radians::new(zenith_rad)),
    )
    .value();
    assert!(
        (t_nsb - t_nsb2).abs() < 1e-14,
        "NSB Beer–Lambert via Siderust ({t_nsb}) must match nsb2 plane-parallel extinction ({t_nsb2})"
    );
}

#[test]
fn direct_path_uses_local_pressure_rayleigh_not_double_reduced() {
    let atmosphere = paranal();
    let wavelength = Nanometers::new(550.0);
    let transport = TransportModel::direct_rayleigh_mie();
    let breakdown = transport.optical_depth(wavelength, atmosphere);
    // Paranal local pressure Rayleigh at 550 nm is finite and larger than the
    // altitude-double-reduced Bodhaine call (see airglow extinction tests).
    assert!(breakdown.rayleigh.value() > 0.07);
    assert!(breakdown.mie.value() > 0.0);
    let _ = mie_optical_depth(&atmosphere.mie_params, wavelength);
}

#[test]
fn krisciunas_airmass_variant_is_selectable() {
    let transport = TransportModel::Direct(DirectTransmission::new(
        ExtinctionIngredients::RAYLEIGH_MIE,
        AirmassModel::KrisciunasSchaefer1991,
    ));
    let zenith = Degrees::new(45.0);
    let t = transport
        .transmission(
            Nanometers::new(550.0),
            DirectPathGeometry::new(zenith),
            paranal(),
        )
        .value();
    let tau = transport
        .optical_depth(Nanometers::new(550.0), paranal())
        .total;
    let expected = beer_lambert(
        tau,
        airmass::<KrisciunasSchaefer1991>(zenith.to::<qtty::angular::Radian>()),
    )
    .value();
    assert!((t - expected).abs() < 1e-14);
}

#[test]
fn transmission_type_is_transmittances() {
    let t: Transmittances = TransportModel::identity().transmission(
        Nanometers::new(500.0),
        DirectPathGeometry::new(Degrees::new(0.0)),
        paranal(),
    );
    assert_eq!(t.value(), 1.0);
}
