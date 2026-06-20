use super::*;
use qtty::Second;

pub struct KrisciunasSchaefer1991 {
    location: Geodetic<ECEF>,
    k_ext: f64,
}

impl KrisciunasSchaefer1991 {
    pub const DEFAULT_PERIOD_SEARCH_STEP: Second = Second::new(600.0);

    pub fn new(location: Geodetic<ECEF>, k_ext: f64) -> Self {
        Self { location, k_ext }
    }

    pub fn standard_clear_sky(location: Geodetic<ECEF>) -> Self {
        Self::new(location, DEFAULT_K_EXT)
    }

    pub fn compute(
        &self,
        time: Time<UTC>,
        target: SphericalDirection<EquatorialMeanJ2000>,
    ) -> Result<MoonOutputs> {
        let geometry = lunar_geometry(time, self.location, target);
        compute_krisciunas_schaefer_1991(&geometry, self.k_ext)
    }

    pub fn periods_in_range(
        &self,
        window: Period<UTC>,
        target: SphericalDirection<EquatorialMeanJ2000>,
        min: PhotonsPerSquareCentimeterNanosecondSteradian,
        max: PhotonsPerSquareCentimeterNanosecondSteradian,
    ) -> Result<Vec<Period<UTC>>> {
        self.periods_in_range_with_step(window, target, min, max, Self::DEFAULT_PERIOD_SEARCH_STEP)
    }

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
}

pub(super) fn compute_krisciunas_schaefer_1991(
    inp: &MoonlightGeometry,
    k_ext: f64,
) -> Result<MoonOutputs> {
    if !inp.moon_zenith.is_finite()
        || !inp.source_zenith.is_finite()
        || !inp.separation.is_finite()
        || !k_ext.is_finite()
    {
        return Ok(zero_outputs());
    }
    if inp.moon_zenith >= Degrees::new(90.0)
        || inp.source_zenith >= Degrees::new(90.0)
        || inp.separation <= Degrees::new(0.0)
    {
        return Ok(zero_outputs());
    }

    let b_nl = scattered_brightness_nanolamberts(
        inp.phase.phase_angle,
        inp.separation,
        inp.moon_zenith,
        inp.source_zenith,
        k_ext,
    );

    if !b_nl.is_finite() || b_nl <= 0.0 {
        return Ok(zero_outputs());
    }

    let v_mag_arcsec2 = v_mag_per_arcsec2_from_nl(b_nl);
    let v_s10 = 10f64.powf(0.4 * (NSB_S10_ZP - v_mag_arcsec2));
    let integrated = v_s10 * S10_V_TO_INTEGRATED_PH;

    Ok(MoonOutputs {
        integrated: radiometry::PhotonsPerSquareCentimeterNanosecondSteradian::new(integrated),
        b_flux_s10: radiometry::S10s::new(v_s10),
        v_flux_s10: radiometry::S10s::new(v_s10),
    })
}

/// `I*(α)` — lunar illuminance above the atmosphere (relative units, eq. 8).
fn lunar_illuminance_outside_atmosphere(alpha: Radians) -> f64 {
    let a = alpha.abs().to::<Degree>().value();
    let exponent = -0.4 * (3.84 + 0.026 * a + 4.0e-9 * a.powi(4));
    10f64.powf(exponent)
}

/// `f(ρ)` — angular scattering function of K&S 1991 (eq. 16/17), summing
/// the Rayleigh + aerosol forward-scattering term and the Mie aureole term.
fn scattering_function(rho: Degrees) -> f64 {
    let cos_rho = rho.cos();
    let rayleigh = 10f64.powf(5.36) * (1.06 + cos_rho * cos_rho);
    let aureole = 10f64.powf(6.15 - rho.value() / 40.0);
    rayleigh + aureole
}

/// Convert moonlight brightness `B` (nanolamberts) into V-band surface
/// brightness (mag/arcsec²) via the inverse of K&S eq. 1.
fn v_mag_per_arcsec2_from_nl(b_nl: f64) -> f64 {
    (20.7233 - (b_nl / 34.08).ln()) / 0.92104
}

