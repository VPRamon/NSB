use super::*;

#[cfg(test)]
use super::krisciunas_schaefer1991::compute_krisciunas_schaefer_1991;

pub struct Jones2013Spectral {
    location: Geodetic<ECEF>,
    conditions: AtmosphericConditions,
    extinction_scale: Option<f64>,
}

impl Jones2013Spectral {
    pub fn new(location: Geodetic<ECEF>, conditions: AtmosphericConditions) -> Self {
        Self {
            location,
            conditions,
            extinction_scale: None,
        }
    }

    pub fn standard_clear_sky(location: Geodetic<ECEF>) -> Self {
        Self::new(location, standard_clear_sky_conditions(location))
    }

    pub fn with_extinction_scale(mut self, k_ext: f64) -> Self {
        self.extinction_scale = Some(k_ext / DEFAULT_K_EXT);
        self
    }

    pub fn compute(
        &self,
        time: Time<UTC>,
        target: SphericalDirection<EquatorialMeanJ2000>,
    ) -> Result<MoonOutputs> {
        let geometry = lunar_geometry(time, self.location, target);
        compute_jones_2013_spectral(
            &geometry,
            bundled_solar_samples(),
            self.extinction_scale.unwrap_or(1.0),
            self.atmosphere_profile(),
        )
    }

