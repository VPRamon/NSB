//! Leinert (1998) zodiacal-light reference table and brightness-grid wrapper.
//!
//! This module contains the transcribed S10 data constants and wraps them with
//! a typed, thread-safe [`Leinert1998Grid`] that performs bilinear
//! interpolation and applies the documented corner extrapolation patches.
//!
//! # Table layout
//!
//! `LEINERT_S10[i_lambda][j_beta]`
//! - rows: ecliptic longitude offset `λ − λ_sun` from 180° down to 0° in 5°
//!   steps (37 rows, index 0 = 180°).
//! - columns: ecliptic latitude `β` from 0° to 90° in 5° steps (19 columns).
//! - values: zodiacal surface brightness in S10 units (equivalent 10th-magnitude
//!   stars per square degree) at 500 nm.
//!
//! The internal [`Grid2D`] stores the y-axis (λ − λ_sun) in ascending order
//! via [`Grid2D::from_raw_row_major_y_descending`] so that the standard
//! bilinear interpolation path is identical to the historical hand-rolled
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
//!
//! Ch. Leinert et al., "The 1997 reference of diffuse night sky brightness",
//! *A&AS* 127 (1998) 1–99. The table constants follow the published 500 nm
//! S10 grid; the unmeasured solar-corner cells use the three constant regions
//! documented above.
//!
//! Provenance:
//! Leinert zodiacal grid lives in `components::zodiacal`.

use std::sync::OnceLock;

use crate::error::{NsbError, Result};
use optica::data::Provenance;
use optica::grid::{ConstantRegion, Grid2D};
use qtty::angular::{Degree, Degrees, Radians};
use qtty::radiometry::{S10s as S10, S10 as S10Unit};

// ─── Raw table ────────────────────────────────────────────────────────────────

