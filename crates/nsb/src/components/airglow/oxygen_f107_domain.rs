// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Vallés Puig, Ramon
//! Evaluation-level F10.7 domain diagnostics for experimental PALACE Oxygen.
//!
//! Distinguishes numerical membership of the **applied** operational F10.7
//! scalar in the historical PALACE evidence interval from covariate
//! compatibility with the centred 27-day training statistic, and from any
//! claim about actual solar conditions. Numerically in-range climatology must
//! not be reported as scientifically validated in-domain evidence (issue #294;
//! research context #295).

use crate::solar_activity::{F107Kind, MonthlyCompleteness, ResolvedSolarActivity};

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

/// What the applied operational F10.7 can scientifically support about actual
/// solar conditions relative to PALACE's centred-27-day training covariate.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum OxygenF107ActualConditionsClaim {
    /// Climatology or other degraded planning fallback; actual solar state unknown.
    UnknownBecausePlanningFallback,
    /// Forecast-derived monthly planning mean; not observed centred-27 evidence.
    PlanningForecastIndexOnly,
    /// Finalized monthly observed planning mean; not centred-27 training evidence.
    MonthlyObservedPlanningIndexOnly,
    /// Explicit caller scalar; provenance is caller-owned.
    ExplicitCallerScalarOnly,
}

impl OxygenF107ActualConditionsClaim {
    /// Stable machine-readable identifier.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::UnknownBecausePlanningFallback => "unknown-because-planning-fallback",
            Self::PlanningForecastIndexOnly => "planning-forecast-index-only",
            Self::MonthlyObservedPlanningIndexOnly => "monthly-observed-planning-index-only",
            Self::ExplicitCallerScalarOnly => "explicit-caller-scalar-only",
        }
    }
}

/// Centred-27-day F10.7 average used when fitting PALACE solar-response slopes.
pub const OXYGEN_F107_TRAINING_COVARIATE: &str = "centred-27-day-f107-average";

/// Per-evaluation Oxygen F10.7 applicability diagnostic.
///
/// Present only for [`crate::AirglowModel::ParanalPalaceOxygenV1`] when an F10.7
/// value was resolved for the evaluation. Continuum-only Airglow omits this
/// field. The diagnostic never upgrades experimental maturity or site
/// calibration status.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub struct OxygenF107DomainDiagnostic {
    /// F10.7 value actually applied to Oxygen scaling, in sfu.
    pub applied_sfu: f64,
    /// Inclusive lower bound of the historical training evidence interval (sfu).
    pub training_range_low_sfu: f64,
    /// Inclusive upper bound of the historical training evidence interval (sfu).
    pub training_range_high_sfu: f64,
    /// PALACE training solar covariate identity.
    pub training_covariate: &'static str,
    /// Operational quantity identity for the applied resolver product.
    pub operational_quantity: &'static str,
    /// Numerical relationship of `applied_sfu` to the training interval.
    pub numerical_range: OxygenF107NumericalRange,
    /// True when linear SCE scaling is numerically outside the training interval.
    pub numerically_extrapolated: bool,
    /// Whether the operational quantity matches the centred-27-day training statistic.
    ///
    /// Current NSB resolver products are monthly planning (or explicit/climatology)
    /// quantities, so this is false for all Automatic/Dataset/Explicit paths today.
    pub covariate_matches_training_statistic: bool,
    /// What evidence the applied input provides about actual solar conditions.
    pub actual_conditions_claim: OxygenF107ActualConditionsClaim,
    /// Whether metadata claims that **actual** solar conditions were known to lie
    /// inside the centred-27-day training domain.
    ///
    /// Always `false` for current NSB operational inputs: a climatological or
    /// monthly value numerically inside `[67, 166]` does not establish that the
    /// historical centred-27-day solar state was in-domain.
    pub claims_actual_conditions_within_training_domain: bool,
}

impl OxygenF107DomainDiagnostic {
    /// Build the Oxygen diagnostic from a resolved operational F10.7 record.
    ///
    /// `training_range_sfu` must come from the bundled continuum asset's
    /// `solar_flux_evidence_range_sfu` (authoritative; do not invent bounds).
    pub fn from_resolved(solar: &ResolvedSolarActivity, training_range_sfu: (f64, f64)) -> Self {
        let (low, high) = training_range_sfu;
        debug_assert!(low.is_finite() && high.is_finite() && low <= high);
        let applied = solar.value.value();
        let numerical_range = classify_numerical_range(applied, low, high);
        let numerically_extrapolated = !matches!(
            numerical_range,
            OxygenF107NumericalRange::WithinTrainingInterval
        );
        let operational_quantity = operational_quantity_id(solar);
        let actual_conditions_claim = actual_conditions_claim(solar);
        Self {
            applied_sfu: applied,
            training_range_low_sfu: low,
            training_range_high_sfu: high,
            training_covariate: OXYGEN_F107_TRAINING_COVARIATE,
            operational_quantity,
            numerical_range,
            numerically_extrapolated,
            covariate_matches_training_statistic: false,
            actual_conditions_claim,
            claims_actual_conditions_within_training_domain: false,
        }
    }

