//! Scientific metadata describing an atmospheric transport configuration.
//!
//! Transport sophistication must never silently upgrade component maturity or
//! site [`crate::CalibrationStatus`].

/// Which transport path was evaluated.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum TransportPathKind {
    /// Exact pass-through; atmosphere ignored.
    Identity,
    /// Direct Beer–Lambert extinction only.
    Direct,
    /// Future single in-scattering contribution.
    Scattered,
}

impl TransportPathKind {
    /// Stable machine-readable identifier.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Identity => "identity",
            Self::Direct => "direct",
            Self::Scattered => "scattered",
        }
    }
}

/// Molecular absorption treatment recorded in metadata.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum AbsorptionTreatment {
    /// No molecular absorption term.
    None,
    /// Bundled Siderust ozone transmittance table.
    OzoneBundledTable,
}

impl AbsorptionTreatment {
    /// Stable machine-readable identifier.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::OzoneBundledTable => "ozone-bundled-table",
        }
    }
}

/// Extinction ingredients present in a transport evaluation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub struct ExtinctionIngredientFlags {
    /// Rayleigh scattering optical depth included.
    pub rayleigh: bool,
    /// Mie / aerosol optical depth included.
    pub mie: bool,
    /// Molecular absorption treatment.
    pub absorption: AbsorptionTreatment,
}

/// Scattering ingredients present in a transport evaluation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub struct ScatteringIngredientFlags {
    /// Rayleigh phase function used (scattered path).
    pub rayleigh_phase: bool,
    /// Mie / aerosol phase function used (scattered path).
    pub mie_phase: bool,
}

/// Approximation / extrapolation state for the transport evaluation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum ApproximationState {
    /// Numerically exact within the typed representation (identity).
    ExactWithinRepresentation,
    /// Clear-sky single-column Beer–Lambert; no multiple scattering.
    ClearSkySingleColumn,
    /// Reserved for future approximate LUT / HEALPix solvers.
    AcceleratedApproximation,
}

impl ApproximationState {
    /// Stable machine-readable identifier.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::ExactWithinRepresentation => "exact-within-representation",
            Self::ClearSkySingleColumn => "clear-sky-single-column",
            Self::AcceleratedApproximation => "accelerated-approximation",
        }
    }
}

/// Uncertainty reporting policy for transport.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum UncertaintyReporting {
    /// No quantitative transport uncertainty is claimed.
    Absent,
}

impl UncertaintyReporting {
    /// Stable machine-readable identifier.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Absent => "absent",
        }
    }
}

/// Inspectable scientific description of a transport configuration.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub struct TransportMetadata {
    /// Transport model identity (`identity`, `direct-transmission`, …).
    pub model_id: &'static str,
    /// Direct / identity / scattered path kind.
    pub path_kind: TransportPathKind,
    /// Caller-supplied atmosphere/profile identity string.
    pub atmosphere_profile_id: &'static str,
    /// Extinction ingredients.
    pub extinction: ExtinctionIngredientFlags,
    /// Scattering ingredients.
    pub scattering: ScatteringIngredientFlags,
    /// Airmass model identity (or `none` for identity transport).
    pub airmass_model_id: &'static str,
    /// Approximation / extrapolation state.
    pub approximation: ApproximationState,
    /// Human-readable validated domain statement.
    pub validated_domain: &'static str,
    /// Provenance statement.
    pub provenance: &'static str,
    /// Uncertainty reporting policy.
    pub uncertainty: UncertaintyReporting,
}
