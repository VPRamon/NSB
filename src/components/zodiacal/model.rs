//! [`ZodiacalLight`] model: constructor logic and high-level compute methods.
//!
//! # Model overview
//!
//! Zodiacal light is sunlight scattered by interplanetary dust in the inner
//! Solar System. It is one of the dominant diffuse sky-background contributors
//! away from the Galactic plane.
//!
//! The default model — [`ZodiacalLight::standard`] — uses:
//!
//! - **Source brightness**: Leinert et al. (1998) empirical S10 table,
//!   bilinearly interpolated at the target ecliptic position relative to the Sun.
//! - **Spectral model**: solar spectrum scaled to match the 500 nm S10
//!   brightness, with Leinert wavelength reddening applied.
//! - **Atmospheric propagation**: Noll et al. (2012) Rayleigh + Mie
//!   extinction approximation.
//!
//! # Source model vs atmospheric propagation
//!
//! The design separates two conceptually distinct steps:
//!
//! 1. **Exoatmospheric component** — the zodiacal radiance above the
//!    atmosphere, computed from the target's position relative to the Sun.
//!    Use [`ZodiacalLight::compute_exoatmospheric`] to obtain this component.
//!    No observer location is required.
//!
//! 2. **Observed component** — the signal that arrives at the telescope
//!    after propagating through the atmosphere. Use
//!    [`ZodiacalLight::compute_observed`] (or the combined
//!    [`ZodiacalLight::compute`]) to obtain this. Requires an observer
//!    location to derive the target zenith distance.
//!
//! The default extinction model is an approximation. It is not calibrated to
//! any specific site or atmospheric profile. Advanced users can disable
//! extinction via [`ZodiacalExtinction::None`] or will be able to supply a
//! custom profile in a future release.
//!
//! # Custom brightness grids
//!
//! [`ZodiacalBrightnessModel::CustomGrid`] allows callers to supply a
//! validated tabular S10 brightness grid. See [`ZodiacalBrightnessGrid`] for
//! construction constraints.

use crate::error::{NsbError, Result};
use crate::evaluator::Target;
use crate::reference::solar;
use optica::spectrum::SampledSpectrum;
use siderust::coordinates::centers::Geodetic;
use siderust::coordinates::frames::ECEF;
use siderust::qtty::{length::Meter, Nanometer};
use tempoch::{Time, UTC};

use super::extinction::ZodiacalExtinction;
use super::geometry;
use super::leinert::Leinert1998Grid;
use super::output::{ZodiacalOutputs, ZodiacalSpectrum};
use super::spectrum as zl_spectrum;

// ─── Public types ──────────────────────────────────────────────────────────

/// Tabular S10 brightness grid for use with [`ZodiacalBrightnessModel::CustomGrid`].
///
/// # Validation
///
/// The grid must satisfy:
/// - `beta_axis` strictly increasing, values in `[0°, 90°]` (degrees).
/// - `delta_lambda_axis` strictly increasing, values in `[0°, 180°]` (degrees).
/// - `s10_values` finite and non-negative, in row-major order
///   `[beta_idx][delta_lambda_idx]`.
///
/// Bilinear interpolation is used for lookup. No corner-region patches are
/// applied (unlike the Leinert table); the grid is assumed to cover its full
/// domain.
#[derive(Debug, Clone)]
pub struct ZodiacalBrightnessGrid {
    pub(super) beta_axis: Vec<f64>,
    pub(super) delta_lambda_axis: Vec<f64>,
    pub(super) s10_values: Vec<Vec<f64>>,
    /// Optional human-readable provenance string.
    pub provenance: Option<String>,
}