    /// Compact provenance fragment for string metadata channels.
    pub fn provenance_fragment(&self) -> String {
        format!(
            "oxygen_f107_domain applied_sfu={} training_range_sfu=[{}, {}] training_covariate={} operational_quantity={} numerical_range={} numerically_extrapolated={} covariate_matches_training_statistic={} actual_conditions_claim={} claims_actual_conditions_within_training_domain={}",
            self.applied_sfu,
            self.training_range_low_sfu,
            self.training_range_high_sfu,
            self.training_covariate,
            self.operational_quantity,
            self.numerical_range.as_str(),
            self.numerically_extrapolated,
            self.covariate_matches_training_statistic,
            self.actual_conditions_claim.as_str(),
            self.claims_actual_conditions_within_training_domain,
        )
    }
}

/// Inclusive classification against the asset evidence interval.
pub fn classify_numerical_range(
    applied_sfu: f64,
    low_sfu: f64,
    high_sfu: f64,
) -> OxygenF107NumericalRange {
    if applied_sfu < low_sfu {
        OxygenF107NumericalRange::BelowTrainingInterval
    } else if applied_sfu > high_sfu {
        OxygenF107NumericalRange::AboveTrainingInterval
    } else {
        OxygenF107NumericalRange::WithinTrainingInterval
    }
}

fn operational_quantity_id(solar: &ResolvedSolarActivity) -> &'static str {
    match solar.record.kind {
        F107Kind::Explicit => "explicit-caller-override",
        F107Kind::Climatology => "planning-climatology-fallback",
        F107Kind::Observed => match solar.monthly_completeness {
            Some(MonthlyCompleteness::CompleteObserved) => "monthly-observed-planning-mean",
            _ => "observed-f107-nonstandard-monthly-product",
        },
        F107Kind::Forecast => match solar.monthly_completeness {
            Some(MonthlyCompleteness::CompleteForecast) => "monthly-45-day-forecast-mean",
            Some(MonthlyCompleteness::ProvisionalObservedPlusForecast) => {
                "monthly-provisional-observed-plus-forecast-mean"
            }
            Some(MonthlyCompleteness::OfficialMonthlyPrediction) => {
                "official-monthly-solar-cycle-prediction"
            }
            Some(MonthlyCompleteness::Climatology) => "planning-climatology-fallback",
            Some(MonthlyCompleteness::CompleteObserved) => "monthly-observed-planning-mean",
            None => "forecast-f107-unspecified-monthly-product",
        },
    }
}

fn actual_conditions_claim(solar: &ResolvedSolarActivity) -> OxygenF107ActualConditionsClaim {
    if solar.is_degraded_planning_input()
        || matches!(solar.record.kind, F107Kind::Climatology)
        || matches!(
            solar.monthly_completeness,
            Some(MonthlyCompleteness::Climatology)
        )
    {
        // Forecasts and climatology cannot evidence actual centred-27 conditions.
        // Distinguish explicit caller overrides first.
        if matches!(solar.record.kind, F107Kind::Explicit) {
            return OxygenF107ActualConditionsClaim::ExplicitCallerScalarOnly;
        }
        if matches!(
            solar.monthly_completeness,
            Some(
                MonthlyCompleteness::CompleteForecast
                    | MonthlyCompleteness::ProvisionalObservedPlusForecast
                    | MonthlyCompleteness::OfficialMonthlyPrediction
            )
        ) || matches!(solar.record.kind, F107Kind::Forecast)
        {
            return OxygenF107ActualConditionsClaim::PlanningForecastIndexOnly;
        }
        return OxygenF107ActualConditionsClaim::UnknownBecausePlanningFallback;
    }
    match solar.record.kind {
        F107Kind::Explicit => OxygenF107ActualConditionsClaim::ExplicitCallerScalarOnly,
        F107Kind::Observed => OxygenF107ActualConditionsClaim::MonthlyObservedPlanningIndexOnly,
        F107Kind::Forecast => OxygenF107ActualConditionsClaim::PlanningForecastIndexOnly,
        F107Kind::Climatology => OxygenF107ActualConditionsClaim::UnknownBecausePlanningFallback,
    }
}

