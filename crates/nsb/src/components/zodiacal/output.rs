// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Vallés Puig, Ramon
//! Internal scalar output for Zodiacal-light evaluation.

use crate::units::radiometry::{
    PhotonsPerSquareCentimeterNanosecondSteradian as BandPhotonRadiance, S10s as S10,
};

#[derive(Debug, Clone, Copy)]
pub(crate) struct ZodiacalOutputs {
    pub(crate) integrated: BandPhotonRadiance,
    pub(crate) b_flux_s10: S10,
    pub(crate) v_flux_s10: S10,
}
