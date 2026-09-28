//! Atmospheric transport of spectral radiance.
//!
//! This module is the NSB-owned orchestration layer between celestial (or
//! atmospheric) source emission and instrument-independent ground-level
//! radiance:
//!
//! ```text
//! sky emission
//!     ↓
//! atmospheric transport
//!     ↓
//! ground-level spectral radiance
//!     ↓
//! future instrument response (#189)
//! ```
//!
//! # Ownership boundary with Siderust
//!
//! Siderust owns generic atmospheric primitives: airmass formulas, Bodhaine
//! Rayleigh optical depth, Patat Mie optical depth, Beer–Lambert
//! transmission, ozone transmittance tables, Rayleigh and tabulated phase
//! functions, and [`siderust::atmosphere::AtmosphereProfile`].
//!
//! NSB owns:
//! - typed transport model selection (`TransportModel`);
//! - composition of those primitives into direct / identity / (future)
//!   scattered paths;
//! - scientific metadata and provenance for transport choices;
//! - mapping component radiance through transport without erasing source
//!   model identity.
//!
//! This module deliberately does **not** re-implement Rayleigh, Mie, ozone,
//! or airmass kernels. Missing generic primitives belong upstream in
//! Siderust before any NSB-local substitute is introduced.
//!
//! # Component migration status
//!
//! | Component | Uses this layer at runtime? | Notes |
//! | --- | --- | --- |
//! | Zodiacal | No (legacy [`crate::ZodiacalExtinction`]) | Source ⊥ propagation already; migration is a follow-up |
//! | Starlight | No by default | Helpers can propagate admitted TOA radiance without mutating the map product |
//! | Moonlight | No | Jones/KS91 retain validated embedded scattering; do not double-count |
//! | Airglow | No | In-atmosphere emission; see `RadianceOrigin::AtmosphericEmission` |
//!
//! # First implementation
//!
//! - `TransportModel::Identity` — exact pass-through.
//! - `TransportModel::Direct` — Beer–Lambert direct transmission with
//!   selectable Rayleigh / Mie / ozone ingredients.
//! - Explicit scaffolding for single in-scattering (`ScatteredPath` /
//!   `ScatteringPathStatus`) without shipping an incomplete all-sky RT engine.

mod apply;
mod direct;
mod extinction;
mod geometry;
mod identity;
mod metadata;
mod model;
mod origin;
mod scattering;

pub use apply::{apply_monochromatic, apply_spectral_radiance};
pub use direct::DirectTransmission;
pub use extinction::{ExtinctionIngredients, MolecularAbsorption, OpticalDepthBreakdown};
pub use geometry::{DirectPathGeometry, ScatteringGeometry};
pub use identity::IdentityTransport;
pub use metadata::{
    AbsorptionTreatment, ApproximationState, ExtinctionIngredientFlags, ScatteringIngredientFlags,
    TransportMetadata, TransportPathKind, UncertaintyReporting,
};
pub use model::{AirmassModel, TransportModel};
pub use origin::RadianceOrigin;
pub use scattering::{
    rayleigh_phase_value, ScatteredPath, ScatteringPathStatus, SingleScatteringNotImplemented,
};
