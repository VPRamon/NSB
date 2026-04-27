//! Airmass formulae used by the Python NSB model.
//!
//! `Airmass(zenithDeg, which=3)` selects between several approximations.
//! We expose all of them; the orchestrator defaults to `which = 3`
//! (Krisciunas & Schaefer 1991).

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

#[inline]
pub fn airmass(zenith_deg: f64, formula: Formula) -> f64 {
    let z = zenith_deg.to_radians();
    match formula {
        Formula::PlaneParallel => 1.0 / z.cos(),
        Formula::Young => {
            let c = z.cos();
            let num = 1.002432 * c * c + 0.148386 * c + 0.0096467;
            let den = c * c * c + 0.149864 * c * c + 0.0102963 * c + 0.000303978;
            num / den
        }
        Formula::Rozenberg => 1.0 / (z.cos() + 0.025 * (-11.0 * z.cos()).exp()),
        Formula::KrisciunasSchaefer => {
            // X = (1 - 0.96 sin² z)^(-1/2) (Krisciunas & Schaefer 1991).
            let s = z.sin();
            (1.0 - 0.96 * s * s).powf(-0.5)
        }
    }
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
