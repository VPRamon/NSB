//! Airmass formula re-export.
//!
//! Behavior is upstreamed in [`siderust::atmosphere::airmass`]. This
//! module preserves NSB's `f64`-degree call-site signature for backwards
//! compatibility (the orchestrator threads zenith distance as degrees).

use siderust::atmosphere::airmass::{airmass as upstream_airmass, AirmassFormula};
use qtty::angular::Radians;

/// Airmass formula selector mirroring the Python `which` argument.
#[derive(Debug, Clone, Copy)]
pub enum Formula {
    /// Plane-parallel: `sec z`. Diverges near the horizon.
    PlaneParallel,
    /// Young 1994.
    Young,
    /// Rozenberg 1966.
    Rozenberg,
    /// Krisciunas & Schaefer 1991 — Python default (`which = 3`).
    KrisciunasSchaefer,
}

impl From<Formula> for AirmassFormula {
    #[inline]
    fn from(f: Formula) -> AirmassFormula {
        match f {
            Formula::PlaneParallel => AirmassFormula::PlaneParallel,
            Formula::Young => AirmassFormula::Young1994,
            Formula::Rozenberg => AirmassFormula::Rozenberg1966,
            Formula::KrisciunasSchaefer => AirmassFormula::KrisciunasSchaefer1991,
        }
    }
}

#[inline]
pub fn airmass(zenith_deg: f64, formula: Formula) -> f64 {
    upstream_airmass(Radians::new(zenith_deg.to_radians()), formula.into())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn zenith_is_one() {
        for f in [Formula::PlaneParallel, Formula::Young, Formula::Rozenberg, Formula::KrisciunasSchaefer] {
            let x = airmass(0.0, f);
            assert!((x - 1.0).abs() < 1e-3, "{:?} -> {}", f, x);
        }
    }
}
