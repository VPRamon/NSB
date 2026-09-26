use super::*;
use crate::spectra::solar::SolarSpectrum;
use crate::units::s10_for_spectral_photon_radiance;
use crate::units::ScaleFactors;
use optica::grid::OutOfRange;
use optica::spectrum::{Interpolation, SampledSpectrum};
use qtty::length::Nanometer;
use qtty::radiometry::{
    PhotonPerSquareCentimeterNanosecondSteradian as BandPhotonRadianceUnit,
    PhotonPerSquareCentimeterNanosecondSteradianNanometer as SpectralBandPhotonRadianceUnit,
};

/// Wavelength-resolved Jones et al. (2013) scattered-moonlight evaluator.
pub(crate) struct Jones2013Spectral {
    location: Geodetic<ECEF>,
    conditions: AtmosphericConditions,
}

impl Jones2013Spectral {
    pub(crate) fn new(location: Geodetic<ECEF>, conditions: AtmosphericConditions) -> Self {
        Self {
            location,
            conditions,
        }
    }

    pub(crate) fn compute(
        &self,
        time: Time<UTC>,
        target: SphericalDirection<EquatorialMeanJ2000>,
    ) -> Result<MoonOutputs> {
        let geometry = lunar_geometry(time, self.location, target);
        compute_jones_2013_spectral(
            &geometry,
            bundled_solar_spectrum(),
            ScaleFactors::new(1.0),
            self.atmosphere_profile(),
        )
    }

    fn atmosphere_profile(&self) -> AtmosphereProfile {
        AtmosphereProfile {
            surface_pressure: self.conditions.surface_pressure,
            observer_altitude: self.location.height.to::<Kilometer>(),
            rayleigh_scale_height: self.conditions.rayleigh_scale_height,
            mie_params: self.conditions.mie_params,
        }
    }
}

fn compute_jones_2013_spectral(
    inp: &MoonlightGeometry,
    solar: &SolarSpectrum,
    tau_scale: ScaleFactors,
    profile: AtmosphereProfile,
) -> Result<MoonOutputs> {
    if !inp.moon_zenith.is_finite()
        || !inp.source_zenith.is_finite()
        || !inp.separation.is_finite()
        || !tau_scale.is_finite()
        || !inp.moon_distance.is_finite()
    {
        return Ok(zero_outputs());
    }
    if inp.moon_zenith >= Degrees::new(90.0)
        || inp.source_zenith >= Degrees::new(90.0)
        || inp.separation <= Degrees::new(0.0)
        || inp.moon_distance <= Kilometers::new(0.0)
    {
        return Ok(zero_outputs());
    }

    let mie_params = profile.mie_params;

    let mie = mie_grid();
    let correction = correction_grid();
    let am_moon = airmass::<KrisciunasSchaeferAirmass>(inp.moon_zenith.to::<Radian>());
    let am_src = airmass::<KrisciunasSchaeferAirmass>(inp.source_zenith.to::<Radian>());
    let tau_scale = tau_scale.value();

    let mut lam = Vec::new();
    let mut density = Vec::new();
    for (&lambda_nm, &solar_irradiance) in solar.xs_raw().iter().zip(solar.ys_raw()) {
        if !(WL_LOW.value()..=WL_HIGH.value()).contains(&lambda_nm) {
            continue;
        }
        let wavelength = Nanometers::new(lambda_nm);
        let lunar_radiance = reflected_lunar_spectral_radiance_jones2013(
            solar_irradiance,
            wavelength,
            inp.phase.phase_angle,
            inp.moon_distance,
        );
        if !lunar_radiance.value().is_finite() || lunar_radiance.value() <= 0.0 {
            continue;
        }
        let lunar_ph = spectral_radiance_to_photon_radiance_ns_nm(
            WattsPerSquareMeterSteradianNanometer::new(lunar_radiance.value()),
            wavelength,
        );
        let tau_r = rayleigh_optical_depth_bodhaine99(
            wavelength,
            profile.surface_pressure,
            profile.observer_altitude,
            profile.rayleigh_scale_height,
        )
        .value()
            * tau_scale;
        let tau_m = mie_optical_depth(&mie_params, wavelength).value() * tau_scale;
        let phase_r = rayleigh_phase(inp.separation.to::<Radian>()).value();
        let phase_m = mie.lookup(inp.separation, wavelength);
        let multi = correction.lookup(inp.separation, wavelength);
        let am_moon_v = am_moon.value();
        let am_src_v = am_src.value();
        let scatter = (tau_r * phase_r + tau_m * JONES_MIE_WEIGHT * phase_m).max(0.0);
        let transmission = (-(tau_r + tau_m) * 0.5 * (am_moon_v + am_src_v)).exp();
        let source_path = 1.0 - (-(tau_r + tau_m) * am_src_v).exp();
        let value = (lunar_ph * scatter * transmission * source_path.max(0.0) * multi).value();
        if value.is_finite() && value > 0.0 {
            lam.push(lambda_nm);
            density.push(value);
        }
    }

    if lam.len() < 2 {
        return Ok(zero_outputs());
    }

    let spectrum = SampledSpectrum::<Nanometer, SpectralBandPhotonRadianceUnit>::from_raw(
        lam,
        density,
        Interpolation::Linear,
        OutOfRange::ClampToEndpoints,
        None,
    )
    .map_err(|error| {
        crate::error::NsbError::Interpolation(format!("Jones 2013 moonlight spectrum: {error}"))
    })?;
    let integrated = spectrum
        .integrate_range(WL_LOW, WL_HIGH)
        .to::<BandPhotonRadianceUnit>();
    let b_density = spectrum.interp_at(B_FILTER);
    let v_density = spectrum.interp_at(V_FILTER);

    Ok(MoonOutputs {
        integrated,
        b_flux_s10: s10_for_spectral_photon_radiance(b_density, B_FILTER),
        v_flux_s10: s10_for_spectral_photon_radiance(v_density, V_FILTER),
    })
}

