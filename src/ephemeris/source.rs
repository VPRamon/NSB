//! Catalogue of named sources used by the Python `setup_source` helper.

use siderust::coordinates::{frames, spherical};
use siderust::qtty::DEG;

use crate::error::{NsbError, Result};

/// Equatorial (ICRS/J2000) direction of a sky source.
pub type EquatorialPos = spherical::Direction<frames::EquatorialMeanJ2000>;

/// Resolve a source by name. Currently supports the names hardcoded in
/// `setup_source`.
pub fn resolve(name: &str) -> Result<EquatorialPos> {
    match name.trim() {
        // SgrA* : RA 17h45m40.04s, Dec -29°00'28.1"
        "SgrA*" | "Sgr A*" | "sgra*" => Ok(EquatorialPos::new(266.41683 * DEG, -29.00781 * DEG)),
        // Crab pulsar
        "Crab" | "Crab Nebula" | "crab" => Ok(EquatorialPos::new(83.6331 * DEG, 22.0145 * DEG)),
        // Polaris (just for testing)
        "Polaris" | "polaris" => Ok(EquatorialPos::new(37.95456 * DEG, 89.26411 * DEG)),
        other => Err(NsbError::UnknownSource(other.to_string())),
    }
}
