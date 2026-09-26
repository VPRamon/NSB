//! Airglow model-selection and evaluation-outcome contracts.
//!
//! Distinguishes:
//! - configuration/selection metadata (known before evaluation);
//! - evaluation outcome (known only after a time-dependent query).
//!
//! The global climatological planning model from #157 is deferred; automatic
//! selection currently resolves to a temporary Paranal-derived planning
//! fallback with that fallback machine-visible via typed reason enums.

use super::model::AirglowModel;
use crate::error::Result;

/// How the Airglow scientific model is chosen for an evaluator.
///
/// Automatic selection is a durable policy hook for future #157 climatology and
/// site-calibration refinement. Explicit selection always wins and never falls
/// back to another model.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum AirglowSelection {
    /// Deterministic automatic selection policy.
    ///
    /// Until a global climatological model is admitted (#157), this resolves to
    /// the temporary Paranal-derived planning fallback. That fallback is always
    /// reported in selection metadata and is not presented as a globally
    /// representative scientific default.
    Automatic,
    /// Caller explicitly selected this scientific model.
    ///
    /// Unsupported explicit selections fail; they do not silently switch models.
    Explicit(AirglowModel),
}

impl AirglowSelection {
    /// Stable machine-readable selection-policy identifier.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Automatic => "automatic",
            Self::Explicit(_) => "explicit",
        }
    }

    /// Return the explicitly requested model, if any.
    pub const fn requested_model(self) -> Option<AirglowModel> {
        match self {
            Self::Automatic => None,
            Self::Explicit(model) => Some(model),
        }
    }
}

/// Whether configuration selected Airglow automatically or explicitly.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum AirglowSelectionKind {
    /// [`AirglowSelection::Automatic`].
    Automatic,
    /// [`AirglowSelection::Explicit`].
    Explicit,
}

impl AirglowSelectionKind {
    /// Stable machine-readable identifier.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Automatic => "automatic",
            Self::Explicit => "explicit",
        }
    }
}

/// Why automatic Airglow policy deliberately selected a temporary fallback.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum AirglowFallbackReason {
    /// Global climatological planning model is not yet admitted (#157 deferred).
    GlobalPlanningModelUnavailable,
}

impl AirglowFallbackReason {
    /// Stable machine-readable identifier (canonical contract).
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::GlobalPlanningModelUnavailable => "global-planning-model-unavailable",
        }
    }
}

/// Why Airglow radiance is physically zero for a query.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum AirglowPhysicalZeroReason {
    /// Query is outside astronomical night; continuum is physically inactive.
    OutsideAstronomicalNight,
}

impl AirglowPhysicalZeroReason {
    /// Stable machine-readable identifier (canonical contract).
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::OutsideAstronomicalNight => "outside-astronomical-night",
        }
    }
}

/// Physical evaluation outcome distinct from selection/fallback policy.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum AirglowPhysicalOutcome {
    /// Model produced a physical radiance estimate for the query domain.
    Evaluated,
    /// Component is physically inactive for the query (for example outside
    /// astronomical night). Distinct from invalid input or unsupported models.
    PhysicalZero,
}

impl AirglowPhysicalOutcome {
    /// Stable machine-readable identifier.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Evaluated => "evaluated",
            Self::PhysicalZero => "physical-zero",
        }
    }
}

/// Configuration/selection metadata known without a time-dependent evaluation.
///
/// [`crate::NsbEvaluator::describe_components`] may populate this without
/// inventing a physical evaluation outcome. When automatic selection later
/// becomes context-dependent (#157), `resolved_model` / fallback fields may be
/// `None` until evaluation resolves them.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct AirglowSelectionMetadata {
    /// Automatic versus explicit configuration choice.
    pub selection_kind: AirglowSelectionKind,
    /// Model requested by an explicit selection, if any.
    pub requested_model: Option<AirglowModel>,
    /// Model resolved for evaluation when already known.
    pub resolved_model: Option<AirglowModel>,
    /// True when automatic policy deliberately fell back to a temporary model.
    pub used_automatic_fallback: bool,
    /// Typed reason when automatic policy selected a fallback model.
    pub fallback_reason: Option<AirglowFallbackReason>,
}

