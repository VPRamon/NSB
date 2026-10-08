// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Vallés Puig, Ramon
use crate::error::CliError;
use nsb::units::radiometry::PhotonsPerSquareCentimeterNanosecondSteradian as BandPhotonRadiance;

pub fn parse_max_nsb(value: f64) -> Result<BandPhotonRadiance, CliError> {
    if value.is_finite() && value >= 0.0 {
        Ok(BandPhotonRadiance::new(value))
    } else {
        Err(CliError::InvalidMaxNsb)
    }
}

pub fn parse_min_nsb(value: Option<f64>, max: f64) -> Result<Option<BandPhotonRadiance>, CliError> {
    match value {
        Some(v) if !v.is_finite() || v < 0.0 => Err(CliError::InvalidMinNsb),
        Some(v) if v > max => Err(CliError::InvalidNsbRange),
        Some(v) => Ok(Some(BandPhotonRadiance::new(v))),
        None => Ok(None),
    }
}