/// Scattered-moon surface brightness at the source location, in nanolamberts
/// (eq. 15 of K&S 1991).
fn scattered_brightness_nanolamberts(
    alpha: Radians,
    rho: Degrees,
    z_moon: Degrees,
    z_src: Degrees,
    k_ext: f64,
) -> f64 {
    let i_star = lunar_illuminance_outside_atmosphere(alpha);
    let f_rho = scattering_function(rho);
    let am_moon = airmass::<KrisciunasSchaeferAirmass>(z_moon.to::<Radian>());
    let am_src = airmass::<KrisciunasSchaeferAirmass>(z_src.to::<Radian>());
    let trans_moon = 10f64.powf(-0.4 * k_ext * am_moon.value());
    let absorb_path = 1.0 - 10f64.powf(-0.4 * k_ext * am_src.value());
    f_rho * i_star * trans_moon * absorb_path
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{DateTime, Utc};
    use qtty::angular::Radians;
    use qtty::photometry::s10_to_surface_brightness;
    use qtty::radiometry::S10s;
    use siderust::qtty::{Degrees as SiderustDegrees, IlluminationFractions, Meters};

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

    fn inputs(alpha_deg: f64, rho_deg: f64, z_moon: f64, z_src: f64) -> MoonlightGeometry {
        MoonlightGeometry {
            separation: Degrees::new(rho_deg),
            moon_zenith: Degrees::new(z_moon),
            phase: make_phase(alpha_deg),
            source_zenith: Degrees::new(z_src),
            moon_distance: siderust::event::lunar::photometry::MEAN_MOON_DISTANCE,
        }
    }

    fn compute(inp: &MoonlightGeometry) -> Result<MoonOutputs> {
        compute_krisciunas_schaefer_1991(inp, DEFAULT_K_EXT)
    }

    fn v_mag_arcsec2(out: &MoonOutputs) -> f64 {
        s10_to_surface_brightness(out.v_flux_s10, NSB_S10_ZP).value()
    }

    #[test]
    fn full_moon_reference_geometry_matches_published_brightness() {
        let out = compute(&inputs(0.0, 90.0, 45.0, 45.0)).unwrap();
        assert!(out.v_flux_s10 > S10s::zero());
        let v_mag = v_mag_arcsec2(&out);
        assert!(
            (v_mag - 18.0).abs() < 0.7,
            "V_sky = {v_mag:.2} mag/arcsec² not within 0.7 of published ~18"
        );
    }

    #[test]
    fn new_moon_contribution_is_negligible_relative_to_full() {
        let out_new = compute(&inputs(180.0, 90.0, 45.0, 45.0)).unwrap();
        let out_full = compute(&inputs(0.0, 90.0, 45.0, 45.0)).unwrap();
        assert!(out_full.v_flux_s10 > S10s::zero());
        assert!(
            out_new.v_flux_s10 < out_full.v_flux_s10 * 1e-3,
            "new-moon V S10 ({}) should be << full-moon ({})",
            out_new.v_flux_s10.value(),
            out_full.v_flux_s10.value()
        );
    }

    #[test]
    fn scattering_function_exhibits_expected_behavior() {
        let inputs_at_rho = |rho| compute(&inputs(45.0, rho, 30.0, 60.0)).unwrap();

        let b5 = inputs_at_rho(5.0).v_flux_s10;
        let b90 = inputs_at_rho(90.0).v_flux_s10;
        let b120 = inputs_at_rho(120.0).v_flux_s10;
        let b175 = inputs_at_rho(175.0).v_flux_s10;

        assert!(
            b5 > S10s::zero(),
            "brightness at rho=5 deg must be positive"
        );
        assert!(
            b90 > S10s::zero(),
            "brightness at rho=90 deg must be positive"
        );
        assert!(
            b90 < b5,
            "brightness at rho=90 deg should be less than forward scattering"
        );
        assert!(
            b120 > b90,
            "brightness should increase from rho=90 deg to rho=120 deg"
        );
        assert!(b175 > b120, "brightness should increase toward rho=180 deg");
        assert!(
            b175 < b5,
            "backscattering should be weaker than forward scattering"
        );
    }

    #[test]
    fn moon_below_horizon_returns_zero() {
        let out = compute(&inputs(0.0, 30.0, 95.0, 30.0)).unwrap();
        assert_eq!(out.v_flux_s10, S10s::zero());
        assert_eq!(out.integrated.value(), 0.0);
    }

    #[test]
    fn airmass_at_zenith_is_unity() {
        let am = airmass::<KrisciunasSchaeferAirmass>(Degrees::new(0.0).to::<Radian>());
        assert!((am.value() - 1e0).abs() < 1e-12, "X(0) = {:?}", am);
    }

    #[test]
    fn periods_in_range_covers_window_for_large_bound() {
        let model = KrisciunasSchaefer1991::standard_clear_sky(test_location());
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
    fn periods_in_range_rejects_inverted_bounds() {
        let model = KrisciunasSchaefer1991::standard_clear_sky(test_location());
        let err = model
            .periods_in_range(
                test_window(),
                test_target(),
                PhotonsPerSquareCentimeterNanosecondSteradian::new(2.0),
                PhotonsPerSquareCentimeterNanosecondSteradian::new(1.0),
            )
            .unwrap_err();
        assert!(err.to_string().contains("minimum radiance"));
    }
}
