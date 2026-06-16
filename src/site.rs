//! Named CTAO observation site.
//!
//! `darknsb` recognised two CTAO sites:
//! * `CTAO-N` → La Palma (Roque de los Muchachos).
//! * `CTAO-S` → Cerro Paranal.
//!
//! These map to geodetic constants exposed by `siderust::catalogs::observatories`.
//!
//! Scientific role:
//! a night-sky-background value is meaningless without an observing site,
//! because local horizon geometry, airmass, and the apparent positions of the
//! Sun and Moon depend on where the observer is on Earth.
//!
//! Contribution to the science:
//! this file provides the minimal named-site mapping used by the historical
//! CTAO-oriented interface. It turns user-facing site names into the geodetic
//! coordinates that drive all topocentric calculations in the evaluator.

use siderust::catalogs::observatories;
use siderust::coordinates::centers::Geodetic;
use siderust::coordinates::frames::ECEF;

use crate::error::{NsbError, Result};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Site {
    /// Cerro Paranal (CTAO-S).
    Paranal,
    /// Roque de los Muchachos, La Palma (CTAO-N).
    LaPalma,
}

impl Site {
    pub fn from_name(name: &str) -> Result<Self> {
        match name.trim() {
            "CTAO-S" | "ctao-s" | "paranal" | "Paranal" => Ok(Site::Paranal),
            "CTAO-N" | "ctao-n" | "lapalma" | "La Palma" | "Roque de los Muchachos" => {
                Ok(Site::LaPalma)
            }
            other => Err(NsbError::UnknownSite(other.to_string())),
        }
    }

    pub fn geodetic(self) -> Geodetic<ECEF> {
        match self {
            Site::Paranal => observatories::EL_PARANAL.geodetic(),
            Site::LaPalma => observatories::ROQUE_DE_LOS_MUCHACHOS.geodetic(),
        }
    }
}
