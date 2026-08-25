//! Shared Gaia-compatible HEALPix helpers backed by Siderust.
//!
//! Starlight accumulation uses nested HEALPix at or below Gaia DR3 level 12
//! (`nside <= 4096`). Geometry invariants (`npix`, index domain, power-of-two
//! nested `nside`) come from [`siderust::healpix`], not handwritten formulas.

use anyhow::{bail, Context, Result};
use siderust::healpix::{HealpixGrid, HealpixOrdering, Nside};

/// Gaia DR3 HEALPix order embedded in `source_id` (nside = 4096).
pub const GAIA_HEALPIX_ORDER: u32 = 12;
/// Largest nested `nside` Starlight accepts for Gaia-derived products.
pub const GAIA_MAX_NSIDE: u32 = 1 << GAIA_HEALPIX_ORDER;

/// Validate a nested HEALPix `nside` usable for Gaia Starlight products.
pub fn gaia_nested_nside(nside: u32) -> Result<Nside> {
    let nside = Nside::new(nside).context("invalid HEALPix nside")?;
    if nside.get() > GAIA_MAX_NSIDE {
        bail!(
            "HEALPix nside {} exceeds Gaia level-12 maximum {GAIA_MAX_NSIDE}",
            nside.get()
        );
    }
    // Nested grids reject non-power-of-two nside.
    let _ = gaia_nested_grid_from_nside(nside)?;
    Ok(nside)
}

/// Build a nested HEALPix grid for Gaia Starlight products.
pub fn gaia_nested_grid(nside: u32) -> Result<HealpixGrid> {
    gaia_nested_grid_from_nside(gaia_nested_nside(nside)?)
}

fn gaia_nested_grid_from_nside(nside: Nside) -> Result<HealpixGrid> {
    HealpixGrid::new(nside, HealpixOrdering::Nested).with_context(|| {
        format!(
            "invalid nested HEALPix grid for nside {} (must be a power of two)",
            nside.get()
        )
    })
}

/// Pixel count `12 * nside^2` for a Gaia nested grid.
pub fn gaia_nested_npix(nside: u32) -> Result<u64> {
    Ok(gaia_nested_grid(nside)?.npix())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_gaia_compatible_powers_of_two() {
        for nside in [1, 2, 128, 256, 4096] {
            let grid = gaia_nested_grid(nside).unwrap();
            assert_eq!(grid.nside().get(), nside);
            assert_eq!(grid.npix(), 12 * u64::from(nside) * u64::from(nside));
        }
    }

    #[test]
    fn rejects_zero_non_power_of_two_and_above_gaia_max() {
        for nside in [0, 3, 8192] {
            assert!(gaia_nested_nside(nside).is_err());
            assert!(gaia_nested_grid(nside).is_err());
        }
    }
}
