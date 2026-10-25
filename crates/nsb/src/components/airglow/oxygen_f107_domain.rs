// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Vallés Puig, Ramon
//! Numerical F10.7 range only; solar provenance remains on `solar_activity`.

use super::calibration::AirglowContinuum;
use crate::solar_activity::ResolvedSolarActivity;

/// How the applied F10.7 compares numerically to PALACE Oxygen's historical
/// training evidence interval (inclusive endpoints from the continuum asset).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum OxygenF107NumericalRange {
    /// Applied value is strictly below the training interval.
    BelowTrainingInterval,
    /// Applied value lies in `[low, high]` inclusive.
    WithinTrainingInterval,
    /// Applied value is strictly above the training interval.
    AboveTrainingInterval,
}

impl OxygenF107NumericalRange {
    /// Stable machine-readable identifier.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::BelowTrainingInterval => "below-training-interval",
            Self::WithinTrainingInterval => "within-training-interval",
            Self::AboveTrainingInterval => "above-training-interval",
        }
    }
}

/// Numerical comparison of applied Oxygen F10.7 with the asset's inclusive
/// historical interval. This does not establish scientific applicability,
/// actual centred-27-day solar conditions, or site calibration. Oxygen remains
/// experimental. See the accompanying `NsbComponentMetadata::solar_activity`
/// for explicit/observed/forecast/climatology provenance and temporal convention.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub struct OxygenF107DomainDiagnostic {
    /// F10.7 scalar actually applied to Oxygen scaling (sfu).
    pub applied_sfu: f64,
    /// Inclusive lower training bound from the loaded continuum asset (sfu).
    pub training_range_low_sfu: f64,
    /// Inclusive upper training bound from the loaded continuum asset (sfu).
    pub training_range_high_sfu: f64,
}

impl OxygenF107DomainDiagnostic {
    /// Scientific interpretation carried by CLI, Python and prose metadata.
    pub const LIMITATION: &str = "Numerical range only: operational monthly planning inputs, climatology and caller scalars do not establish actual centred-27-day F10.7 training-domain membership.";

    pub(crate) fn from_resolved(
        solar: &ResolvedSolarActivity,
        continuum: &AirglowContinuum,
    ) -> Self {
        let (low, high) = continuum.solar_flux_evidence_range_sfu();
        Self {
            applied_sfu: solar.value.value(),
            training_range_low_sfu: low,
            training_range_high_sfu: high,
        }
    }

    /// Derived numerical relationship; endpoints are included. This never
    /// classifies the actual solar state or asserts validation.
    pub fn numerical_range(&self) -> OxygenF107NumericalRange {
        if self.applied_sfu < self.training_range_low_sfu {
            OxygenF107NumericalRange::BelowTrainingInterval
        } else if self.applied_sfu > self.training_range_high_sfu {
            OxygenF107NumericalRange::AboveTrainingInterval
        } else {
            OxygenF107NumericalRange::WithinTrainingInterval
        }
    }

    pub(crate) fn provenance_fragment(&self) -> String {
        format!(
            "oxygen_f107_domain applied_sfu={} training_range_sfu=[{}, {}] numerical_range={}; {}",
            self.applied_sfu,
            self.training_range_low_sfu,
            self.training_range_high_sfu,
            self.numerical_range().as_str(),
            Self::LIMITATION,
        )
    }
}