fn mie_grid() -> &'static ScatterGrid {
    static GRID: OnceLock<ScatterGrid> = OnceLock::new();
    GRID.get_or_init(|| ScatterGrid::mie_phase().expect("bundled Mie phase grid"))
}

fn correction_grid() -> &'static ScatterGrid {
    static GRID: OnceLock<ScatterGrid> = OnceLock::new();
    GRID.get_or_init(|| {
        ScatterGrid::multiple_scattering_correction().expect("bundled scattering correction grid")
    })
}

fn bundled_solar_spectrum() -> &'static SolarSpectrum {
    static SPECTRUM: OnceLock<SolarSpectrum> = OnceLock::new();
    SPECTRUM.get_or_init(|| solar::load().expect("bundled solar spectrum"))
}

#[cfg(test)]
mod tests {
    use super::*;
    fn make_phase(alpha_deg: f64) -> MoonPhaseGeometry {
        use siderust::qtty::{IlluminationFractions, Radians};
        MoonPhaseGeometry {
            phase_angle: Radians::new(alpha_deg.to_radians()),
            illuminated_fraction: IlluminationFractions::new(
                0.5 * (1.0 + alpha_deg.to_radians().cos()),
            ),
            elongation: Radians::new(0.0),
            waxing: true,
        }
    }

    fn geometry(
        phase_deg: f64,
        separation_deg: f64,
        moon_zenith_deg: f64,
        source_zenith_deg: f64,
        moon_distance_km: f64,
    ) -> MoonlightGeometry {
        MoonlightGeometry {
            separation: Degrees::new(separation_deg),
            moon_zenith: Degrees::new(moon_zenith_deg),
            phase: make_phase(phase_deg),
            source_zenith: Degrees::new(source_zenith_deg),
            moon_distance: Kilometers::new(moon_distance_km),
        }
    }

    fn paranal_like_profile() -> AtmosphereProfile {
        let conditions = AtmosphericConditions::paranal_average();
        AtmosphereProfile {
            surface_pressure: conditions.surface_pressure,
            observer_altitude: Kilometers::new(2.635),
            rayleigh_scale_height: conditions.rayleigh_scale_height,
            mie_params: conditions.mie_params,
        }
    }

