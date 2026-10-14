// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Vallés Puig, Ramon
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
//! of the 500 nm zodiacal surface brightness in units of
//! 10⁻⁸ W m⁻² sr⁻¹ µm⁻¹ (Noll Eqs. 19–20). The resulting transmission is
//!
//! ```text
//! T(λ, z) = exp(−τ_eff)
//! ```
//!
//! where `τ_eff = (f_ext_R τ_R(λ) + f_ext_M τ_M(λ)) · X(z)` and `X(z)` is the
//! airmass at zenith distance `z` computed with the Young (1994) formula.
//!
//! **Limitations.** The approximation is not site-calibrated. It uses a
//! fixed Paranal-like molecular column (744 hPa local pressure) and the
//! Noll Eq. 4 aerosol law. The brightness factors always use the source at
//! 500 nm, independently of the wavelength being transported. Negative
//! effective factors represent net in-scattering and may give T > 1.
//! Fits primarily cover night (solar zenith >=108°), target zenith <=50°
//! (up to 70° for high brightness); other geometries are extrapolations. For scientific observations requiring precise
//! throughput prediction, a site-specific atmosphere profile should be used.
//!
//! # Advanced extensibility
//!
//! The enum is non-exhaustive so additional propagation strategies that preserve
//! the selector's current public traits can be added compatibly. Stateful or
//! profile-owning propagation requires a separate configuration design rather
//! than adding a non-`Copy` payload variant here. Currently only `None` and
//! `Noll2012Approx` are implemented.
//!
//! # Reference
//! Noll et al. (2012), *A&A* 543, A92.

use crate::site::{atmosphere::rayleigh_optical_depth_local_pressure, AtmosphericConditions};
use crate::units::angular::{Degrees, Radian};
use crate::units::dimensionless::Transmittances;
use crate::units::radiometry::WattsPerSquareMeterSteradianNanometer;
use crate::units::unit;
use crate::units::WattPerSquareMeterSteradianMicrometer;
use siderust::atmosphere::{airmass, Young1994};
use siderust::qtty::Nanometers;

/// Atmospheric extinction strategy for zodiacal-light propagation.
///
/// Additional site-calibrated strategies may be added; match with a wildcard.
#[cfg_attr(
    feature = "python",
    pyo3::pyclass(
        eq,
        frozen,
        rename_all = "SCREAMING_SNAKE_CASE",
        module = "nsb",
        from_py_object
    )
)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[non_exhaustive]
pub enum ZodiacalExtinction {
    /// No atmospheric attenuation in ground-observer evaluation.
    ///
    /// Horizon visibility still applies; this does not switch evaluation to an
    /// exoatmospheric geometry mode. Use this when attenuation is handled externally.
    None,

    /// Noll et al. (2012) piecewise-linear Rayleigh + Mie approximation.
    ///
    /// This is the default generic approximation and is not calibrated to any
    /// specific observing site or atmospheric profile.
    #[default]
    Noll2012Approx,
}

impl ZodiacalExtinction {
    /// Stable machine-readable identifier.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::Noll2012Approx => "noll-2012-approximation",
        }
    }

    pub(crate) fn transmission_for_spectral_radiance(
        &self,
        reference_500: WattsPerSquareMeterSteradianNanometer,
        wavelength: Nanometers,
        zenith: Degrees,
    ) -> Transmittances {
        match self {
            Self::None => Transmittances::new(1.0),
            Self::Noll2012Approx => noll2012_transmission(reference_500, wavelength, zenith),
        }
    }
}

fn noll2012_transmission(
    reference_500: WattsPerSquareMeterSteradianNanometer,
    wavelength: Nanometers,
    zenith: Degrees,
) -> Transmittances {
    // A caller-defined absent source stays zero. The logarithmic empirical
    // fit is undefined at zero; use a neutral multiplier for that boundary.
    if reference_500.value() == 0.0 {
        return Transmittances::new(1.0);
    }
    let zl_value_w_m2_sr_um = reference_500
        .to::<WattPerSquareMeterSteradianMicrometer>()
        .value();
    // I0 is the dimensionless numerical value in Noll Fig. 7 units.
    let dex = (zl_value_w_m2_sr_um / 1.0e-8).log10();
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
    // Noll Eq. 1: positive optical depth τ = 0.4 ln(10) k.
    // Molecular and aerosol factors apply to their own optical depths.
    let tau_m = 0.4 * std::f64::consts::LN_10 * kaer;
    let tau_r =
        rayleigh_optical_depth_local_pressure(wavelength, AtmosphericConditions::paranal_average())
            .value();
    let am = airmass::<Young1994>(zenith.to::<Radian>());
    let tau_eff = (tau_r * fext_r + tau_m * fext_m) * am.value();
    Transmittances::new((-tau_eff).exp())
}