impl ZodiacalBrightnessGrid {
    /// Construct a new custom brightness grid.
    ///
    /// # Errors
    ///
    /// Returns [`NsbError::OutOfRange`] if any axis is not strictly increasing,
    /// values are out of range, or the S10 table has wrong dimensions.
    pub fn new(
        beta_axis: Vec<f64>,
        delta_lambda_axis: Vec<f64>,
        s10_values: Vec<Vec<f64>>,
        provenance: Option<String>,
    ) -> Result<Self> {
        if beta_axis.len() < 2 || delta_lambda_axis.len() < 2 {
            return Err(NsbError::OutOfRange(
                "custom ZodiacalBrightnessGrid axes must have at least 2 points each".to_string(),
            ));
        }
        if !is_strictly_increasing(&beta_axis) {
            return Err(NsbError::OutOfRange(
                "beta_axis must be strictly increasing".to_string(),
            ));
        }
        if !is_strictly_increasing(&delta_lambda_axis) {
            return Err(NsbError::OutOfRange(
                "delta_lambda_axis must be strictly increasing".to_string(),
            ));
        }
        if *beta_axis.first().unwrap() < 0.0 || *beta_axis.last().unwrap() > 90.0 {
            return Err(NsbError::OutOfRange(
                "beta_axis values must be in [0, 90] degrees".to_string(),
            ));
        }
        if *delta_lambda_axis.first().unwrap() < 0.0 || *delta_lambda_axis.last().unwrap() > 180.0 {
            return Err(NsbError::OutOfRange(
                "delta_lambda_axis values must be in [0, 180] degrees".to_string(),
            ));
        }
        if s10_values.len() != beta_axis.len() {
            return Err(NsbError::OutOfRange(format!(
                "s10_values row count {} != beta_axis length {}",
                s10_values.len(),
                beta_axis.len()
            )));
        }
        for (i, row) in s10_values.iter().enumerate() {
            if row.len() != delta_lambda_axis.len() {
                return Err(NsbError::OutOfRange(format!(
                    "s10_values row {} has length {} != delta_lambda_axis length {}",
                    i,
                    row.len(),
                    delta_lambda_axis.len()
                )));
            }
            for &v in row {
                if !v.is_finite() || v < 0.0 {
                    return Err(NsbError::OutOfRange(format!(
                        "s10_values[{i}] contains non-finite or negative value: {v}"
                    )));
                }
            }
        }
        Ok(Self {
            beta_axis,
            delta_lambda_axis,
            s10_values,
            provenance,
        })
    }

    /// Bilinear interpolation at `(beta_deg, delta_lambda_deg)`.
    pub(super) fn lookup_s10(&self, beta_deg: f64, delta_lambda_deg: f64) -> Result<f64> {
        let beta_deg = beta_deg.abs().min(90.0);
        let delta_lambda_deg = delta_lambda_deg.abs().min(180.0);
        let (ib0, ib1, tb) = bracket(&self.beta_axis, beta_deg);
        let (il0, il1, tl) = bracket(&self.delta_lambda_axis, delta_lambda_deg);
        let v = bilinear(
            self.s10_values[ib0][il0],
            self.s10_values[ib0][il1],
            self.s10_values[ib1][il0],
            self.s10_values[ib1][il1],
            tb,
            tl,
        );
        Ok(v)
    }
}

fn is_strictly_increasing(v: &[f64]) -> bool {
    v.windows(2).all(|w| w[1] > w[0])
}

fn bracket(axis: &[f64], value: f64) -> (usize, usize, f64) {
    let pos = axis.partition_point(|&x| x <= value);
    let i1 = pos.min(axis.len() - 1);
    let i0 = if i1 == 0 { 0 } else { i1 - 1 };
    let t = if axis[i1] > axis[i0] {
        (value - axis[i0]) / (axis[i1] - axis[i0])
    } else {
        0.0
    };
    (i0, i1, t.clamp(0.0, 1.0))
}

fn bilinear(v00: f64, v01: f64, v10: f64, v11: f64, tx: f64, ty: f64) -> f64 {
    let r0 = v00 + tx * (v10 - v00);
    let r1 = v01 + ty * (v11 - v01);
    r0 + ty * (r1 - r0)
}

