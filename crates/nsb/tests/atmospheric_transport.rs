//! Authoritative tests for the atmospheric transport foundation (#187 / #193).

use nsb::site::AtmosphericConditions;
use nsb::transport::{
    AirmassModel, DirectPathGeometry, DirectTransmission, ExtinctionIngredients,
    MolecularAbsorption, RadianceOrigin, TransportModel,
};
use qtty::angular::{Degrees, Radian};
use qtty::radiometry::{
    PhotonsPerSquareCentimeterNanosecondSteradianNanometer as PhotonSpectralRadiance,
    WattsPerSquareMeterSteradianNanometer as EnergySpectralRadiance,
};
use siderust::atmosphere::{
    airmass, transmission as beer_lambert, AtmosphereProfile, KrisciunasSchaefer1991,
    PlaneParallel, Rozenberg1966, Young1994,
};
use siderust::qtty::{Nanometers, OpticalDepths};

fn paranal() -> AtmosphericConditions {
    AtmosphericConditions::from_profile_without_altitude(AtmosphereProfile::EL_PARANAL)
}

fn zenith(deg: f64) -> DirectPathGeometry {
    DirectPathGeometry::new(Degrees::new(deg)).expect("valid test zenith")
}

#[test]
fn identity_is_exact_pass_through_for_energy_and_photon_spectral() {
    let transport = TransportModel::Identity;
    let energy = EnergySpectralRadiance::new(1.234_567_890_123);
    let photon = PhotonSpectralRadiance::new(9.876_543_210);
    let wavelength = Nanometers::new(500.0);
    let geometry = zenith(45.0);

    let e_out = transport
        .apply_energy_spectral(
            RadianceOrigin::TopOfAtmosphere,
            energy,
            wavelength,
            geometry,
            paranal(),
        )
        .unwrap();
    let p_out = transport
        .apply_photon_spectral(
            RadianceOrigin::AtmosphericEmission,
            photon,
            wavelength,
            geometry,
            paranal(),
        )
        .unwrap();
    assert_eq!(e_out.value(), energy.value());
    assert_eq!(p_out.value(), photon.value());
    assert_eq!(
        transport
            .transmission(wavelength, geometry, paranal())
            .unwrap()
            .value(),
        1.0
    );
}

#[test]
fn identity_accepts_all_radiance_origins() {
    let transport = TransportModel::identity();
    let incident = EnergySpectralRadiance::new(1.0);
    for origin in [
        RadianceOrigin::TopOfAtmosphere,
        RadianceOrigin::AtmosphericEmission,
        RadianceOrigin::PreScatteredAtmosphere,
    ] {
        transport
            .apply_energy_spectral(
                origin,
                incident,
                Nanometers::new(550.0),
                zenith(10.0),
                paranal(),
            )
            .unwrap_or_else(|_| panic!("identity must accept {}", origin.as_str()));
    }
}

#[test]
fn beer_lambert_matches_known_optical_depth_and_airmass() {
    let tau = OpticalDepths::new(0.2);
    let x = siderust::qtty::Airmasses::new(1.5);
    let expected = (-0.2_f64 * 1.5).exp();
    assert!((beer_lambert(tau, x).value() - expected).abs() < 1e-15);
}

#[test]
fn direct_transmission_matches_manual_paranal_beer_lambert() {
    let atmosphere = paranal();
    let wavelength = Nanometers::new(550.0);
    let geometry = zenith(30.0);
    let transport = TransportModel::direct_rayleigh_mie();

    let breakdown = transport.optical_depth(wavelength, atmosphere).unwrap();
    let x = airmass::<Young1994>(geometry.zenith().to::<Radian>());
    let expected = beer_lambert(breakdown.total, x).value();
    let got = transport
        .transmission(wavelength, geometry, atmosphere)
        .unwrap()
        .value();
    assert!((got - expected).abs() < 1e-14);

    let incident = EnergySpectralRadiance::new(3.0);
    let out = transport
        .apply_energy_spectral(
            RadianceOrigin::TopOfAtmosphere,
            incident,
            wavelength,
            geometry,
            atmosphere,
        )
        .unwrap();
    assert!((out.value() - incident.value() * expected).abs() < 1e-14);
}