/// `LEINERT_S10[i_lambda][j_beta]` — see module docs.
const LEINERT_S10: [[f64; 19]; 37] = [
    [
        180., 166., 152., 139., 127., 116., 105., 96., 89., 82., 76., 71., 66., 62., 59., 58., 58.,
        60., 63.,
    ], // 180
    [
        169., 163., 151., 138., 126., 115., 105., 96., 89., 82., 76., 71., 66., 62., 59., 58., 58.,
        60., 63.,
    ], // 175
    [
        161., 158., 147., 135., 123., 114., 104., 96., 89., 82., 76., 70., 66., 62., 59., 58., 58.,
        60., 63.,
    ], // 170
    [
        153., 150., 140., 129., 118., 110., 102., 95., 88., 81., 75., 70., 65., 62., 59., 58., 58.,
        60., 63.,
    ], // 165
    [
        147., 144., 134., 122., 113., 106., 98., 93., 86., 80., 75., 69., 65., 61., 59., 58., 59.,
        61., 63.,
    ], // 160
    [
        143., 140., 130., 118., 110., 102., 94., 89., 83., 78., 73., 68., 64., 61., 58., 58., 59.,
        61., 63.,
    ], // 155
    [
        140., 139., 129., 116., 107., 99., 91., 86., 80., 75., 71., 67., 63., 60., 58., 58., 59.,
        61., 63.,
    ], // 150
    [
        139., 138., 129., 115., 106., 97., 89., 83., 77., 73., 69., 65., 62., 60., 58., 58., 59.,
        61., 63.,
    ], // 145
    [
        139., 138., 129., 115., 105., 96., 87., 81., 75., 71., 67., 64., 62., 60., 58., 59., 60.,
        61., 63.,
    ], // 140
    [
        140., 139., 130., 115., 105., 95., 86., 80., 74., 70., 66., 63., 61., 59., 58., 59., 60.,
        62., 63.,
    ], // 135
    [
        141., 140., 132., 116., 105., 95., 86., 80., 74., 69., 65., 63., 61., 60., 59., 60., 60.,
        62., 63.,
    ], // 130
    [
        144., 142., 135., 118., 106., 96., 87., 80., 74., 69., 65., 63., 61., 60., 59., 60., 61.,
        62., 63.,
    ], // 125
    [
        147., 145., 138., 120., 108., 98., 88., 81., 75., 70., 66., 63., 61., 60., 59., 60., 61.,
        62., 63.,
    ], // 120
    [
        152., 150., 143., 124., 111., 100., 89., 82., 76., 71., 67., 64., 62., 61., 60., 61., 61.,
        62., 63.,
    ], // 115
    [
        158., 156., 148., 128., 113., 101., 91., 84., 78., 73., 68., 65., 63., 61., 61., 61., 62.,
        62., 63.,
    ], // 110
    [
        166., 164., 154., 133., 117., 104., 93., 86., 80., 75., 70., 67., 64., 62., 61., 62., 62.,
        62., 63.,
    ], // 105
    [
        175., 172., 160., 137., 120., 107., 96., 89., 82., 77., 72., 68., 65., 63., 62., 62., 62.,
        62., 63.,
    ], // 100
    [
        187., 184., 168., 144., 125., 111., 99., 92., 84., 79., 74., 70., 66., 64., 63., 63., 62.,
        63., 63.,
    ], //  95
    [
        202., 196., 176., 151., 130., 115., 103., 95., 87., 81., 76., 72., 68., 66., 64., 64., 63.,
        63., 63.,
    ], //  90
    [
        219., 211., 186., 158., 137., 121., 108., 99., 90., 84., 79., 74., 70., 69., 65., 64., 63.,
        63., 63.,
    ], //  85
    [
        239., 227., 197., 167., 144., 127., 113., 103., 94., 87., 82., 77., 72., 70., 67., 65.,
        64., 63., 63.,
    ], //  80
    [
        264., 248., 210., 177., 153., 134., 118., 107., 98., 91., 85., 79., 74., 71., 68., 66.,
        64., 63., 63.,
    ], //  75
    [
        296., 273., 228., 188., 162., 142., 124., 112., 103., 95., 88., 82., 77., 73., 69., 67.,
        65., 64., 63.,
    ], //  70
    [
        338., 305., 250., 205., 174., 152., 122., 120., 109., 99., 92., 85., 79., 75., 70., 68.,
        65., 64., 63.,
    ], //  65
    [
        394., 345., 275., 228., 190., 163., 143., 129., 116., 105., 96., 89., 82., 77., 72., 69.,
        66., 64., 63.,
    ], //  60
    [
        470., 395., 310., 253., 209., 179., 158., 140., 125., 113., 103., 93., 85., 79., 74., 70.,
        66., 64., 63.,
    ], //  55
    [
        572., 458., 355., 285., 238., 200., 173., 153., 135., 120., 108., 98., 89., 82., 76., 71.,
        67., 65., 63.,
    ], //  50
    [
        710., 570., 435., 335., 278., 228., 195., 168., 146., 130., 115., 103., 92., 84., 78., 72.,
        67., 65., 63.,
    ], //  45
    [
        925., 735., 545., 415., 325., 264., 220., 186., 160., 140., 123., 108., 95., 87., 80., 74.,
        68., 65., 63.,
    ], //  40
    [
        1290., 990., 710., 530., 400., 310., 250., 208., 177., 151., 132., 113., 99., 90., 82.,
        75., 69., 66., 63.,
    ], //  35
    [
        1940., 1460., 955., 660., 480., 365., 285., 230., 194., 162., 140., 119., 103., 93., 84.,
        76., 70., 66., 63.,
    ], //  30
    [
        3000., 2210., 1350., 860., 585., 425., 320., 253., 209., 174., 150., 126., 107., 96., 86.,
        78., 72., 67., 63.,
    ], //  25
    [
        5000., 3500., 1880., 1100., 710., 495., 355., 278., 226., 185., 157., 130., 111., 98., 88.,
        79., 73., 67., 63.,
    ], //  20
    [
        9000., 5300., 2690., 1450., 870., 590., 410., 310., 242., 196., 162., 136., 115., 100.,
        89., 80., 73., 67., 63.,
    ], //  15
    [
        6750., 4500., 3700., 1930., 1070., 675., 460., 340., 260., 206., 167., 138., 117., 102.,
        90., 80., 74., 68., 63.,
    ], //  10
    [
        5250., 3750., 3000., 2300., 1200., 740., 490., 355., 271., 212., 169., 139., 118., 103.,
        90., 80., 74., 68., 63.,
    ], //   5
    [
        4244., 3238., 2725., 2450., 1260., 770., 500., 360., 275., 215., 170., 140., 118., 103.,
        90., 80., 74., 68., 63.,
    ], //   0
];

/// Re-exported as the canonical name used by zodiacal spectrum calculations.
pub(super) use crate::units::S10_TO_W_M2_SR_UM as LEINERT_S10_TO_W_M2_SR_UM;

/// Maximum-value clamps for the unmeasured corners of the table
/// (`λ-λ_sun` close to 0, low `β`).
const CORNER_LL_LT_20_B_LT_25: f64 = 2450.0;
const CORNER_LL_LT_25_B_LT_20: f64 = 2300.0;
const CORNER_LL_LT_30_B_LT_15: f64 = 3700.0;

// ─── Grid wrapper ─────────────────────────────────────────────────────────────

/// x-axis = β [deg, ascending], y-axis = λ-λ_sun [deg, descending].
type LeinertGrid = Grid2D<Degree, Degree, S10Unit>;

/// Thread-safe handle to the lazily-initialised Leinert (1998) S10 grid.
pub(crate) struct Leinert1998Grid;

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
    pub(crate) fn lookup_s10(beta: Radians, delta_lambda: Radians) -> Result<S10> {
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

/// Historical hand-rolled bilinear lookup retained only for unit comparisons.
///
/// Returns `None` for inputs outside the valid range (β ∉ [0°, 90°) or
/// |λ−λ_sun| ∉ [0°, 180°]). The corner-region clamps mirror the three
/// constant regions used by the production grid.
#[cfg(test)]
pub(crate) fn reference_lookup_s10_for_test(beta_rad: f64, delta_lambda_rad: f64) -> Option<f64> {
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