/// Brightness source model used by [`ZodiacalLight`].
#[derive(Debug, Clone)]
pub enum ZodiacalBrightnessModel {
    /// Leinert et al. (1998) empirical S10 table with bilinear interpolation.
    /// This is the validated default.
    Leinert1998,
    /// Caller-supplied tabular S10 grid with validated axes and values.
    CustomGrid(ZodiacalBrightnessGrid),
}

// ─── ZodiacalLight ─────────────────────────────────────────────────────────

/// Zodiacal-light model.
///
/// # Usage
///
/// ```no_run
/// use nsb::components::zodiacal::{ZodiacalLight, ZodiacalExtinction};
/// use nsb::evaluator::{Location, Target};
/// use nsb::site::Site;
/// use nsb::DEG;
/// use tempoch::{Time, UTC};
///
/// # fn main() -> nsb::error::Result<()> {
/// let model = ZodiacalLight::standard()?;
/// // …
/// # Ok(())
/// # }
/// ```
///
/// See [`ZodiacalLight::compute`], [`ZodiacalLight::compute_exoatmospheric`],
/// and [`ZodiacalLight::compute_observed`] for evaluation methods.
#[derive(Debug, Clone)]
pub struct ZodiacalLight {
    brightness_model: ZodiacalBrightnessModel,
    solar_spectrum: SampledSpectrum<Nanometer, Meter>,
    extinction: ZodiacalExtinction,
}

impl ZodiacalLight {
    // ── Constructors ──────────────────────────────────────────────────────

    /// Standard model: Leinert (1998) brightness grid, built-in solar
    /// spectrum, and Noll (2012) approximate atmospheric extinction.
    ///
    /// This is the recommended constructor for normal use.
    pub fn standard() -> Result<Self> {
        Self::leinert1998()
    }

    /// Leinert (1998) brightness grid with built-in solar spectrum and
    /// Noll (2012) approximate atmospheric extinction.
    pub fn leinert1998() -> Result<Self> {
        Ok(Self {
            brightness_model: ZodiacalBrightnessModel::Leinert1998,
            solar_spectrum: solar::load()?,
            extinction: ZodiacalExtinction::Noll2012Approx,
        })
    }

    /// Custom brightness model with the built-in solar spectrum and
    /// Noll (2012) approximate atmospheric extinction.
    pub fn with_brightness_model(model: ZodiacalBrightnessModel) -> Result<Self> {
        Ok(Self {
            brightness_model: model,
            solar_spectrum: solar::load()?,
            extinction: ZodiacalExtinction::Noll2012Approx,
        })
    }

    // ── Builder modifiers ─────────────────────────────────────────────────

    /// Override the solar spectrum used for spectral scaling.
    ///
    /// The supplied spectrum must be positive at 500 nm; otherwise
    /// [`compute`][`ZodiacalLight::compute`] will return an error.
    pub fn with_solar_spectrum(
        mut self,
        solar_spectrum: SampledSpectrum<Nanometer, Meter>,
    ) -> Self {
        self.solar_spectrum = solar_spectrum;
        self
    }

    /// Override the atmospheric extinction strategy.
    pub fn with_extinction(mut self, extinction: ZodiacalExtinction) -> Self {
        self.extinction = extinction;
        self
    }

    // ── Compute: combined (geometry + observed) ───────────────────────────

    /// Evaluate the observed zodiacal-light contribution at a single
    /// `(time, location, target)`.
    ///
    /// Computes target ecliptic geometry, applies the configured brightness
    /// model and spectral model, and propagates the result through the
    /// configured atmospheric extinction.
    ///
    /// This is the primary API for ground-based sky background estimation.
    pub fn compute(
        &self,
        time: Time<UTC>,
        location: Geodetic<ECEF>,
        target: Target,
    ) -> Result<ZodiacalOutputs> {
        self.compute_observed(time, location, target)
    }

    // ── Compute: exoatmospheric ───────────────────────────────────────────