/// Evaluation-specific physical outcome known only after a query is evaluated.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct AirglowEvaluationOutcome {
    /// Physical evaluation outcome for the query.
    pub physical_outcome: AirglowPhysicalOutcome,
    /// Typed reason when [`AirglowPhysicalOutcome::PhysicalZero`].
    pub physical_zero_reason: Option<AirglowPhysicalZeroReason>,
}

/// Resolved Airglow model identity after applying selection policy.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ResolvedAirglowSelection {
    pub model: AirglowModel,
    pub selection_kind: AirglowSelectionKind,
    pub requested_model: Option<AirglowModel>,
    pub used_automatic_fallback: bool,
    pub fallback_reason: Option<AirglowFallbackReason>,
}

impl ResolvedAirglowSelection {
    pub(crate) fn selection_metadata(self) -> AirglowSelectionMetadata {
        AirglowSelectionMetadata {
            selection_kind: self.selection_kind,
            requested_model: self.requested_model,
            resolved_model: Some(self.model),
            used_automatic_fallback: self.used_automatic_fallback,
            fallback_reason: self.fallback_reason,
        }
    }

    pub(crate) fn evaluation_outcome(
        physical_outcome: AirglowPhysicalOutcome,
        physical_zero_reason: Option<AirglowPhysicalZeroReason>,
    ) -> AirglowEvaluationOutcome {
        AirglowEvaluationOutcome {
            physical_outcome,
            physical_zero_reason,
        }
    }
}

/// Resolve configuration selection to a concrete supported model.
///
/// Explicit selections never fall back. Automatic selection is deterministic and
/// currently admits only the temporary Paranal-derived planning fallback.
pub(crate) fn resolve_airglow_selection(
    selection: AirglowSelection,
) -> Result<ResolvedAirglowSelection> {
    match selection {
        AirglowSelection::Automatic => Ok(ResolvedAirglowSelection {
            model: AirglowModel::ParanalNollSkyCalcFors1,
            selection_kind: AirglowSelectionKind::Automatic,
            requested_model: None,
            used_automatic_fallback: true,
            fallback_reason: Some(AirglowFallbackReason::GlobalPlanningModelUnavailable),
        }),
        AirglowSelection::Explicit(model) => match model {
            AirglowModel::ParanalNollSkyCalcFors1 => Ok(ResolvedAirglowSelection {
                model,
                selection_kind: AirglowSelectionKind::Explicit,
                requested_model: Some(model),
                used_automatic_fallback: false,
                fallback_reason: None,
            }),
        },
    }
}

/// Load continuum assets for a resolved scientific model.
pub(crate) fn load_continuum_for_model(
    model: AirglowModel,
) -> Result<std::sync::Arc<super::calibration::AirglowContinuum>> {
    match model {
        AirglowModel::ParanalNollSkyCalcFors1 => Ok(std::sync::Arc::new(
            super::calibration::load_builtin_standard()?,
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn automatic_selection_uses_visible_temporary_fallback() {
        let resolved = resolve_airglow_selection(AirglowSelection::Automatic).unwrap();
        assert_eq!(resolved.model, AirglowModel::ParanalNollSkyCalcFors1);
        assert!(resolved.used_automatic_fallback);
        assert_eq!(
            resolved.fallback_reason,
            Some(AirglowFallbackReason::GlobalPlanningModelUnavailable)
        );
        assert_eq!(
            resolved.fallback_reason.unwrap().as_str(),
            "global-planning-model-unavailable"
        );
        assert_eq!(resolved.selection_kind, AirglowSelectionKind::Automatic);
    }

    #[test]
    fn explicit_paranal_selection_is_not_reported_as_fallback() {
        let resolved = resolve_airglow_selection(AirglowSelection::Explicit(
            AirglowModel::ParanalNollSkyCalcFors1,
        ))
        .unwrap();
        assert!(!resolved.used_automatic_fallback);
        assert_eq!(resolved.fallback_reason, None);
        assert_eq!(resolved.selection_kind, AirglowSelectionKind::Explicit);
        assert_eq!(
            resolved.requested_model,
            Some(AirglowModel::ParanalNollSkyCalcFors1)
        );
    }

    #[test]
    fn physical_zero_reason_identifier_is_stable() {
        assert_eq!(
            AirglowPhysicalZeroReason::OutsideAstronomicalNight.as_str(),
            "outside-astronomical-night"
        );
    }
}
