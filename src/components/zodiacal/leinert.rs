//! Leinert (1998) brightness-grid wrapper for zodiacal-light lookup.
//!
//! The raw numerical table lives in `crate::leinert` (shared, because the
//! table is large and may also be referenced by integration tests). This
//! module wraps it with a typed, thread-safe [`Leinert1998Grid`] that
//! performs bilinear interpolation and applies the documented corner
//! extrapolation patches.
//!
//! # Grid layout
//!
//! `LEINERT_S10[i_lambda][j_beta]`:
//! - row 0 → `λ − λ_sun = 180°`; row 36 → `λ − λ_sun = 0°` (5° steps)
//! - column 0 → `β = 0°`; column 18 → `β = 90°` (5° steps)
//!
//! The internal [`Grid2D`] stores the y-axis (λ − λ_sun) in ascending order
//! via [`Grid2D::from_raw_row_major_y_descending`] so that the standard
//! bilinear interpolation path is identical to the legacy hand-rolled
//! `lt = (180 − dl_deg − 5·l0)/5` arithmetic (bit-for-bit equality).
//!
//! # Corner extrapolation
//!
//! For target positions close to the solar disk, the Leinert (1998) table
//! does not cover all `(β, λ−λ_sun)` cells. Three constant-fill regions
//! are attached at construction time:
//!
//! | region                 | value (S10) |
//! |------------------------|-------------|
//! | `λ−λ_sun < 20°, β < 25°` | 2450 |
//! | `λ−λ_sun < 25°, β < 20°` | 2300 |
//! | `λ−λ_sun < 30°, β < 15°` | 3700 |
//!
//! # Reference
//! Leinert et al. (1998), *A&AS* 127, 1-99.

use std::sync::OnceLock;

use crate::error::{NsbError, Result};
use crate::leinert::{
    CORNER_LL_LT_20_B_LT_25, CORNER_LL_LT_25_B_LT_20, CORNER_LL_LT_30_B_LT_15, LEINERT_S10,
};
use optica::data::Provenance;
use optica::grid::{ConstantRegion, Grid2D};
use qtty::angular::{Degree, Degrees, Radians};
use qtty::radiometry::{S10s as S10, S10 as S10Unit};

pub use crate::leinert::S10_TO_W_M2_SR_UM as LEINERT_S10_TO_W_M2_SR_UM;

/// x-axis = β [deg, ascending], y-axis = λ-λ_sun [deg, descending].
type LeinertGrid = Grid2D<Degree, Degree, S10Unit>;

/// Thread-safe handle to the lazily-initialised Leinert (1998) S10 grid.
pub struct Leinert1998Grid;

impl Leinert1998Grid {
    /// Bilinear interpolation in the Leinert (1998) table at
    /// `(β [rad], (λ−λ_sun) [rad])`.
    ///
    /// Returns the S10 brightness at 500 nm. Inputs are in radians; the
    /// function converts them to degrees internally before querying the grid.
    ///
    /// # Errors
    ///
    /// Returns [`NsbError::OutOfRange`] if `beta` or `delta_lambda` is not
    /// finite, or if `|β| > 90°`.
    pub fn lookup_s10(beta: Radians, delta_lambda: Radians) -> Result<S10> {
        if !beta.is_finite() {
            return Err(NsbError::OutOfRange(format!(
                "β={} rad is not finite",
                beta.value()
            )));
        }
        if !delta_lambda.is_finite() {
            return Err(NsbError::OutOfRange(format!(
                "Δλ={} rad is not finite",
                delta_lambda.value()
            )));
        }
        let beta_abs = beta.abs().to::<Degree>();
        let dl_abs = delta_lambda.abs().to::<Degree>().min(Degrees::new(180.0));
        if beta_abs > Degrees::new(90.0) {
            return Err(NsbError::OutOfRange(format!(
                "β={}° not in [0,90]",
                beta_abs.value()
            )));
        }
        Ok(grid().interp_at(beta_abs, dl_abs))
    }
}

fn grid() -> &'static LeinertGrid {
    static G: OnceLock<LeinertGrid> = OnceLock::new();
    G.get_or_init(|| {
        let xs: Vec<f64> = (0..=18).map(|i| i as f64 * 5.0).collect();
        let ys_desc: Vec<f64> = (0..37).map(|i| 180.0 - i as f64 * 5.0).collect();
        let mut table: Vec<f64> = Vec::with_capacity(37 * 19);
        for row in LEINERT_S10.iter() {
            table.extend_from_slice(row);
        }
        Grid2D::from_raw_row_major_y_descending(&xs, &ys_desc, &table)
            .expect("Leinert S10 grid construction is infallible for valid table constants")
            .with_constant_region(ConstantRegion::lower_corner(
                25.0,
                20.0,
                CORNER_LL_LT_20_B_LT_25,
            ))
            .with_constant_region(ConstantRegion::lower_corner(
                20.0,
                25.0,
                CORNER_LL_LT_25_B_LT_20,
            ))
            .with_constant_region(ConstantRegion::lower_corner(
                15.0,
                30.0,
                CORNER_LL_LT_30_B_LT_15,
            ))
            .with_provenance(Provenance::cited("Leinert+1998"))
    })
}

/// Legacy hand-rolled bilinear lookup — preserved for bit-for-bit parity tests.
///
/// Returns `None` for inputs outside the valid range (β ∉ [0°, 90°) or
/// |λ−λ_sun| ∉ [0°, 180°]). The corner-region clamps mirror the three
/// branches in the original Python `GetZodiacalLight`.
#[cfg(test)]
pub(crate) fn legacy_lookup_s10_for_test(beta_rad: f64, delta_lambda_rad: f64) -> Option<f64> {
    use crate::leinert::LEINERT_S10;
    let beta_deg = beta_rad.to_degrees().abs();
    let dl_deg = delta_lambda_rad.to_degrees().abs().min(180.0);
    if !(0.0..90.0).contains(&beta_deg) {
        return None;
    }
    if !(0.0..=180.0).contains(&dl_deg) {
        return None;
    }
    if dl_deg < 20.0 && beta_deg < 25.0 {
        return Some(CORNER_LL_LT_20_B_LT_25);
    }
    if dl_deg < 25.0 && beta_deg < 20.0 {
        return Some(CORNER_LL_LT_25_B_LT_20);
    }
    if dl_deg < 30.0 && beta_deg < 15.0 {
        return Some(CORNER_LL_LT_30_B_LT_15);
    }

    let b0 = (beta_deg / 5.0).floor() as usize;
    let b1 = (b0 + 1).min(18);
    let bt = (beta_deg - 5.0 * b0 as f64) / 5.0;

    let l0_idx = ((180.0 - dl_deg.ceil()) / 5.0).floor() as isize;
    let l0 = l0_idx.clamp(0, 35) as usize;
    let l1 = (l0 + 1).min(36);
    let lt = (180.0 - dl_deg - 5.0 * l0 as f64) / 5.0;

    Some(optica::grid::algo::bilinear_unit(
        LEINERT_S10[l0][b0],
        LEINERT_S10[l0][b1],
        LEINERT_S10[l1][b0],
        LEINERT_S10[l1][b1],
        bt,
        lt,
    ))
}
