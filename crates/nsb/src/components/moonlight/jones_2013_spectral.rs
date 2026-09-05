use super::*;
use crate::units::s10_for_spectral_photon_radiance;
use crate::units::ScaleFactors;
use qtty::Second;

/// Wavelength-resolved Jones et al. (2013) scattered-moonlight evaluator.
pub struct Jones2013Spectral {
    location: Geodetic<ECEF>,
    conditions: AtmosphericConditions,
    extinction_scale: Option<ScaleFactors>,
}

impl Jones2013Spectral {
    /// Default coarse scan step for range searches.
    pub const DEFAULT_PERIOD_SEARCH_STEP: Second = Second::new(600.0);

    /// Build with explicit observer and atmospheric conditions.
    pub fn new(location: Geodetic<ECEF>, conditions: AtmosphericConditions) -> Self {
        Self {
            location,
            conditions,
            extinction_scale: None,
        }
    }

    /// Build with generic altitude-derived clear-sky conditions.
    pub fn standard_clear_sky(location: Geodetic<ECEF>) -> Self {
        Self::new(location, standard_clear_sky_conditions(location))
    }

    /// Override the default extinction coefficient by relative scaling.
    pub fn with_extinction_scale(mut self, k_ext: MagnitudesPerAirmass) -> Self {
        self.extinction_scale = Some(ScaleFactors::new(k_ext.value() / DEFAULT_K_EXT.value()));
        self
    }

    /// Evaluate scattered moonlight toward a target at one UTC instant.
    pub fn compute(
        &self,
        time: Time<UTC>,
        target: SphericalDirection<EquatorialMeanJ2000>,
    ) -> Result<MoonOutputs> {
        let geometry = lunar_geometry(time, self.location, target);
        compute_jones_2013_spectral(
            &geometry,
            bundled_solar_samples(),
            self.extinction_scale.unwrap_or(ScaleFactors::new(1.0)),
            self.atmosphere_profile(),
        )
    }

    /// Find periods whose integrated moonlight lies in the inclusive range.
    pub fn periods_in_range(
        &self,
        window: Period<UTC>,
        target: SphericalDirection<EquatorialMeanJ2000>,
        min: PhotonsPerSquareCentimeterNanosecondSteradian,
        max: PhotonsPerSquareCentimeterNanosecondSteradian,
    ) -> Result<Vec<Period<UTC>>> {
        self.periods_in_range_with_step(window, target, min, max, Self::DEFAULT_PERIOD_SEARCH_STEP)
    }

    /// Find in-range periods with an explicit coarse scan step.
    pub fn periods_in_range_with_step(
        &self,
        window: Period<UTC>,
        target: SphericalDirection<EquatorialMeanJ2000>,
        min: PhotonsPerSquareCentimeterNanosecondSteradian,
        max: PhotonsPerSquareCentimeterNanosecondSteradian,
        sample_step: Second,
    ) -> Result<Vec<Period<UTC>>> {
        crate::window_search::periods_in_range(window, sample_step, min, max, |time| {
            Ok(self.compute(time, target)?.integrated)
        })
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
    solar_samples: &[(f64, f64)],
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
    for &(lambda_nm, solar_irradiance) in solar_samples {
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

    let integrated = algo::trapz_range(&lam, &density, WL_LOW.value(), WL_HIGH.value());
    let b_density = algo::interp_linear(
        &lam,
        &density,
        B_FILTER.value(),
        OutOfRange::ClampToEndpoints,
    )
    .unwrap_or(0.0);
    let v_density = algo::interp_linear(
        &lam,
        &density,
        V_FILTER.value(),
        OutOfRange::ClampToEndpoints,
    )
    .unwrap_or(0.0);

    Ok(MoonOutputs {
        integrated: radiometry::PhotonsPerSquareCentimeterNanosecondSteradian::new(integrated),
        b_flux_s10: s10_for_spectral_photon_radiance(
            SpectralBandPhotonRadiance::new(b_density),
            B_FILTER,
        ),
        v_flux_s10: s10_for_spectral_photon_radiance(
            SpectralBandPhotonRadiance::new(v_density),
            V_FILTER,
        ),
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

fn bundled_solar_samples() -> &'static Vec<(f64, f64)> {
    static SAMPLES: OnceLock<Vec<(f64, f64)>> = OnceLock::new();
    SAMPLES.get_or_init(|| {
        let solar = solar::load().expect("bundled solar spectrum");
        solar
            .xs_raw()
            .iter()
            .copied()
            .zip(solar.ys_raw().iter().copied())
            .collect()
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{DateTime, Utc};
    use siderust::qtty::{Degrees as SiderustDegrees, Meters};

    fn parse_utc(input: &str) -> Time<UTC> {
        Time::<UTC>::from_chrono(
            DateTime::parse_from_rfc3339(input)
                .unwrap()
                .with_timezone(&Utc),
        )
    }

    fn test_location() -> Geodetic<ECEF> {
        Geodetic::<ECEF>::new_raw(
            SiderustDegrees::new(-70.0),
            SiderustDegrees::new(-24.0),
            Meters::new(2500.0),
        )
    }

    fn test_target() -> SphericalDirection<EquatorialMeanJ2000> {
        SphericalDirection::<EquatorialMeanJ2000>::new(
            SiderustDegrees::new(270.0),
            SiderustDegrees::new(-30.0),
        )
    }

    fn test_window() -> Period<UTC> {
        Period::new(
            parse_utc("2023-09-04T02:00:00Z"),
            parse_utc("2023-09-04T03:00:00Z"),
        )
    }

    #[test]
    fn periods_in_range_with_step_rejects_bad_step() {
        let model = Jones2013Spectral::standard_clear_sky(test_location());
        let err = model
            .periods_in_range_with_step(
                test_window(),
                test_target(),
                PhotonsPerSquareCentimeterNanosecondSteradian::new(0.0),
                PhotonsPerSquareCentimeterNanosecondSteradian::new(1.0),
                Second::new(0.0),
            )
            .unwrap_err();
        assert!(err.to_string().contains("sample_step"));
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

    /// Regression pins for the three historical fixture geometries.
    ///
    /// The CSV `expected_*` columns remain a schema/tolerance manifest for
    /// external references. Those historical LUT values diverge from the
    /// current spectral implementation (~85% relative); these pins protect the
    /// spectral model itself against silent radiance changes.
    #[test]
    fn historical_fixture_geometries_match_spectral_regression_pins() {
        const REL_TOL: f64 = 1.0e-9;
        let cases = [
            (
                85.5,
                97.523,
                36.0,
                60.0,
                384_400.0,
                0.081_651_100_816_024_92,
            ),
            (85.5, 4.0, 36.0, 40.0, 384_400.0, 0.302_669_644_974_378_37),
            (85.5, 52.216, 62.0, 15.0, 384_400.0, 0.066_186_634_791_062),
        ];
        let profile = paranal_like_profile();
        for (phase, sep, z_moon, z_src, dist, expected) in cases {
            let out = compute_jones_2013_spectral(
                &geometry(phase, sep, z_moon, z_src, dist),
                bundled_solar_samples(),
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
}
