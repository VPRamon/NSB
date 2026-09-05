use super::*;
use crate::reference::solar::SolarSpectrum;
use crate::units::s10_for_spectral_photon_radiance;
use crate::units::ScaleFactors;
use optica::spectrum::{Interpolation, SampledSpectrum};
use qtty::length::Nanometer;
use qtty::radiometry::{
    PhotonPerSquareCentimeterNanosecondSteradian as BandPhotonRadianceUnit,
    PhotonPerSquareCentimeterNanosecondSteradianNanometer as SpectralBandPhotonRadianceUnit,
};
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
            bundled_solar_spectrum(),
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
    fn spectral_moonlight_computes_non_negative_result() {
        let model = Jones2013Spectral::standard_clear_sky(test_location());
        let out = model
            .compute(parse_utc("2023-09-04T02:00:00Z"), test_target())
            .unwrap();
        assert!(out.integrated.value() >= 0.0);
        assert!(out.b_flux_s10.value() >= 0.0);
        assert!(out.v_flux_s10.value() >= 0.0);
    }

    #[test]
    fn periods_in_range_covers_window_for_large_bound() {
        let model = Jones2013Spectral::standard_clear_sky(test_location());
        let periods = model
            .periods_in_range(
                test_window(),
                test_target(),
                PhotonsPerSquareCentimeterNanosecondSteradian::new(0.0),
                PhotonsPerSquareCentimeterNanosecondSteradian::new(1.0e9),
            )
            .unwrap();
        assert_eq!(periods, vec![test_window()]);
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
}
