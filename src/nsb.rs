//! Top-level NSB orchestration.
//!
//! Mirrors `get_NSB.py`: assembles ZL + SL + AG + Moon for a given site,
//! time, and source.

use crate::components::{airglow, moonlight, starlight, zodiacal};
use crate::error::Result;
use crate::ephemeris::{source as source_db, sun as sun_eph};
use crate::spectra::{self, integrate};
use crate::units::{BandPhotonRadiance, S10, SurfaceBrightness};
use siderust::bodies::Moon;
use siderust::calculus::horizontal::star_horizontal;
use siderust::coordinates::transform::TransformFrame;
use siderust::qtty::{Kilometer, Radian};
use siderust::time::JulianDate;
use tempoch::{Time, UTC};

pub use crate::geometry::observer::Site;
pub use siderust::coordinates::{frames::EquatorialMeanJ2000, frames::EclipticMeanJ2000, spherical::Direction as SphericalDirection};
pub use siderust::qtty::DEG;

#[derive(Debug, Clone)]
pub enum Source {
    /// Resolve by name from the built-in catalogue (`SgrA*`, `Crab`, …).
    Named(String),
    /// Direct equatorial coordinates (ICRS/J2000).
    RaDec(SphericalDirection<EquatorialMeanJ2000>),
}

bitflags::bitflags! {
    /// Which components to include in the calculation.
    #[derive(Debug, Clone, Copy)]
    pub struct ComponentMask: u8 {
        const ZODIACAL  = 0b0001;
        const STARLIGHT = 0b0010;
        const AIRGLOW   = 0b0100;
        const MOON      = 0b1000;
        const ALL       = Self::ZODIACAL.bits()
                        | Self::STARLIGHT.bits()
                        | Self::AIRGLOW.bits()
                        | Self::MOON.bits();
    }
}

#[derive(Debug, Clone)]
pub struct ObservationRequest {
    pub site: Site,
    pub time: Time<UTC>,
    pub source: Source,
    pub components: ComponentMask,
}

#[derive(Debug, Clone)]
pub struct NsbComponent {
    pub name: &'static str,
    pub integrated: BandPhotonRadiance,
    pub b_flux_s10: S10,
    pub v_flux_s10: S10,
}

#[derive(Debug, Clone)]
pub struct NsbResult {
    pub integrated: BandPhotonRadiance,
    pub b_mag: SurfaceBrightness,
    pub v_mag: SurfaceBrightness,
    pub components: Vec<NsbComponent>,
}

/// Bridge a `tempoch::Time<UTC>` to siderust's legacy TT-scale `JulianDate`.
///
/// Routes through `chrono::DateTime<Utc>` so siderust applies its own
/// UTC→TT leap-second conversion, regardless of which `tempoch` build
/// the surrounding crates pulled in.
fn jd_tt(time: Time<UTC>) -> JulianDate {
    let dt = time
        .try_to_chrono()
        .expect("UTC time is within chrono's representable range");
    JulianDate::from_utc(dt)
}

/// Top-level entry point.
pub fn calculate(req: &ObservationRequest) -> Result<NsbResult> {
    // Resolve source equatorial direction.
    let source = match &req.source {
        Source::Named(n) => source_db::resolve(n)?,
        Source::RaDec(dir) => *dir,
    };

    let jd = jd_tt(req.time);
    let hz = star_horizontal(source.ra(), source.dec(), &req.site.geodetic(), jd);
    let altitude_deg = hz.alt().value();
    let zenith_deg = 90.0 - altitude_deg;
    let ecl: SphericalDirection<EclipticMeanJ2000> = source.to_frame();
    let ecliptic_lat = ecl.lat().to::<Radian>();
    let ecliptic_lon = ecl.lon().to::<Radian>();
    let lambda_sun = sun_eph::ecliptic_longitude(jd);
    let mut delta_lambda = (ecliptic_lon - lambda_sun).value().abs();
    while delta_lambda > std::f64::consts::PI {
        delta_lambda = (2.0 * std::f64::consts::PI - delta_lambda).abs();
    }

    let solar = spectra::solar::load()?;

    let mut components = Vec::new();
    let mut total = BandPhotonRadiance::new(0.0);
    let (mut b_total, mut v_total) = (0.0, 0.0);

    if req.components.contains(ComponentMask::ZODIACAL) {
        let out = zodiacal::compute(&zodiacal::ZlInputs {
            beta_rad: ecliptic_lat.value(),
            delta_lambda_rad: delta_lambda,
            zenith_deg,
        }, &solar)?;
        total += out.integrated;
        b_total += out.b_flux_s10.value();
        v_total += out.v_flux_s10.value();
        components.push(NsbComponent {
            name: "zodiacal",
            integrated: out.integrated,
            b_flux_s10: out.b_flux_s10,
            v_flux_s10: out.v_flux_s10,
        });
    }
    if req.components.contains(ComponentMask::STARLIGHT) {
        let out = starlight::compute()?;
        total += out.integrated;
        b_total += out.b_flux_s10.value();
        v_total += out.v_flux_s10.value();
        components.push(NsbComponent {
            name: "starlight",
            integrated: out.integrated,
            b_flux_s10: out.b_flux_s10,
            v_flux_s10: out.v_flux_s10,
        });
    }
    if req.components.contains(ComponentMask::AIRGLOW) {
        let out = airglow::compute(&airglow::AgInputs { altitude_deg })?;
        total += out.integrated;
        b_total += out.b_flux_s10.value();
        v_total += out.v_flux_s10.value();
        components.push(NsbComponent {
            name: "airglow",
            integrated: out.integrated,
            b_flux_s10: out.b_flux_s10,
            v_flux_s10: out.v_flux_s10,
        });
    }
    if req.components.contains(ComponentMask::MOON) {
        let moon_pos = Moon::get_horizontal::<Kilometer>(jd, req.site.geodetic());
        let moon_dir = moon_pos.direction();
        let moon_zenith_deg = 90.0 - moon_dir.alt().value();
        let separation_deg = hz.angular_separation(&moon_dir).value();
        let phase_fraction = Moon::phase_geocentric(jd).illuminated_fraction;
        let out = moonlight::compute(&moonlight::MoonInputs {
            separation_deg,
            moon_zenith_deg,
            phase_fraction,
            source_zenith_deg: zenith_deg,
        })?;
        total += out.integrated;
        b_total += out.b_flux_s10.value();
        v_total += out.v_flux_s10.value();
        components.push(NsbComponent {
            name: "moon",
            integrated: out.integrated,
            b_flux_s10: out.b_flux_s10,
            v_flux_s10: out.v_flux_s10,
        });
    }

    Ok(NsbResult {
        integrated: total,
        b_mag: SurfaceBrightness::from_band_flux(b_total.max(f64::MIN_POSITIVE)),
        v_mag: SurfaceBrightness::from_band_flux(v_total.max(f64::MIN_POSITIVE)),
        components,
    })
}

// silence unused-import warnings until the integrator wires `integrate` below.
#[allow(dead_code)]
fn _kept_for_future_use() { let _ = integrate::flux_to_mag; }