    fn hsrs_from_env(variable: &str) -> SolarSpectrum {
        let path = std::env::var(variable).unwrap_or_else(|_| panic!("{variable} path"));
        let raw = std::fs::read_to_string(path).expect("HSRS CSV");
        let mut wavelengths = Vec::new();
        let mut irradiances = Vec::new();
        for line in raw.lines().skip(1) {
            let (wavelength, irradiance) = line.split_once(',').expect("two columns");
            wavelengths.push(wavelength.parse().expect("wavelength"));
            irradiances.push(irradiance.parse().expect("irradiance"));
        }
        SolarSpectrum::from_raw(
            wavelengths,
            irradiances,
            Interpolation::Linear,
            OutOfRange::ClampToEndpoints,
            None,
        )
        .expect("HSRS spectrum")
    }

    /// Regression pins for the three historical fixture geometries.
    ///
    /// The CSV `expected_*` columns remain a schema/tolerance manifest for
    /// external references. Those historical LUT values diverge from the
    /// current spectral implementation (~85% relative). These pins were
    /// intentionally refreshed for the reviewed TSIS-1 HSRS v2 replacement;
    /// the validation report records the 1.94–2.10% scientific delta.
    #[test]
    fn historical_fixture_geometries_match_spectral_regression_pins() {
        const REL_TOL: f64 = 1.0e-9;
        let cases = [
            (85.5, 97.523, 36.0, 60.0, 384_400.0, 0.083_367_447_328_456_6),
            (85.5, 4.0, 36.0, 40.0, 384_400.0, 0.308_541_289_220_820_8),
            (85.5, 52.216, 62.0, 15.0, 384_400.0, 0.067_532_264_783_783_4),
        ];
        let profile = paranal_like_profile();
        for (phase, sep, z_moon, z_src, dist, expected) in cases {
            let out = compute_jones_2013_spectral(
                &geometry(phase, sep, z_moon, z_src, dist),
                bundled_solar_spectrum(),
                crate::units::ScaleFactors::new(1.0),
                profile,
            )
            .expect("spectral evaluate");
            let actual = out.integrated.value();
            let rel = (actual - expected).abs() / expected.max(1.0e-12);
            assert!(
                rel <= REL_TOL,
                "geometry phase={phase} sep={sep}: actual={actual} expected={expected} rel={rel}"
            );
            assert!(actual > 0.0);
        }
    }

    /// Reproducible resolution study used by the solar-spectrum validation report.
    #[test]
    #[ignore = "requires NSB_TSIS_P025 and NSB_TSIS_NATIVE from the solar-spectrum update workspace"]
    fn native_hsrs_resolution_comparison() {
        let selected = hsrs_from_env("NSB_TSIS_P025");
        let native = hsrs_from_env("NSB_TSIS_NATIVE");
        let profile = paranal_like_profile();
        for (phase, separation, moon_zenith, source_zenith, distance) in [
            (85.5, 97.523, 36.0, 60.0, 384_400.0),
            (85.5, 4.0, 36.0, 40.0, 384_400.0),
            (85.5, 52.216, 62.0, 15.0, 384_400.0),
        ] {
            let case = geometry(phase, separation, moon_zenith, source_zenith, distance);
            let output = compute_jones_2013_spectral(
                &case,
                &native,
                crate::units::ScaleFactors::new(1.0),
                profile,
            )
            .expect("native-resolution evaluation");
            let candidate = compute_jones_2013_spectral(
                &case,
                &selected,
                crate::units::ScaleFactors::new(1.0),
                profile,
            )
            .expect("candidate-resolution evaluation");
            eprintln!(
                "phase={phase} separation={separation} candidate=({:.17},{:.17},{:.17}) native=({:.17},{:.17},{:.17})",
                candidate.integrated.value(),
                candidate.b_flux_s10.value(),
                candidate.v_flux_s10.value(),
                output.integrated.value(),
                output.b_flux_s10.value(),
                output.v_flux_s10.value()
            );
            // Source-selection gates (p025nm ↔ native). Measured maxima:
            // ~0.00116% integrated, ~1.19% B, ~0.27% V.
            assert!(
                ((candidate.integrated.value() / output.integrated.value()) - 1.0).abs() < 2.0e-5
            );
            assert!(
                ((candidate.b_flux_s10.value() / output.b_flux_s10.value()) - 1.0).abs() < 0.0125
            );
            assert!(
                ((candidate.v_flux_s10.value() / output.v_flux_s10.value()) - 1.0).abs() < 3.0e-3
            );
        }
    }