    /// Evaluate the zodiacal-light contribution *above the atmosphere*.
    ///
    /// Does not require an observer location. Does not apply atmospheric
    /// extinction (equivalent to [`ZodiacalExtinction::None`]).
    ///
    /// Use this to obtain the celestial component before local atmospheric
    /// propagation, e.g. for space-based predictions or for isolating the
    /// source contribution from the propagation effects.
    pub fn compute_exoatmospheric(
        &self,
        time: Time<UTC>,
        target: Target,
    ) -> Result<ZodiacalOutputs> {
        let geom = geometry::compute_exoatmospheric(time, target)?;
        self.evaluate_geometry(&geom, ZodiacalExtinction::None)
    }

    // ── Compute: observed ─────────────────────────────────────────────────

    /// Evaluate the observed zodiacal-light contribution with atmospheric
    /// extinction applied.
    ///
    /// Computes the target zenith distance from `location` and `time` and
    /// applies the configured extinction strategy.
    ///
    /// # Horizon semantics
    ///
    /// If the target is at or below the horizon (altitude ≤ 0°), this method
    /// returns a zero-radiance result rather than an error. This matches
    /// planner semantics: a target that is not yet observable contributes zero
    /// background. For strict scientific use, check the target altitude before
    /// calling this method.
    pub fn compute_observed(
        &self,
        time: Time<UTC>,
        location: Geodetic<ECEF>,
        target: Target,
    ) -> Result<ZodiacalOutputs> {
        let geom = geometry::compute_observed(time, location, target)?;
        let alt = geom
            .zenith
            .map(|z| qtty::angular::Degrees::new(90.0) - z)
            .unwrap_or(qtty::angular::Degrees::new(0.0));
        if alt.value() <= 0.0 {
            return Ok(zero_outputs());
        }
        self.evaluate_geometry(&geom, self.extinction)
    }

    // ── Compute: full spectrum ────────────────────────────────────────────

    /// Evaluate the zodiacal spectrum and scalar summaries for an observed
    /// `(time, location, target)`.
    ///
    /// Unlike [`compute`][`ZodiacalLight::compute`], this method allocates
    /// and returns the full sampled photon-radiance spectrum. Prefer
    /// `compute` in hot loops.
    ///
    /// # Horizon semantics
    ///
    /// Returns zero for below-horizon targets (see [`compute_observed`]).
    pub fn compute_spectrum(
        &self,
        time: Time<UTC>,
        location: Geodetic<ECEF>,
        target: Target,
    ) -> Result<ZodiacalSpectrum> {
        let geom = geometry::compute_observed(time, location, target)?;
        let alt = geom
            .zenith
            .map(|z| qtty::angular::Degrees::new(90.0) - z)
            .unwrap_or(qtty::angular::Degrees::new(0.0));
        if alt.value() <= 0.0 {
            return Ok(zero_spectrum());
        }
        zl_spectrum::compute_spectrum(&geom, &self.solar_spectrum, self.extinction)
    }

    /// Evaluate the exoatmospheric zodiacal spectrum and scalar summaries.
    ///
    /// No location required; no atmospheric extinction applied.
    pub fn compute_spectrum_exoatmospheric(
        &self,
        time: Time<UTC>,
        target: Target,
    ) -> Result<ZodiacalSpectrum> {
        let geom = geometry::compute_exoatmospheric(time, target)?;
        zl_spectrum::compute_spectrum(&geom, &self.solar_spectrum, ZodiacalExtinction::None)
    }

    // ── Internal ──────────────────────────────────────────────────────────

    fn evaluate_geometry(
        &self,
        geom: &geometry::ZodiacalGeometry,
        extinction: ZodiacalExtinction,
    ) -> Result<ZodiacalOutputs> {
        match &self.brightness_model {
            ZodiacalBrightnessModel::Leinert1998 => {
                zl_spectrum::compute_outputs(geom, &self.solar_spectrum, extinction)
            }
            ZodiacalBrightnessModel::CustomGrid(grid) => {
                self.evaluate_custom_grid(geom, grid, extinction)
            }
        }
    }

