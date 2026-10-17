// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Vallés Puig, Ramon
use super::*;
use crate::spectra::solar::SolarSpectrum;
use crate::units::dimensionless::{Airmasses, OpticalDepths, Transmittance};
use crate::units::length::Nanometer;
use crate::units::radiometry::{
    PhotonPerSquareCentimeterNanosecondSteradian as BandPhotonRadianceUnit,
    PhotonPerSquareCentimeterNanosecondSteradianNanometer as SpectralBandPhotonRadianceUnit,
};
use crate::units::s10_for_spectral_photon_radiance;
use crate::units::{
    Per, Quantity, ScaleFactors, SolarSpectralIrradiance, SolarSpectralIrradianceUnit,
};

// The upstream Rayleigh helper uses a dimensionless phase marker. At the
// radiometric boundary its unit-integral phase density is per steradian.
type PhaseDensity =
    Quantity<Per<radiometry::WattPerSquareMeterSteradianNanometer, SolarSpectralIrradianceUnit>>;
use siderust::optica::grid::OutOfRange;
use siderust::optica::spectrum::{Interpolation, SampledSpectrum};

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
        self.compute_with_sky(time, target, &MoonlightSky::at(time, self.location))
    }

    pub(crate) fn compute_with_sky(
        &self,
        time: Time<UTC>,
        target: SphericalDirection<EquatorialMeanJ2000>,
        sky: &MoonlightSky,
    ) -> Result<MoonOutputs> {
        let geometry = sky.geometry(time.to::<TT>().to::<JD>(), self.location, target);
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
    inp.validate()?;
    if !tau_scale.is_finite() {
        return Err(crate::NsbError::OutOfRange(
            "non-finite Jones optical-depth scale".into(),
        ));
    }
    if inp.moon_zenith >= Degrees::new(90.0)
        || inp.source_zenith >= Degrees::new(90.0)
        || inp.separation <= Degrees::new(0.0)
    {
        return Ok(zero_outputs());
    }

    let mie_params = profile.mie_params;

    let mie = mie_grid();
    let correction = correction_grid();
    let am_moon = airmass::<KrisciunasSchaeferAirmass>(inp.moon_zenith.to::<Radian>());
    let am_src = airmass::<KrisciunasSchaeferAirmass>(inp.source_zenith.to::<Radian>());

    let mut lam = Vec::with_capacity(solar.len());
    let mut density = Vec::with_capacity(solar.len());
    for (&lambda_nm, &solar_irradiance) in solar.xs_raw().iter().zip(solar.ys_raw()) {
        if !(WL_LOW.value()..=WL_HIGH.value()).contains(&lambda_nm) {
            continue;
        }
        let wavelength = Nanometers::new(lambda_nm);
        let lunar_irradiance: SolarSpectralIrradiance =
            reflected_lunar_spectral_irradiance_jones2013(
                solar_irradiance,
                wavelength,
                inp.phase.phase_angle,
                inp.moon_distance,
            );
        if !lunar_irradiance.is_finite() {
            return Err(crate::NsbError::OutOfRange(
                "non-finite reflected lunar irradiance".into(),
            ));
        }
        if lunar_irradiance.value() < 0.0 {
            return Err(crate::NsbError::OutOfRange(
                "negative reflected lunar irradiance".into(),
            ));
        }
        if lunar_irradiance.value() == 0.0 {
            // Zero is a real spectral sample, not a gap to interpolate across.
            lam.push(lambda_nm);
            density.push(0.0);
            continue;
        }
        let tau_r = crate::site::atmosphere::rayleigh_optical_depth_local_pressure(
            wavelength,
            AtmosphericConditions::from_profile_without_altitude(profile),
        ) * tau_scale.value();
        let tau_m = mie_optical_depth(&mie_params, wavelength) * tau_scale.value();
        let phase_r = PhaseDensity::new(rayleigh_phase(inp.separation.to::<Radian>()).value());
        let phase_m = ScaleFactors::new(mie.lookup(inp.separation, wavelength));
        let multi = ScaleFactors::new(correction.lookup(inp.separation, wavelength));
        // Siderust returns disc-integrated beam irradiance (W m^-2 nm^-1).
        // Rayleigh is normalized to one; the Mie table integrates to 4π.
        // The kernel makes that normalization explicit while preserving the
        // empirical coefficient defined against the original Mie convention.
        // It converts irradiance to sky radiance once, without another lunar
        // solid-angle conversion.
        let kernel =
            single_scattering_midpoint_kernel(tau_r, tau_m, phase_r, phase_m, am_moon, am_src);
        let sky_radiance = (lunar_irradiance * kernel) * multi.value();
        let value = spectral_radiance_to_photon_radiance_ns_nm(sky_radiance, wavelength).value();
        if !value.is_finite() {
            return Err(crate::NsbError::OutOfRange(
                "non-finite scattered lunar radiance".into(),
            ));
        }
        if value < 0.0 {
            return Err(crate::NsbError::OutOfRange(
                "negative scattered lunar radiance".into(),
            ));
        }
        lam.push(lambda_nm);
        density.push(value);
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

/// Component-specific midpoint reduction of Jones Eq. (6).
///
/// Let u be the fraction of vertical optical depth below a scattering point.
/// For a shared vertical species profile, the line-of-sight integral is
/// X_target (τ_R p_R + w_eff τ_M p_M) ∫₀¹
/// exp[-τ (X_moon (1-u) + X_target u)] du.
/// Evaluating the integrand at u=1/2 retains this implementation's symmetric
/// mean-path approximation. The column is already in τ_R and τ_M; multiplying
/// again by (1-exp(-τ X_target)) incorrectly makes the thin limit quadratic.
/// Rayleigh p_R integrates to one, whereas the input Mie table P_M integrates
/// to 4π. Internally p_M=P_M/(4π) and w_eff=4π*JONES_MIE_WEIGHT; their product
/// preserves the inherited empirical coefficient without retuning it.
/// This approximation is exact for equal airmasses and has relative quadrature
/// error O([τ (X_moon-X_target)]²). It is not Jones's full spherical solver.
fn single_scattering_midpoint_kernel(
    tau_r: OpticalDepths,
    tau_m: OpticalDepths,
    phase_r: PhaseDensity,
    phase_m: ScaleFactors,
    am_moon: Airmasses,
    am_target: Airmasses,
) -> PhaseDensity {
    let four_pi = 4.0 * std::f64::consts::PI;
    let mie_per_sr = PhaseDensity::new(phase_m.value() / four_pi);
    let effective_mie_weight = JONES_MIE_WEIGHT * four_pi;
    // qtty 0.8's named dimensionless units do not implement cross-unit Mul.
    // Erase only these dimensionless multipliers, keeping the physical phase
    // density typed. Irradiance * PhaseDensity cancels structurally to radiance.
    let scatter = (phase_r * tau_r.value() + mie_per_sr * (tau_m.value() * effective_mie_weight))
        .max(PhaseDensity::new(0.0));
    let slant_depth =
        OpticalDepths::new((tau_r + tau_m).value() * 0.5 * (am_moon + am_target).value());
    let transmission = (-slant_depth).exp().to::<Transmittance>();
    scatter * am_target.value() * transmission.value()
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

    // Scalar inputs keep the independent quadrature reference transparent;
    // the production kernel accepts and returns qtty quantities only.
    fn scalar_kernel(tr: f64, tm: f64, pr: f64, pm: f64, xm: f64, xt: f64) -> f64 {
        single_scattering_midpoint_kernel(
            OpticalDepths::new(tr),
            OpticalDepths::new(tm),
            PhaseDensity::new(pr),
            ScaleFactors::new(pm),
            Airmasses::new(xm),
            Airmasses::new(xt),
        )
        .value()
    }

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

    #[test]
    fn rayleigh_and_mie_phase_conventions_are_explicit() {
        let four_pi = 4.0 * std::f64::consts::PI;
        let n = 18_000;
        let dtheta = std::f64::consts::PI / n as f64;
        let native_angles: Vec<f64> = include_str!("../../../data/moonlight_mie_nsb_v1.dat")
            .lines()
            .filter(|line| !line.starts_with('#') && !line.trim().is_empty())
            .nth(2)
            .unwrap()
            .split_whitespace()
            .map(|v| v.parse::<f64>().unwrap().to_radians())
            .collect();
        for wave in [350.0, 551.0, 650.0] {
            let mut rayleigh_integral = 0.0;
            let mut mie_integral = 0.0;
            for i in 0..n {
                let theta = (i as f64 + 0.5) * dtheta;
                let domega = 2.0 * std::f64::consts::PI * theta.sin() * dtheta;
                rayleigh_integral += rayleigh_phase(Radians::new(theta)).value() * domega;
                let raw_mie =
                    mie_grid().lookup(Degrees::new(theta.to_degrees()), Nanometers::new(wave));
                mie_integral += raw_mie / four_pi * domega;
                // Normalization must not silently retune the empirical model.
                let normalized_weighted = JONES_MIE_WEIGHT * four_pi * (raw_mie / four_pi);
                assert!(
                    (normalized_weighted - JONES_MIE_WEIGHT * raw_mie).abs()
                        <= 1e-14 * (1.0 + normalized_weighted.abs())
                );
            }
            assert!((rayleigh_integral - 1.0).abs() < 1e-7);
            // The admission gate applies trapezoidal quadrature to P*sin(theta)
            // at native nodes. That differs from integrating the runtime's
            // piecewise-linear P(theta) exactly: preserve both distinctions.
            let mut admitted = 0.0;
            let mut exact_interpolated = 0.0;
            for pair in native_angles.windows(2) {
                let (t0, t1) = (pair[0], pair[1]);
                let p0 = mie_grid().lookup(Degrees::new(t0.to_degrees()), Nanometers::new(wave));
                let p1 = mie_grid().lookup(Degrees::new(t1.to_degrees()), Nanometers::new(wave));
                admitted += 0.25 * (t1 - t0) * (p0 * t0.sin() + p1 * t1.sin());
                // Exact cell integral of [average + slope*(theta-mid)] sin(theta).
                let mid = (t0 + t1) / 2.0;
                let half = (t1 - t0) / 2.0;
                let slope = (p1 - p0) / (t1 - t0);
                exact_interpolated += 0.5 * (p0 + p1) * mid.sin() * half.sin()
                    + slope * mid.cos() * (half.sin() - half * half.cos());
            }
            assert!((admitted - 1.0).abs() < 1e-4);
            assert!((mie_integral / exact_interpolated - 1.0).abs() < 1e-7);
        }
    }

    #[test]
    fn single_scattering_matches_independent_column_integral() {
        for (tr, tm) in [(0.1, 0.0), (0.0, 0.05), (0.08, 0.05)] {
            for separation in [10.0_f64, 60.0, 120.0] {
                let pr = 3.0 / (16.0 * std::f64::consts::PI)
                    * (1.0 + separation.to_radians().cos().powi(2));
                for wavelength in [350.0, 551.0, 650.0] {
                    let pm =
                        mie_grid().lookup(Degrees::new(separation), Nanometers::new(wavelength));
                    for (xm, xt) in [(1.0, 1.0), (2.0, 2.0), (1.1, 2.0)] {
                        // Independent composite midpoint quadrature of the full
                        // shared-profile slab integral; no production path helper.
                        let n = 10_000;
                        let integral: f64 = (0..n)
                            .map(|i| {
                                let u = (i as f64 + 0.5) / n as f64;
                                (-(tr + tm) * (xm * (1.0 - u) + xt * u)).exp()
                            })
                            .sum::<f64>()
                            / n as f64;
                        let expected = xt * (tr * pr + 0.05 * tm * pm) * integral;
                        let actual = scalar_kernel(tr, tm, pr, pm, xm, xt);
                        let tolerance = if xm == xt { 1e-12 } else { 0.001 };
                        assert!((actual / expected - 1.0).abs() < tolerance);
                    }
                }
            }
        }
    }

    #[test]
    fn single_scattering_zero_and_first_order_limits() {
        assert_eq!(scalar_kernel(0.0, 0.0, 0.1, 0.2, 1.0, 2.0), 0.0);
        for (tr, tm) in [(0.1, 0.0), (0.0, 0.05), (0.08, 0.05)] {
            let evaluate = |scale| scalar_kernel(scale * tr, scale * tm, 0.1, 0.2, 1.5, 2.0);
            let expected_first_order = 2.0 * (tr * 0.1 + 0.05 * tm * 0.2);
            assert!((evaluate(1e-10) / 1e-10 / expected_first_order - 1.0).abs() < 1e-10);
            let slope = (evaluate(1e-7) / evaluate(1e-6)).ln() / 0.1_f64.ln();
            assert!((slope - 1.0).abs() < 1e-6);
            assert!(evaluate(0.02) > evaluate(0.01));
            assert!(evaluate(1e-300).is_finite() && evaluate(1e-300) > 0.0);
        }
    }

    #[test]
    fn tabulated_multiple_scattering_preserves_the_measured_thin_exponent() {
        // Fixed-atmosphere LUT is a finite multiplier, not another scattering
        // probability. It does not become unity for arbitrary scaled atmospheres;
        // the absolute thin coefficient of the LUT is outside its admitted domain.
        for wave in [350.0, 551.0, 650.0] {
            let f = correction_grid().lookup(Degrees::new(60.0), Nanometers::new(wave));
            assert!(f.is_finite() && f >= 1.0);
            let a = scalar_kernel(1e-9, 0.0, 0.1, 0.0, 1.0, 1.0);
            let b = scalar_kernel(1e-10, 0.0, 0.1, 0.0, 1.0, 1.0);
            assert!(((b * f) / (a * f) - 0.1).abs() < 1e-9);
        }
    }

    #[test]
    fn scattered_moonlight_retains_inverse_square_source_distance() {
        let evaluate = |distance| {
            compute_jones_2013_spectral(
                &geometry(60.0, 50.0, 30.0, 45.0, distance),
                bundled_solar_spectrum(),
                ScaleFactors::new(1.0),
                paranal_like_profile(),
            )
            .unwrap()
        };
        let reference = evaluate(384_400.0);
        let nearer = evaluate(350_000.0);
        let expected = (Kilometers::new(384_400.0) / Kilometers::new(350_000.0)).powi(2);
        for (close, far) in [
            (nearer.integrated.value(), reference.integrated.value()),
            (nearer.b_flux_s10.value(), reference.b_flux_s10.value()),
            (nearer.v_flux_s10.value(), reference.v_flux_s10.value()),
        ] {
            assert!((close / far / expected - 1.0).abs() < 1e-12);
        }
    }

    #[test]
    fn local_pressure_column_is_independent_of_redundant_altitude() {
        let inp = geometry(60.0, 50.0, 30.0, 45.0, 384_400.0);
        let profile = paranal_like_profile();
        let mut same_column = profile;
        same_column.observer_altitude = Kilometers::new(0.0);
        for scale in [1.0, 0.01] {
            let evaluate = |p| {
                compute_jones_2013_spectral(
                    &inp,
                    bundled_solar_spectrum(),
                    ScaleFactors::new(scale),
                    p,
                )
                .unwrap()
            };
            let a = evaluate(profile);
            let b = evaluate(same_column);
            assert_eq!(a.integrated, b.integrated);
            assert_eq!(a.v_flux_s10, b.v_flux_s10);
        }
    }

    /// Regression pins for the bundled analytic Planck solar reference.
    ///
    /// The expected tuples pin the independently generated NSB Mie phase grid
    /// with the current analytic solar reference.
    #[test]
    fn analytic_solar_reference_matches_moonlight_regression() {
        const REL_TOL: f64 = 1.0e-9;
        let cases = [
            (
                85.5,
                97.523,
                36.0,
                60.0,
                384_400.0,
                (0.48493586510203146, 540.0488857780191, 270.8486175836127),
            ),
            (
                85.5,
                4.0,
                36.0,
                40.0,
                384_400.0,
                (1.3357191157875907, 1512.4224354010262, 1159.7436633500593),
            ),
            (
                85.5,
                52.216,
                62.0,
                15.0,
                384_400.0,
                (0.3624570994319541, 426.66239109431285, 255.45461658939527),
            ),
        ];
        let profile = paranal_like_profile();
        let mut worst: f64 = 0.0;
        for (phase, sep, z_moon, z_src, dist, expected) in cases {
            let out = compute_jones_2013_spectral(
                &geometry(phase, sep, z_moon, z_src, dist),
                bundled_solar_spectrum(),
                crate::units::ScaleFactors::new(1.0),
                profile,
            )
            .expect("spectral evaluate");
            let actuals = (
                out.integrated.value(),
                out.b_flux_s10.value(),
                out.v_flux_s10.value(),
            );
            for (actual, expected) in [
                (actuals.0, expected.0),
                (actuals.1, expected.1),
                (actuals.2, expected.2),
            ] {
                let rel = (actual - expected).abs() / expected.max(1.0e-12);
                worst = worst.max(rel);
                assert!(actual > 0.0);
            }
        }
        assert!(worst <= REL_TOL, "maximum snapshot change {worst}");
    }
}