    /// Scientific error budget for the compact runtime representation.
    #[test]
    #[ignore = "requires NSB_TSIS_P025 from the solar-spectrum update workspace"]
    fn compact_runtime_hsrs_comparison() {
        let selected = hsrs_from_env("NSB_TSIS_P025");
        let profile = paranal_like_profile();
        for (phase, separation, moon_zenith, source_zenith, distance) in [
            (85.5, 97.523, 36.0, 60.0, 384_400.0),
            (85.5, 4.0, 36.0, 40.0, 384_400.0),
            (85.5, 52.216, 62.0, 15.0, 384_400.0),
        ] {
            let case = geometry(phase, separation, moon_zenith, source_zenith, distance);
            let runtime = compute_jones_2013_spectral(
                &case,
                bundled_solar_spectrum(),
                crate::units::ScaleFactors::new(1.0),
                profile,
            )
            .expect("compact runtime evaluation");
            let selected = compute_jones_2013_spectral(
                &case,
                &selected,
                crate::units::ScaleFactors::new(1.0),
                profile,
            )
            .expect("p025nm evaluation");
            eprintln!(
                "phase={phase} separation={separation} runtime=({:.17},{:.17},{:.17}) p025nm=({:.17},{:.17},{:.17})",
                runtime.integrated.value(),
                runtime.b_flux_s10.value(),
                runtime.v_flux_s10.value(),
                selected.integrated.value(),
                selected.b_flux_s10.value(),
                selected.v_flux_s10.value()
            );
            assert!(
                ((runtime.integrated.value() / selected.integrated.value()) - 1.0).abs() < 5.0e-5
            );
            assert!(
                ((runtime.b_flux_s10.value() / selected.b_flux_s10.value()) - 1.0).abs() < 1.0e-12
            );
            assert!(
                ((runtime.v_flux_s10.value() / selected.v_flux_s10.value()) - 1.0).abs() < 1.0e-12
            );
        }
    }

    /// Release-mode timing evidence plus a deterministic complexity guard.
    #[test]
    #[ignore = "benchmark evidence; requires NSB_TSIS_P025"]
    fn compact_runtime_jones_performance_evidence() {
        use std::hint::black_box;
        use std::time::Instant;

        let selected = hsrs_from_env("NSB_TSIS_P025");
        let runtime = bundled_solar_spectrum();
        assert!(
            selected.xs_raw().len() / runtime.xs_raw().len() >= 100,
            "runtime complexity reduction must remain at least 100x"
        );
        let case = geometry(85.5, 52.216, 62.0, 15.0, 384_400.0);
        let profile = paranal_like_profile();
        let repeats = 10;

        let started = Instant::now();
        for _ in 0..repeats {
            black_box(
                compute_jones_2013_spectral(
                    &case,
                    runtime,
                    crate::units::ScaleFactors::new(1.0),
                    profile,
                )
                .unwrap(),
            );
        }
        let runtime_elapsed = started.elapsed();

        let started = Instant::now();
        for _ in 0..repeats {
            black_box(
                compute_jones_2013_spectral(
                    &case,
                    &selected,
                    crate::units::ScaleFactors::new(1.0),
                    profile,
                )
                .unwrap(),
            );
        }
        let selected_elapsed = started.elapsed();
        eprintln!(
            "Jones {repeats} evaluations: runtime={runtime_elapsed:?}, p025nm={selected_elapsed:?}, speedup={:.2}x, samples={}/{}",
            selected_elapsed.as_secs_f64() / runtime_elapsed.as_secs_f64(),
            runtime.xs_raw().len(),
            selected.xs_raw().len()
        );
    }
}