    pub fn periods_in_range(
        &self,
        _window: Period<UTC>,
        _target: SphericalDirection<EquatorialMeanJ2000>,
        _min: PhotonsPerSquareCentimeterNanosecondSteradian,
        _max: PhotonsPerSquareCentimeterNanosecondSteradian,
    ) -> Result<Vec<Period<UTC>>> {
        unimplemented!("moonlight-only period search is not implemented yet")
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
    tau_scale: f64,
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

    let mut lam = Vec::new();
    let mut density = Vec::new();
    for &(lambda_nm, solar_irradiance) in solar_samples {
        if !(WL_LOW_NM..=WL_HIGH_NM).contains(&lambda_nm) {
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
        )
        .value();
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
        let value = lunar_ph * scatter * transmission * source_path.max(0.0) * multi;
        if value.is_finite() && value > 0.0 {
            lam.push(lambda_nm);
            density.push(value);
        }
    }

    if lam.len() < 2 {
        return Ok(zero_outputs());
    }

    let integrated = algo::trapz_range(&lam, &density, WL_LOW_NM, WL_HIGH_NM);
    let b_density = algo::interp_linear(&lam, &density, B_FILTER_NM, OutOfRange::ClampToEndpoints)
        .unwrap_or(0.0);
    let v_density = algo::interp_linear(&lam, &density, V_FILTER_NM, OutOfRange::ClampToEndpoints)
        .unwrap_or(0.0);

    Ok(MoonOutputs {
        integrated: radiometry::PhotonsPerSquareCentimeterNanosecondSteradian::new(integrated),
        b_flux_s10: spectral_photon_density_to_s10(b_density, Nanometers::new(B_FILTER_NM)),
        v_flux_s10: spectral_photon_density_to_s10(v_density, Nanometers::new(V_FILTER_NM)),
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
fn compute_jones2013(inp: &MoonlightGeometry) -> Result<MoonOutputs> {
    compute_jones2013_with_profile(inp, AtmosphereProfile::EL_PARANAL, DEFAULT_K_EXT)
}

#[cfg(test)]
fn compute_jones2013_with_profile(
    inp: &MoonlightGeometry,
    profile: AtmosphereProfile,
    k_ext: f64,
) -> Result<MoonOutputs> {
    let samples = reference_solar_samples();
    compute_jones_2013_spectral(inp, &samples, k_ext / DEFAULT_K_EXT, profile)
}

#[cfg(test)]
fn reference_solar_samples() -> Vec<(f64, f64)> {
    let mut out = Vec::new();
    let mut lambda = WL_LOW_NM;
    while lambda <= WL_HIGH_NM {
        let x = (lambda - 550.0) / 180.0;
        let irradiance = 1.88 * (-0.5 * x * x).exp() + 0.25;
        out.push((lambda, irradiance));
        lambda += 10.0;
    }
    out
}

fn spectral_photon_density_to_s10(density: f64, wavelength: Nanometers) -> radiometry::S10s {
    let lambda_m = wavelength.value() * 1.0e-9;
    let photon_energy = HC_JOULE_METER / lambda_m;
    let w_m2_sr_nm = density * 1.0e13 * photon_energy;
    let w_m2_sr_um = w_m2_sr_nm * 1.0e3;
    radiometry::S10s::new(w_m2_sr_um / S10_TO_W_M2_SR_UM)
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{DateTime, Duration, Utc};
    use qtty::angular::Radians;
    use qtty::photometry::s10_to_surface_brightness;
    use qtty::radiometry::S10s;
    use siderust::qtty::{
        Degrees as SiderustDegrees, Hectopascals as SiderustHectopascals, IlluminationFractions,
        Meters,
    };

    const LUT_MOON_PHASE_0454: &str =
        include_str!("../../../data/lut_moon/Phase_0.454_waxing_moon_LUT.csv");

    fn make_phase(alpha_deg: f64) -> MoonPhaseGeometry {
        MoonPhaseGeometry {
            phase_angle: Radians::new(alpha_deg.to_radians()),
            illuminated_fraction: IlluminationFractions::new(
                0.5 * (1.0 + alpha_deg.to_radians().cos()),
            ),
            elongation: Radians::new(0.0),
            waxing: true,
        }
    }

    fn phase_from_illumination_fraction(fraction: f64) -> MoonPhaseGeometry {
        let phase_angle = (2.0 * fraction - 1.0).clamp(-1.0, 1.0).acos();
        MoonPhaseGeometry {
            phase_angle: Radians::new(phase_angle),
            illuminated_fraction: IlluminationFractions::new(fraction),
            elongation: Radians::new(0.0),
            waxing: true,
        }
    }

    fn geometry(alpha_deg: f64) -> MoonlightGeometry {
        MoonlightGeometry {
            separation: Degrees::new(60.0),
            moon_zenith: Degrees::new(30.0),
            phase: make_phase(alpha_deg),
            source_zenith: Degrees::new(35.0),
            moon_distance: Kilometers::new(384_400.0),
        }
    }

    fn parse_utc(input: &str) -> Time<UTC> {
        Time::<UTC>::from_chrono(
            DateTime::parse_from_rfc3339(input)
                .unwrap()
                .with_timezone(&Utc),
        )
    }

    fn profile_from_conditions(
        location: Geodetic<ECEF>,
        conditions: AtmosphericConditions,
    ) -> AtmosphereProfile {
        AtmosphereProfile {
            surface_pressure: conditions.surface_pressure,
            observer_altitude: location.height.to::<Kilometer>(),
            rayleigh_scale_height: conditions.rayleigh_scale_height,
            mie_params: conditions.mie_params,
        }
    }

    #[test]
    fn ks_zero_when_moon_below_horizon() {
        let mut g = geometry(40.0);
        g.moon_zenith = Degrees::new(95.0);
        let out = compute_krisciunas_schaefer_1991(&g, DEFAULT_K_EXT).unwrap();
        assert_eq!(out.integrated.value(), 0.0);
    }

    #[test]
    fn ks_phase_dependence_full_brighter_than_crescent() {
        let full = compute_krisciunas_schaefer_1991(&geometry(5.0), DEFAULT_K_EXT).unwrap();
        let cres = compute_krisciunas_schaefer_1991(&geometry(130.0), DEFAULT_K_EXT).unwrap();
        assert!(full.integrated.value() > cres.integrated.value());
    }

    #[test]
    fn ks_separation_decreases_with_distance() {
        let near = compute_krisciunas_schaefer_1991(&geometry(60.0), DEFAULT_K_EXT).unwrap();
        let mut far_g = geometry(60.0);
        far_g.separation = Degrees::new(120.0);
        let far = compute_krisciunas_schaefer_1991(&far_g, DEFAULT_K_EXT).unwrap();
        assert!(near.integrated.value() > far.integrated.value());
    }

    #[test]
    fn ks_default_extinction_matches_legacy_curve_scale() {
        let out = compute_krisciunas_schaefer_1991(&geometry(45.0), DEFAULT_K_EXT).unwrap();
        assert!(out.integrated.value() > 0.0);
        assert_eq!(out.b_flux_s10.value(), out.v_flux_s10.value());
    }

    #[test]
    fn jones_spectral_positive_for_good_geometry() {
        let out = compute_jones2013(&geometry(50.0)).unwrap();
        assert!(out.integrated.value() > 0.0);
        assert!(out.b_flux_s10.value() > 0.0);
        assert!(out.v_flux_s10.value() > 0.0);
    }

    #[test]
    fn jones_spectral_zero_when_source_below_horizon() {
        let mut g = geometry(40.0);
        g.source_zenith = Degrees::new(92.0);
        let out = compute_jones2013(&g).unwrap();
        assert_eq!(out.integrated.value(), 0.0);
    }

    #[test]
    fn jones_profile_changes_result() {
        let g = geometry(50.0);
        let paranal = compute_jones2013_with_profile(&g, AtmosphereProfile::EL_PARANAL, DEFAULT_K_EXT)
            .unwrap();
        let sea_level = compute_jones2013_with_profile(
            &g,
            AtmosphereProfile {
                surface_pressure: SiderustHectopascals::new(1013.25),
                observer_altitude: Kilometers::new(0.0),
                rayleigh_scale_height: DEFAULT_SCALE_HEIGHT,
                mie_params: MieParams::PARANAL,
            },
            DEFAULT_K_EXT,
        )
        .unwrap();
        assert_ne!(paranal.integrated.value(), sea_level.integrated.value());
    }

    #[test]
    fn cta_n_profile_changes_jones_result_against_generic_clear_sky() {
        let location = Geodetic::<ECEF>::new_raw(
            SiderustDegrees::new(-17.892),
            SiderustDegrees::new(28.762),
            Meters::new(2_200.0),
        );
        let g = geometry(50.0);
        let generic = compute_jones2013_with_profile(
            &g,
            profile_from_conditions(location, AtmosphericConditions::generic_clear_sky(location)),
            DEFAULT_K_EXT,
        )
        .unwrap();
        let cta_n = compute_jones2013_with_profile(
            &g,
            profile_from_conditions(location, SiteProfileId::CtaNorth.profile(location).atmosphere),
            DEFAULT_K_EXT,
        )
        .unwrap();

        assert_ne!(generic.integrated.value(), cta_n.integrated.value());
    }

    #[test]
    fn jones_standard_clear_sky_changes_with_location_altitude() {
        let target = SphericalDirection::<EquatorialMeanJ2000>::new(
            SiderustDegrees::new(270.0),
            SiderustDegrees::new(-30.0),
        );
        let time = parse_utc("2023-09-04T02:00:00Z");
        let low = Geodetic::<ECEF>::new_raw(
            SiderustDegrees::new(-70.0),
            SiderustDegrees::new(-24.0),
            Meters::new(0.0),
        );
        let high = Geodetic::<ECEF>::new_raw(
            SiderustDegrees::new(-70.0),
            SiderustDegrees::new(-24.0),
            Meters::new(2500.0),
        );
        let low_out = Jones2013Spectral::standard_clear_sky(low)
            .compute(time, target)
            .unwrap();
        let high_out = Jones2013Spectral::standard_clear_sky(high)
            .compute(time, target)
            .unwrap();
        assert_ne!(low_out.integrated.value(), high_out.integrated.value());
    }

    #[test]
    fn jones_conditions_have_no_altitude_field() {
        let conditions = AtmosphericConditions {
            surface_pressure: SiderustHectopascals::new(780.0),
            rayleigh_scale_height: DEFAULT_SCALE_HEIGHT,
            mie_params: MieParams::PARANAL,
        };
        assert_eq!(conditions.surface_pressure.value(), 780.0);
    }

    #[test]
    fn jones_lut_reference_contains_expected_columns() {
        let first_data = LUT_MOON_PHASE_0454
            .lines()
            .find(|l| !l.trim().is_empty() && !l.trim_start().starts_with('#'))
            .unwrap();
        let cols: Vec<_> = first_data.split(',').collect();
        assert!(cols.len() >= 4);
    }

    #[test]
    fn moonlight_output_converts_to_surface_brightness() {
        let out = compute_krisciunas_schaefer_1991(&geometry(45.0), DEFAULT_K_EXT).unwrap();
        let mag = s10_to_surface_brightness(out.v_flux_s10.max(S10s::new(1e-9)), NSB_S10_ZP);
        assert!(mag.value().is_finite());
    }

    #[test]
    fn site_bound_jones_api_computes_from_time_and_target() {
        let location = Geodetic::<ECEF>::new_raw(
            SiderustDegrees::new(-70.0),
            SiderustDegrees::new(-24.0),
            Meters::new(2500.0),
        );
        let model = Jones2013Spectral::standard_clear_sky(location);
        let target = SphericalDirection::<EquatorialMeanJ2000>::new(
            SiderustDegrees::new(270.0),
            SiderustDegrees::new(-30.0),
        );
        let time = parse_utc("2023-09-04T02:00:00Z");
        let out = model.compute(time, target).unwrap();
        assert!(out.integrated.value() >= 0.0);
    }

    #[test]
    fn standard_clear_sky_is_not_paranal_altitude_for_arbitrary_location() {
        let location = Geodetic::<ECEF>::new_raw(
            SiderustDegrees::new(0.0),
            SiderustDegrees::new(0.0),
            Meters::new(0.0),
        );
        let model = Jones2013Spectral::standard_clear_sky(location);
        let profile = model.atmosphere_profile();
        assert_eq!(profile.observer_altitude.value(), 0.0);
    }

    #[test]
    fn spectral_moonlight_with_extinction_scale_changes_result() {
        let g = geometry(40.0);
        let base = compute_jones2013_with_profile(&g, AtmosphereProfile::EL_PARANAL, DEFAULT_K_EXT)
            .unwrap();
        let scaled = compute_jones2013_with_profile(&g, AtmosphereProfile::EL_PARANAL, DEFAULT_K_EXT * 1.2)
            .unwrap();
        assert_ne!(base.integrated.value(), scaled.integrated.value());
    }
}