    fn evaluate_custom_grid(
        &self,
        geom: &geometry::ZodiacalGeometry,
        grid: &ZodiacalBrightnessGrid,
        extinction: ZodiacalExtinction,
    ) -> Result<ZodiacalOutputs> {
        use qtty::angular::Degree;
        let beta_deg = geom.beta.abs().to::<Degree>().value();
        let dl_deg = geom.delta_lambda.to::<Degree>().value().abs().min(180.0);
        let s10_500 = grid.lookup_s10(beta_deg, dl_deg)?;

        let custom_geom = super::geometry::ZodiacalGeometry {
            beta: geom.beta,
            delta_lambda: geom.delta_lambda,
            zenith: geom.zenith,
        };

        // Temporarily override the Leinert lookup result by constructing a
        // synthetic geometry with the custom S10 value baked into the scale.
        // We reuse the spectrum builder, which calls Leinert internally, so
        // we apply a correction factor: scale the solar spectrum so that the
        // Leinert lookup result is replaced by the custom grid value.
        //
        // Since `compute_outputs` calls `Leinert1998Grid::lookup_s10` internally,
        // we build a scaled solar spectrum that produces the custom S10 at 500 nm.
        let leinert_s10_500 = Leinert1998Grid::lookup_s10(geom.beta, geom.delta_lambda)?.value();
        if leinert_s10_500 <= 0.0 {
            return Err(NsbError::OutOfRange(
                "Leinert reference value at 500 nm is non-positive".to_string(),
            ));
        }
        let scale = s10_500 / leinert_s10_500;
        let scaled_solar = scale_spectrum(&self.solar_spectrum, scale);
        zl_spectrum::compute_outputs(&custom_geom, &scaled_solar, extinction)
    }
}

// ─── Helpers ───────────────────────────────────────────────────────────────

fn zero_outputs() -> ZodiacalOutputs {
    use qtty::radiometry::{
        PhotonsPerSquareCentimeterNanosecondSteradian as BandPhotonRadiance, S10s as S10,
    };
    ZodiacalOutputs {
        integrated: BandPhotonRadiance::new(0.0),
        b_flux_s10: S10::new(0.0),
        v_flux_s10: S10::new(0.0),
    }
}

fn zero_spectrum() -> ZodiacalSpectrum {
    use optica::data::Provenance;
    use optica::grid::OutOfRange;
    use optica::spectrum::Interpolation;
    use qtty::radiometry::{
        PhotonsPerSquareCentimeterNanosecondSteradian as BandPhotonRadiance, S10s as S10,
    };

    let spectrum = SampledSpectrum::<Nanometer, Meter>::from_raw(
        vec![300.0, 650.0],
        vec![0.0, 0.0],
        Interpolation::Linear,
        OutOfRange::ClampToEndpoints,
        Some(Provenance::computed("zodiacal-zero")),
    )
    .expect("zero spectrum construction is infallible for valid constant inputs");

    ZodiacalSpectrum {
        spectrum,
        integrated: BandPhotonRadiance::new(0.0),
        b_flux_s10: S10::new(0.0),
        v_flux_s10: S10::new(0.0),
    }
}

/// Return a new spectrum with all y-values multiplied by `scale`.
fn scale_spectrum(
    s: &SampledSpectrum<Nanometer, Meter>,
    scale: f64,
) -> SampledSpectrum<Nanometer, Meter> {
    use optica::grid::OutOfRange;
    use optica::spectrum::Interpolation;

    let xs = s.xs_raw().to_vec();
    let ys: Vec<f64> = s.ys_raw().iter().map(|&y| y * scale).collect();
    SampledSpectrum::<Nanometer, Meter>::from_raw(
        xs,
        ys,
        Interpolation::Linear,
        OutOfRange::ClampToEndpoints,
        None,
    )
    .expect("scale_spectrum: input spectrum was already valid")
}
