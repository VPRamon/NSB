//! Airglow model-selection contract.
//!
//! Distinguishes automatic/default selection policy from explicit caller choice.
//! The global climatological planning model from #157 is deferred; automatic
//! selection currently resolves to a temporary Paranal-derived planning
//! fallback with that fallback machine-visible in result metadata.

use super::model::AirglowModel;
use crate::error::{NsbError, Result};
use std::sync::Arc;

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
    /// reported in result metadata and is not presented as a globally
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

/// Machine-readable Airglow selection and outcome report attached to results.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct AirglowSelectionReport {
    /// Automatic versus explicit configuration choice.
    pub selection_kind: AirglowSelectionKind,
    /// Model requested by an explicit selection, if any.
    pub requested_model: Option<AirglowModel>,
    /// Model actually used for evaluation.
    pub resolved_model: AirglowModel,
    /// True when automatic policy deliberately fell back to a temporary model.
    pub used_automatic_fallback: bool,
    /// Why automatic policy selected a fallback model, when applicable.
    pub fallback_reason: Option<&'static str>,
    /// Physical evaluation outcome for the query.
    pub physical_outcome: AirglowPhysicalOutcome,
    /// Machine-readable reason when [`AirglowPhysicalOutcome::PhysicalZero`].
    pub physical_zero_reason: Option<&'static str>,
}

/// Reason string for the release-scoped temporary automatic fallback.
pub const TEMPORARY_AUTOMATIC_FALLBACK_REASON: &str = concat!(
    "global climatological planning model not yet admitted (#157 deferred); ",
    "using temporary Paranal-derived Noll/SkyCalc/FORS1 planning fallback"
);

/// Reason string for physical zero outside the astronomical-night domain.
pub const PHYSICAL_ZERO_OUTSIDE_ASTRONOMICAL_NIGHT: &str =
    "outside astronomical night; Airglow continuum is physically inactive";

/// Resolved Airglow model identity after applying selection policy.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ResolvedAirglowSelection {
    pub model: AirglowModel,
    pub selection_kind: AirglowSelectionKind,
    pub requested_model: Option<AirglowModel>,
    pub used_automatic_fallback: bool,
    pub fallback_reason: Option<&'static str>,
}

impl ResolvedAirglowSelection {
    pub(crate) fn report(
        self,
        physical_outcome: AirglowPhysicalOutcome,
        physical_zero_reason: Option<&'static str>,
    ) -> AirglowSelectionReport {
        AirglowSelectionReport {
            selection_kind: self.selection_kind,
            requested_model: self.requested_model,
            resolved_model: self.model,
            used_automatic_fallback: self.used_automatic_fallback,
            fallback_reason: self.fallback_reason,
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
            fallback_reason: Some(TEMPORARY_AUTOMATIC_FALLBACK_REASON),
        }),
        AirglowSelection::Explicit(model) => match model {
            AirglowModel::ParanalNollSkyCalcFors1 => Ok(ResolvedAirglowSelection {
                model,
                selection_kind: AirglowSelectionKind::Explicit,
                requested_model: Some(model),
                used_automatic_fallback: false,
                fallback_reason: None,
            }),
            // Reserved / not-yet-admitted models fail rather than silently
            // substituting the Paranal fallback or any other model.
            AirglowModel::GlobalClimatology => Err(NsbError::Unsupported(format!(
                "explicitly selected Airglow model '{}' is not yet admitted; \
                 the global climatological model remains deferred (#157)",
                model.as_str()
            ))),
        },
    }
}

/// Load continuum assets for a resolved scientific model.
pub(crate) fn load_continuum_for_model(
    model: AirglowModel,
) -> Result<Arc<super::calibration::AirglowContinuum>> {
    match model {
        AirglowModel::ParanalNollSkyCalcFors1 => {
            Ok(Arc::new(super::calibration::load_builtin_standard()?))
        }
        AirglowModel::GlobalClimatology => Err(NsbError::Unsupported(format!(
            "Airglow model '{}' has no admitted continuum asset in this build",
            model.as_str()
        ))),
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
            Some(TEMPORARY_AUTOMATIC_FALLBACK_REASON)
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
    fn unsupported_explicit_model_does_not_silently_fall_back() {
        let error =
            resolve_airglow_selection(AirglowSelection::Explicit(AirglowModel::GlobalClimatology))
                .expect_err("unadmitted explicit model must fail");
        assert!(
            matches!(error, NsbError::Unsupported(message) if message.contains("global-climatology"))
        );
    }
}