/// Authoritative Oxygen/PALACE F10.7 evidence interval from the continuum asset.
pub(crate) fn training_range_sfu_from_continuum_asset() -> (f64, f64) {
    super::calibration::load_builtin_standard()
        .expect("bundled continuum asset is build-verified")
        .solar_flux_evidence_range_sfu()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::solar_activity::{F107Record, MonthlyCompleteness};
    use crate::SolarFluxUnits;
    use chrono::NaiveDate;

    fn solar(
        value: f64,
        kind: F107Kind,
        product: &str,
        completeness: Option<MonthlyCompleteness>,
    ) -> ResolvedSolarActivity {
        let date = NaiveDate::from_ymd_opt(2000, 4, 9).unwrap();
        let date_s = date.format("%Y-%m-%d").to_string();
        let record = F107Record {
            date: date_s.clone(),
            value_sfu: value,
            kind,
            provider: match kind {
                F107Kind::Explicit => "caller".into(),
                F107Kind::Climatology => "nsb".into(),
                _ => "noaa-swpc".into(),
            },
            product: product.into(),
            observation_date: matches!(kind, F107Kind::Observed).then(|| date_s.clone()),
            forecast_issued_at_utc: matches!(kind, F107Kind::Forecast)
                .then(|| "2000-04-01T00:00:00Z".into()),
            retrieved_at_utc: None,
            valid_from: Some(date_s.clone()),
            valid_through: Some(date_s),
            cadence: Some(match kind {
                F107Kind::Climatology => "climatology".into(),
                F107Kind::Explicit => "explicit".into(),
                _ => "monthly".into(),
            }),
            uncertainty_sfu: None,
            range_low_sfu: None,
            range_high_sfu: None,
            source_locator: None,
        };
        ResolvedSolarActivity {
            value: SolarFluxUnits::new(value),
            record,
            dataset_id: Some("test".into()),
            snapshot_id: Some("test".into()),
            checksum_sha256: None,
            requested_date: date,
            resolution_step: "test",
            monthly_completeness: completeness,
            observed_days: completeness.map(|_| 30),
            forecast_days: Some(0),
            total_days: Some(30),
        }
    }

    #[test]
    fn inclusive_endpoints_and_neighbours() {
        let (lo, hi) = (67.0, 166.0);
        assert_eq!(
            classify_numerical_range(66.999, lo, hi),
            OxygenF107NumericalRange::BelowTrainingInterval
        );
        assert_eq!(
            classify_numerical_range(67.0, lo, hi),
            OxygenF107NumericalRange::WithinTrainingInterval
        );
        assert_eq!(
            classify_numerical_range(100.0, lo, hi),
            OxygenF107NumericalRange::WithinTrainingInterval
        );
        assert_eq!(
            classify_numerical_range(166.0, lo, hi),
            OxygenF107NumericalRange::WithinTrainingInterval
        );
        assert_eq!(
            classify_numerical_range(166.001, lo, hi),
            OxygenF107NumericalRange::AboveTrainingInterval
        );
    }

    #[test]
    fn climatology_inside_interval_does_not_claim_actual_in_domain() {
        let resolved = solar(
            129.20671119074768,
            F107Kind::Climatology,
            "documented-climatology-fallback",
            Some(MonthlyCompleteness::Climatology),
        );
        let diag = OxygenF107DomainDiagnostic::from_resolved(&resolved, (67.0, 166.0));
        assert_eq!(
            diag.numerical_range,
            OxygenF107NumericalRange::WithinTrainingInterval
        );
        assert!(!diag.numerically_extrapolated);
        assert!(!diag.covariate_matches_training_statistic);
        assert_eq!(
            diag.actual_conditions_claim,
            OxygenF107ActualConditionsClaim::UnknownBecausePlanningFallback
        );
        assert!(!diag.claims_actual_conditions_within_training_domain);
        assert_eq!(diag.operational_quantity, "planning-climatology-fallback");
        assert_eq!(diag.training_covariate, OXYGEN_F107_TRAINING_COVARIATE);
    }

    #[test]
    fn high_explicit_is_numerical_extrapolation_without_validation_claim() {
        let resolved = solar(200.0, F107Kind::Explicit, "explicit-override", None);
        let diag = OxygenF107DomainDiagnostic::from_resolved(&resolved, (67.0, 166.0));
        assert_eq!(
            diag.numerical_range,
            OxygenF107NumericalRange::AboveTrainingInterval
        );
        assert!(diag.numerically_extrapolated);
        assert!(!diag.claims_actual_conditions_within_training_domain);
        assert_eq!(
            diag.actual_conditions_claim,
            OxygenF107ActualConditionsClaim::ExplicitCallerScalarOnly
        );
    }

    #[test]
    fn monthly_observed_within_range_is_not_centred_training_evidence() {
        let resolved = solar(
            150.0,
            F107Kind::Observed,
            "observed-solar-cycle-indices",
            Some(MonthlyCompleteness::CompleteObserved),
        );
        let diag = OxygenF107DomainDiagnostic::from_resolved(&resolved, (67.0, 166.0));
        assert_eq!(
            diag.numerical_range,
            OxygenF107NumericalRange::WithinTrainingInterval
        );
        assert!(!diag.numerically_extrapolated);
        assert!(!diag.covariate_matches_training_statistic);
        assert_eq!(
            diag.actual_conditions_claim,
            OxygenF107ActualConditionsClaim::MonthlyObservedPlanningIndexOnly
        );
        assert!(!diag.claims_actual_conditions_within_training_domain);
    }

    #[test]
    fn asset_training_range_matches_documented_palace_evidence() {
        assert_eq!(training_range_sfu_from_continuum_asset(), (67.0, 166.0));
    }
}