#[test]
fn photon_spectral_scales_by_same_transmission() {
    let transport = TransportModel::direct_rayleigh_mie();
    let wavelength = Nanometers::new(480.0);
    let geometry = zenith(25.0);
    let t = transport
        .transmission(wavelength, geometry, paranal())
        .unwrap()
        .value();
    let incident = PhotonSpectralRadiance::new(2.0);
    let out = transport
        .apply_photon_spectral(
            RadianceOrigin::TopOfAtmosphere,
            incident,
            wavelength,
            geometry,
            paranal(),
        )
        .unwrap();
    assert!((out.value() - incident.value() * t).abs() < 1e-14);
}

#[test]
fn transmission_in_unit_interval() {
    let transport = TransportModel::direct_rayleigh_mie();
    for zenith_deg in [0.0, 30.0, 60.0, 75.0, 90.0] {
        for wl in [350.0, 450.0, 550.0, 650.0] {
            let t = transport
                .transmission(Nanometers::new(wl), zenith(zenith_deg), paranal())
                .unwrap()
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
    let incident = EnergySpectralRadiance::new(5.0);
    let out = transport
        .apply_energy_spectral(
            RadianceOrigin::TopOfAtmosphere,
            incident,
            Nanometers::new(500.0),
            zenith(50.0),
            paranal(),
        )
        .unwrap();
    assert_eq!(out.value(), incident.value());
}

#[test]
fn increasing_optical_depth_does_not_increase_transmitted_radiance() {
    let atmosphere = paranal();
    let wavelength = Nanometers::new(450.0);
    let geometry = zenith(40.0);
    let incident = EnergySpectralRadiance::new(1.0);

    let low = TransportModel::Direct(DirectTransmission::new(
        ExtinctionIngredients::new(true, false, MolecularAbsorption::None),
        AirmassModel::Young1994,
    ));
    let high = TransportModel::Direct(DirectTransmission::new(
        ExtinctionIngredients::new(true, true, MolecularAbsorption::OzoneBundledTable),
        AirmassModel::Young1994,
    ));
    let i_low = low
        .apply_energy_spectral(
            RadianceOrigin::TopOfAtmosphere,
            incident,
            wavelength,
            geometry,
            atmosphere,
        )
        .unwrap()
        .value();
    let i_high = high
        .apply_energy_spectral(
            RadianceOrigin::TopOfAtmosphere,
            incident,
            wavelength,
            geometry,
            atmosphere,
        )
        .unwrap()
        .value();
    assert!(i_high <= i_low);
    assert!(
        high.optical_depth(wavelength, atmosphere)
            .unwrap()
            .total
            .value()
            >= low
                .optical_depth(wavelength, atmosphere)
                .unwrap()
                .total
                .value()
    );
}

#[test]
fn celestial_direct_rejects_airglow_and_moonlight_origins() {
    let transport = TransportModel::direct_rayleigh_mie();
    let incident = EnergySpectralRadiance::new(1.0);
    for origin in [
        RadianceOrigin::AtmosphericEmission,
        RadianceOrigin::PreScatteredAtmosphere,
    ] {
        let err = transport
            .apply_energy_spectral(
                origin,
                incident,
                Nanometers::new(500.0),
                zenith(10.0),
                paranal(),
            )
            .expect_err("must reject non-TOA origins");
        let msg = err.to_string();
        assert!(msg.contains("unsupported configuration") || msg.contains("top-of-atmosphere"));
    }
}

#[test]
fn invalid_geometry_is_rejected_at_construction() {
    assert!(DirectPathGeometry::new(Degrees::new(f64::NAN)).is_err());
    assert!(DirectPathGeometry::new(Degrees::new(-1.0)).is_err());
    assert!(DirectPathGeometry::new(Degrees::new(90.000_1)).is_err());
    assert!(DirectPathGeometry::new(Degrees::new(f64::INFINITY)).is_err());
}

#[test]
fn invalid_wavelength_is_rejected() {
    let transport = TransportModel::direct_rayleigh_mie();
    let geometry = zenith(0.0);
    assert!(transport
        .optical_depth(Nanometers::new(f64::NAN), paranal())
        .is_err());
    assert!(transport
        .transmission(Nanometers::new(0.0), geometry, paranal())
        .is_err());
    assert!(transport
        .apply_energy_spectral(
            RadianceOrigin::TopOfAtmosphere,
            EnergySpectralRadiance::new(1.0),
            Nanometers::new(-10.0),
            geometry,
            paranal(),
        )
        .is_err());
}

#[test]
fn model_metadata_has_no_caller_invented_atmosphere_id() {
    let meta = TransportModel::direct_rayleigh_mie().metadata();
    assert_eq!(meta.model_id, "direct-transmission");
    assert!(meta.rayleigh && meta.mie);
    assert_eq!(meta.absorption, MolecularAbsorption::None);
    assert_eq!(meta.airmass_model_id, "young-1994");
    assert_eq!(meta.approximation, "clear-sky-single-column");
    assert_eq!(meta.uncertainty, "absent");
    assert!(meta.validated_domain.contains("not a site-calibrated"));

    let identity = TransportModel::Identity.metadata();
    assert_eq!(identity.model_id, "identity");
    assert_eq!(identity.approximation, "exact-within-representation");
    assert!(!identity.rayleigh && !identity.mie);
}

#[test]
fn all_airmass_variants_are_selectable_and_finite() {
    let wavelength = Nanometers::new(550.0);
    let geometry = zenith(45.0);
    let atmosphere = paranal();
    for airmass_model in [
        AirmassModel::Young1994,
        AirmassModel::KrisciunasSchaefer1991,
        AirmassModel::PlaneParallel,
        AirmassModel::Rozenberg1966,
    ] {
        let transport = TransportModel::Direct(DirectTransmission::new(
            ExtinctionIngredients::RAYLEIGH_MIE,
            airmass_model,
        ));
        let t = transport
            .transmission(wavelength, geometry, atmosphere)
            .unwrap()
            .value();
        assert!(
            t.is_finite() && (0.0..=1.0).contains(&t),
            "{}",
            airmass_model.as_str()
        );

        let tau = transport
            .optical_depth(wavelength, atmosphere)
            .unwrap()
            .total;
        let expected = match airmass_model {
            AirmassModel::Young1994 => {
                beer_lambert(tau, airmass::<Young1994>(geometry.zenith().to::<Radian>()))
            }
            AirmassModel::KrisciunasSchaefer1991 => beer_lambert(
                tau,
                airmass::<KrisciunasSchaefer1991>(geometry.zenith().to::<Radian>()),
            ),
            AirmassModel::PlaneParallel => beer_lambert(
                tau,
                airmass::<PlaneParallel>(geometry.zenith().to::<Radian>()),
            ),
            AirmassModel::Rozenberg1966 => beer_lambert(
                tau,
                airmass::<Rozenberg1966>(geometry.zenith().to::<Radian>()),
            ),
            other => panic!("unexpected airmass model in test: {}", other.as_str()),
        }
        .value();
        assert!(
            (t - expected).abs() < 1e-14,
            "{}: got {t}, expected {expected}",
            airmass_model.as_str()
        );
    }
}

#[test]
fn extinction_ingredient_configurations_change_optical_depth() {
    let wavelength = Nanometers::new(320.0);
    let atmosphere = paranal();
    let none = ExtinctionIngredients::new(false, false, MolecularAbsorption::None);
    let rayleigh_only = ExtinctionIngredients::new(true, false, MolecularAbsorption::None);
    let mie_only = ExtinctionIngredients::new(false, true, MolecularAbsorption::None);
    let both = ExtinctionIngredients::RAYLEIGH_MIE;
    let with_ozone = ExtinctionIngredients::RAYLEIGH_MIE_OZONE;

    let tau = |ingredients| {
        TransportModel::Direct(DirectTransmission::new(
            ingredients,
            AirmassModel::Young1994,
        ))
        .optical_depth(wavelength, atmosphere)
        .unwrap()
        .total
        .value()
    };

    assert_eq!(tau(none), 0.0);
    assert!(tau(rayleigh_only) > 0.0);
    assert!(tau(mie_only) > 0.0);
    assert!((tau(both) - (tau(rayleigh_only) + tau(mie_only))).abs() < 1e-14);
    assert!(
        tau(with_ozone) > tau(both),
        "ozone should increase τ in the UV (320 nm): with={}, without={}",
        tau(with_ozone),
        tau(both)
    );
}

#[test]
fn direct_transmission_accessors_and_defaults() {
    let default_direct = DirectTransmission::default();
    assert_eq!(
        default_direct.ingredients(),
        ExtinctionIngredients::RAYLEIGH_MIE
    );
    assert_eq!(default_direct.airmass(), AirmassModel::Young1994);
    assert_eq!(TransportModel::default().as_str(), "identity");
    assert_eq!(
        ExtinctionIngredients::default(),
        ExtinctionIngredients::RAYLEIGH_MIE
    );
    assert_eq!(AirmassModel::default().as_str(), "young-1994");
    assert_eq!(MolecularAbsorption::default().as_str(), "none");
    assert_eq!(
        DirectTransmission::rayleigh_mie_ozone_young1994().ingredients(),
        ExtinctionIngredients::RAYLEIGH_MIE_OZONE
    );
    assert_eq!(
        RadianceOrigin::TopOfAtmosphere.as_str(),
        "top-of-atmosphere"
    );
    assert_eq!(
        RadianceOrigin::AtmosphericEmission.as_str(),
        "atmospheric-emission"
    );
    assert_eq!(
        RadianceOrigin::PreScatteredAtmosphere.as_str(),
        "pre-scattered-atmosphere"
    );
}

/// Cross-implementation validation against the nsb2 Beer–Lambert extinction
/// contract for matched optical depth and plane-parallel airmass.
///
/// This compares mathematical identity under shared assumptions. It is **not**
/// independent astrophysical validation of either implementation.
#[test]
fn cross_implementation_validation_nsb2_beer_lambert_plane_parallel() {
    let tau_r = 0.1_f64;
    let tau_m = 0.05_f64;
    let tau_abs = 0.01_f64;
    let tau = tau_r + tau_m + tau_abs;
    let zenith_rad = 40.0_f64.to_radians();
    let x_nsb2 = 1.0 / zenith_rad.cos();
    let t_nsb2 = (-tau * x_nsb2).exp();

    let t_nsb = beer_lambert(
        OpticalDepths::new(tau),
        airmass::<PlaneParallel>(qtty::angular::Radians::new(zenith_rad)),
    )
    .value();
    assert!(
        (t_nsb - t_nsb2).abs() < 1e-14,
        "NSB Beer–Lambert via Siderust ({t_nsb}) must match nsb2 plane-parallel extinction ({t_nsb2})"
    );
}

#[test]
fn direct_path_uses_local_pressure_rayleigh_not_double_reduced() {
    let breakdown = TransportModel::direct_rayleigh_mie()
        .optical_depth(Nanometers::new(550.0), paranal())
        .unwrap();
    assert!(breakdown.rayleigh.value() > 0.07);
    assert!(breakdown.mie.value() > 0.0);
    assert_eq!(breakdown.absorption.value(), 0.0);
}
