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

    pub fn from_site(site: Site) -> Self {
        let profile = match site {
            Site::Paranal => AtmosphereProfile::EL_PARANAL,
            Site::LaPalma => AtmosphereProfile::ROQUE_DE_LOS_MUCHACHOS,
        };
        Self::new(
            site.geodetic(),
            AtmosphericConditions::from_profile_without_altitude(profile),
        )
        .with_extinction_scale(DEFAULT_K_EXT)
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

    fn inputs(alpha_deg: f64, rho_deg: f64, z_moon: f64, z_src: f64) -> MoonlightGeometry {
        MoonlightGeometry {
            separation: Degrees::new(rho_deg),
            moon_zenith: Degrees::new(z_moon),
            phase: make_phase(alpha_deg),
            source_zenith: Degrees::new(z_src),
            moon_distance: siderust::event::lunar::photometry::MEAN_MOON_DISTANCE,
        }
    }

    fn horizontal_separation_deg(
        moon_alt_deg: f64,
        moon_az_deg: f64,
        source_alt_deg: f64,
        source_az_deg: f64,
    ) -> f64 {
        let moon_alt = moon_alt_deg.to_radians();
        let source_alt = source_alt_deg.to_radians();
        let delta_az = (source_az_deg - moon_az_deg).to_radians();
        let cos_sep =
            moon_alt.sin() * source_alt.sin() + moon_alt.cos() * source_alt.cos() * delta_az.cos();
        cos_sep.clamp(-1.0, 1.0).acos().to_degrees()
    }

    fn lut_inputs(line: &str) -> (MoonlightGeometry, f64) {
        let values: Vec<f64> = line
            .split(',')
            .map(|field| field.trim().parse::<f64>().expect("numeric LUT field"))
            .collect();
        assert_eq!(values.len(), 6);
        let separation = horizontal_separation_deg(values[1], values[0], values[3], values[4]);
        (
            MoonlightGeometry {
                separation: Degrees::new(separation),
                moon_zenith: Degrees::new(90.0 - values[1]),
                phase: phase_from_illumination_fraction(values[2]),
                source_zenith: Degrees::new(90.0 - values[3]),
                moon_distance: siderust::event::lunar::photometry::MEAN_MOON_DISTANCE,
            },
            values[5],
        )
    }

    fn v_mag_arcsec2(out: &MoonOutputs) -> f64 {
        s10_to_surface_brightness(out.v_flux_s10, NSB_S10_ZP).value()
    }

    fn parse_time(input: &str) -> Time<UTC> {
        let dt = DateTime::parse_from_rfc3339(input)
            .expect("RFC3339 time")
            .with_timezone(&Utc);
        Time::<UTC>::from_chrono(dt)
    }

    fn target(ra_deg: f64, dec_deg: f64) -> SphericalDirection<EquatorialMeanJ2000> {
        SphericalDirection::<EquatorialMeanJ2000>::new(Degrees::new(ra_deg), Degrees::new(dec_deg))
    }

    fn scan_time<F>(
        location: Geodetic<ECEF>,
        target: SphericalDirection<EquatorialMeanJ2000>,
        predicate: F,
    ) -> Time<UTC>
    where
        F: Fn(MoonlightGeometry) -> bool,
    {
        let start = DateTime::parse_from_rfc3339("2023-09-01T00:00:00Z")
            .expect("start time")
            .with_timezone(&Utc);
        for hour in 0..(24 * 45) {
            let time = Time::<UTC>::from_chrono(start + Duration::hours(hour));
            let geometry = lunar_geometry(time, location, target);
            if predicate(geometry) {
                return time;
            }
        }
        panic!("test geometry not found");
    }

    fn nonzero_time(
        location: Geodetic<ECEF>,
        target: SphericalDirection<EquatorialMeanJ2000>,
    ) -> Time<UTC> {
        scan_time(location, target, |geometry| {
            geometry.moon_zenith < Degrees::new(80.0)
                && geometry.source_zenith < Degrees::new(80.0)
                && geometry.separation > Degrees::new(5.0)
        })
    }

    #[test]
    fn jones2013_full_moon_high_altitude() {
        let out_jones = compute_jones2013(&inputs(0.0, 90.0, 20.0, 45.0)).unwrap();
        assert!(out_jones.v_flux_s10 > S10s::zero());
        assert!(out_jones.integrated.value().is_finite());
        assert!(v_mag_arcsec2(&out_jones).is_finite());
    }

    #[test]
    fn jones2013_twilight_conditions() {
        let out = compute_jones2013(&inputs(45.0, 60.0, 50.0, 30.0)).unwrap();
        assert!(out.v_flux_s10 > S10s::zero());
        assert!(out.integrated.value() > 0.0);
        assert!(out.b_flux_s10 > S10s::zero());
    }

    #[test]
    fn jones2013_new_moon_negligible() {
        let out_new = compute_jones2013(&inputs(180.0, 90.0, 45.0, 45.0)).unwrap();
        let out_full = compute_jones2013(&inputs(0.0, 90.0, 45.0, 45.0)).unwrap();
        assert!(out_full.v_flux_s10 > S10s::zero());
        assert!(out_new.v_flux_s10 < out_full.v_flux_s10 * 1e-3);
    }

    #[test]
    fn jones2013_moon_below_horizon_returns_zero() {
        let out = compute_jones2013(&inputs(0.0, 30.0, 95.0, 30.0)).unwrap();
        assert_eq!(out.v_flux_s10, S10s::zero());
        assert_eq!(out.integrated.value(), 0.0);
    }

    #[test]
    fn jones2013_source_below_horizon_returns_zero() {
        let out = compute_jones2013(&inputs(0.0, 30.0, 30.0, 95.0)).unwrap();
        assert_eq!(out.v_flux_s10, S10s::zero());
        assert_eq!(out.integrated.value(), 0.0);
    }

    #[test]
    fn jones2013_vs_ks_comparison() {
        let inp = inputs(60.0, 75.0, 40.0, 50.0);
        let out_ks = compute_krisciunas_schaefer_1991(&inp, DEFAULT_K_EXT).unwrap();
        let out_jones = compute_jones2013(&inp).unwrap();
        let ratio = out_jones.v_flux_s10.value() / out_ks.v_flux_s10.value();
        assert!(ratio.is_finite() && ratio > 0.0);
        assert!((ratio - 1.0).abs() > 1.0e-6);
    }

    #[test]
    fn jones2013_atmosphere_profile_sensitivity() {
        let inp = inputs(0.0, 90.0, 30.0, 45.0);
        let paranal = AtmosphereProfile::EL_PARANAL;
        let sea_level = AtmosphereProfile {
            surface_pressure: SiderustHectopascals::new(1013.25),
            observer_altitude: Kilometers::new(0.0),
            ..paranal
        };

        let out_paranal = compute_jones2013_with_profile(&inp, paranal, DEFAULT_K_EXT).unwrap();
        let out_sea = compute_jones2013_with_profile(&inp, sea_level, DEFAULT_K_EXT).unwrap();

        assert!(out_paranal.integrated.value() > 0.0);
        assert!(out_sea.integrated.value() > 0.0);
        assert_ne!(out_paranal.integrated.value(), out_sea.integrated.value());
    }

    #[test]
    fn jones2013_lut_moon_fixture_same_scale() {
        let fixture_line = LUT_MOON_PHASE_0454
            .lines()
            .nth(1)
            .expect("first LUT data row");
        let (inp, expected) = lut_inputs(fixture_line);
        let out = compute_jones2013(&inp).unwrap();
        let ratio = out.integrated.value() / expected;
        assert!(ratio.is_finite() && ratio > 0.0);
        assert!((0.05..=20.0).contains(&ratio));
    }

    #[test]
    fn standard_clear_sky_does_not_use_paranal_altitude() {
        let location = Geodetic::new_raw(
            SiderustDegrees::new(-70.0),
            SiderustDegrees::new(-25.0),
            Meters::new(123.0),
        );
        let model = Jones2013Spectral::standard_clear_sky(location);
        let profile = model.atmosphere_profile();

        assert_eq!(profile.observer_altitude, location.height.to::<Kilometer>());
        assert_ne!(
            profile.observer_altitude,
            AtmosphereProfile::EL_PARANAL.observer_altitude
        );
    }

    #[test]
    fn atmospheric_conditions_do_not_store_altitude() {
        let conditions =
            AtmosphericConditions::from_profile_without_altitude(AtmosphereProfile::EL_PARANAL);
        let AtmosphericConditions {
            surface_pressure,
            rayleigh_scale_height,
            mie_params,
        } = conditions;

        assert_eq!(
            surface_pressure,
            AtmosphereProfile::EL_PARANAL.surface_pressure
        );
        assert_eq!(
            rayleigh_scale_height,
            AtmosphereProfile::EL_PARANAL.rayleigh_scale_height
        );
        assert_eq!(mie_params, AtmosphereProfile::EL_PARANAL.mie_params);
    }

    #[test]
    fn jones2013_site_paranal_matches_previous_explicit_paranal_behavior() {
        let location = Site::Paranal.geodetic();
        let target = target(266.41683, -29.00781);
        let time = nonzero_time(location, target);
        let geometry = lunar_geometry(time, location, target);

        let new = Jones2013Spectral::from_site(Site::Paranal)
            .compute(time, target)
            .unwrap();
        let old = compute_jones_2013_spectral(
            &geometry,
            bundled_solar_samples(),
            1.0,
            AtmosphereProfile::EL_PARANAL,
        )
        .unwrap();

        let diff = (new.integrated.value() - old.integrated.value()).abs();
        assert!(diff <= old.integrated.value().abs() * 1e-12);
    }

    #[test]
    fn jones2013_changes_with_conditions() {
        let location = Site::Paranal.geodetic();
        let target = target(266.41683, -29.00781);
        let time = nonzero_time(location, target);
        let base =
            AtmosphericConditions::from_profile_without_altitude(AtmosphereProfile::EL_PARANAL);
        let clearer = AtmosphericConditions {
            surface_pressure: Hectopascals::new(base.surface_pressure.value() * 0.8),
            mie_params: MieParams {
                tau0: OpticalDepths::new(base.mie_params.tau0.value() * 0.5),
                ..base.mie_params
            },
            ..base
        };

        let out_base = Jones2013Spectral::new(location, base)
            .with_extinction_scale(DEFAULT_K_EXT)
            .compute(time, target)
            .unwrap();
        let out_clearer = Jones2013Spectral::new(location, clearer)
            .with_extinction_scale(DEFAULT_K_EXT)
            .compute(time, target)
            .unwrap();

        assert!(out_base.integrated.value() > 0.0);
        assert_ne!(out_base.integrated.value(), out_clearer.integrated.value());
    }

    #[test]
    fn jones2013_changes_with_location_altitude_under_standard_clear_sky() {
        let low = Geodetic::new_raw(
            SiderustDegrees::new(-70.0),
            SiderustDegrees::new(-25.0),
            Meters::new(0.0),
        );
        let high = Geodetic::new_raw(
            SiderustDegrees::new(-70.0),
            SiderustDegrees::new(-25.0),
            Meters::new(4000.0),
        );
        let target = target(266.41683, -29.00781);
        let time = nonzero_time(low, target);

        let low_out = Jones2013Spectral::standard_clear_sky(low)
            .compute(time, target)
            .unwrap();
        let high_out = Jones2013Spectral::standard_clear_sky(high)
            .compute(time, target)
            .unwrap();

        assert!(low_out.integrated.value() > 0.0);
        assert!(high_out.integrated.value() > 0.0);
        assert_ne!(low_out.integrated.value(), high_out.integrated.value());
    }

    #[test]
    fn ks1991_standard_clear_sky_matches_default_extinction() {
        let model = KrisciunasSchaefer1991::standard_clear_sky(Site::Paranal.geodetic());
        assert_eq!(model.k_ext(), DEFAULT_K_EXT);
    }

    #[test]
    fn moon_below_horizon_returns_zero_for_both_models() {
        let location = Site::Paranal.geodetic();
        let target = target(266.41683, -29.00781);
        let time = scan_time(location, target, |geometry| {
            geometry.moon_zenith >= Degrees::new(90.0)
                && geometry.source_zenith < Degrees::new(80.0)
        });

        let ks = KrisciunasSchaefer1991::standard_clear_sky(location)
            .compute(time, target)
            .unwrap();
        let jones = Jones2013Spectral::from_site(Site::Paranal)
            .compute(time, target)
            .unwrap();

        assert_eq!(ks.v_flux_s10, S10s::zero());
        assert_eq!(ks.integrated.value(), 0.0);
        assert_eq!(jones.v_flux_s10, S10s::zero());
        assert_eq!(jones.integrated.value(), 0.0);
    }

    #[test]
    fn target_below_horizon_returns_zero_for_both_models() {
        let location = Site::Paranal.geodetic();
        let target = target(0.0, 70.0);
        let time = scan_time(location, target, |geometry| {
            geometry.source_zenith >= Degrees::new(90.0)
        });

        let ks = KrisciunasSchaefer1991::standard_clear_sky(location)
            .compute(time, target)
            .unwrap();
        let jones = Jones2013Spectral::from_site(Site::Paranal)
            .compute(time, target)
            .unwrap();

        assert_eq!(ks.v_flux_s10, S10s::zero());
        assert_eq!(ks.integrated.value(), 0.0);
        assert_eq!(jones.v_flux_s10, S10s::zero());
        assert_eq!(jones.integrated.value(), 0.0);
    }

    #[test]
    fn public_api_does_not_expose_moon_inputs() {
        let location = Site::Paranal.geodetic();
        let target = target(266.41683, -29.00781);
        let time = parse_time("2023-09-04T01:48:00Z");

        let _ks = KrisciunasSchaefer1991::standard_clear_sky(location)
            .compute(time, target)
            .unwrap();
        let _jones = Jones2013Spectral::from_site(Site::Paranal)
            .compute(time, target)
            .unwrap();
    }
}
