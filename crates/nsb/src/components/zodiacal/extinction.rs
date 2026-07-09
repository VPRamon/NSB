//! Atmospheric extinction strategy for zodiacal-light propagation.
//!
//! Zodiacal light originates above the atmosphere. Before reaching a
//! ground-based telescope it is attenuated by Rayleigh scattering and
//! aerosol (Mie) extinction. This module defines the [`ZodiacalExtinction`]
//! strategy enum and the default `Noll2012Approx` approximation.
//!
//! # Default model: `Noll2012Approx`
//!
//! The default approximation follows Noll et al. (2012), using piecewise-linear
//! fits to the extinction coefficients as a function of the base-10 logarithm
//! of the zodiacal surface brightness (in W m⁻² sr⁻¹ µm⁻¹ at the relevant
//! wavelength). The resulting transmission is
//!
//! ```text
//! T(λ, z) = exp(−τ_eff)
//! ```
//!
//! where `τ_eff = τ₀(λ) · (f_ext_R + f_ext_M) · X(z)` and `X(z)` is the
//! airmass at zenith distance `z` computed with the Young (1994) formula.
//!
//! **Limitations.** The approximation is not site-calibrated. It uses a
//! fixed aerosol model inherited from the SkyCalc parametric sky background
//! model (Noll et al. 2012). For scientific observations requiring precise
//! throughput prediction, a site-specific atmosphere profile should be used.
//!
//! # Advanced extensibility
//!
//! The enum is designed so a future `AtmosphereProfile(Box<dyn ...>)` variant
//! can be added without breaking existing code. Currently only `None` and
//! `Noll2012Approx` are implemented.
//!
//! # Reference
//! Noll et al. (2012), *A&A* 543, A92.

use crate::units::{WattPerSquareMeterSteradianMicrometer, WattsPerSquareMeterSteradianMicrometer};
use qtty::angular::{Degrees, Radian};
use qtty::radiometry::WattsPerSquareMeterSteradianNanometer;
use qtty::unit;
use siderust::atmosphere::{airmass, Young1994};
use siderust::qtty::Nanometers;

/// Atmospheric extinction strategy for zodiacal-light propagation.
///
/// Determines whether — and how — the exoatmospheric zodiacal signal is
/// attenuated before reaching a ground-based observer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ZodiacalExtinction {
    /// No atmospheric attenuation. Use this for exoatmospheric predictions
    /// or when attenuation is handled externally.
    None,

    /// Noll et al. (2012) piecewise-linear Rayleigh + Mie approximation.
    ///
    /// This is the default and matches the original NSB Python pipeline.
    /// It is a generic approximation and is not calibrated to any specific
    /// observing site or atmospheric profile.
    #[default]
    Noll2012Approx,
}

impl ZodiacalExtinction {
    /// Compute the transmission `T(λ, zenith) ∈ (0, 1]` for a photon at
    /// wavelength `lambda_nm` observed at zenith distance `zenith`.
    ///
    /// `zl_value_w_m2_sr_um` is the zodiacal spectral radiance at `lambda_nm`
    /// in W m⁻² sr⁻¹ µm⁻¹, which is used as a proxy input to the Noll
    /// parametric extinction model.
    ///
    /// Returns `1.0` for [`ZodiacalExtinction::None`].
    pub fn transmission(&self, zl_value_w_m2_sr_um: f64, lambda_nm: f64, zenith: Degrees) -> f64 {
        let spectral_radiance = WattsPerSquareMeterSteradianMicrometer::new(zl_value_w_m2_sr_um)
            .to::<unit::WattPerSquareMeterSteradianNanometer>();
        self.transmission_for_spectral_radiance(
            spectral_radiance,
            Nanometers::new(lambda_nm),
            zenith,
        )
    }

    pub(crate) fn transmission_for_spectral_radiance(
        &self,
        spectral_radiance: WattsPerSquareMeterSteradianNanometer,
        wavelength: Nanometers,
        zenith: Degrees,
    ) -> f64 {
        match self {
            Self::None => 1.0,
            Self::Noll2012Approx => noll2012_transmission(spectral_radiance, wavelength, zenith),
        }
    }
}

fn noll2012_transmission(
    spectral_radiance: WattsPerSquareMeterSteradianNanometer,
    wavelength: Nanometers,
    zenith: Degrees,
) -> f64 {
    let zl_value_w_m2_sr_um = spectral_radiance
        .to::<WattPerSquareMeterSteradianMicrometer>()
        .value();
    let dex = zl_value_w_m2_sr_um.log10();
    let fext_m = if dex <= 2.255 {
        1.309 * dex - 2.598
    } else {
        0.468 * dex - 0.702
    };
    let fext_r = if dex <= 2.244 {
        1.407 * dex - 2.692
    } else {
        0.527 * dex - 0.715
    };

    let lam_um = wavelength.to::<unit::Micrometer>().value();
    let kaer = if lam_um < 0.4 {
        0.05
    } else {
        0.013 * lam_um.powf(-1.38)
    };
    let tau0 = (10f64).powf(-0.4 * kaer).ln();
    let am = airmass::<Young1994>(zenith.to::<Radian>());
    let tau_eff = tau0 * (fext_r + fext_m) * am.value();
    (-tau_eff).exp()
}
