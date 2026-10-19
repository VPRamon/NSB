// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Vallés Puig, Ramon
//! Atmospheric transport of spectral radiance.
//!
//! NSB-owned orchestration between source emission and instrument-independent
//! ground-level spectral radiance:
//!
//! ```text
//! source spectral radiance
//!     ↓
//! atmospheric transport (identity | direct extinction)
//!     ↓
//! ground spectral radiance
//!     ↓
//! future integration / instrument response (#189)
//! ```
//!
//! # Supported in this foundation
//!
//! - `TransportModel::Identity` — exact pass-through
//! - `TransportModel::Direct` — wavelength-dependent Beer–Lambert extinction
//!
//! Propagation always requires a `RadianceOrigin` so celestial direct
//! transmission cannot be applied to airglow or pre-scattered moonlight.
//!
//! # Not part of the supported public API yet
//!
//! The opt-in [`single`](crate::transport::single) reference computes direct and first-order scattering.
//! It is experimental and does not change evaluator defaults. HEALPix sampling
//! and LUT solvers remain future work.
//!
//! # Ownership
//!
//! Siderust owns airmass, Rayleigh/Mie optical depth, Beer–Lambert transmission,
//! ozone tables, and generic phase functions. NSB owns transport selection,
//! origin policy, and model metadata.

mod direct;
mod extinction;
mod geometry;
mod metadata;
mod model;
mod origin;

/// Experimental opt-in scalar single-scattering reference; no evaluator migration.
pub mod single;

/// Experimental opt-in scalar full-sphere multiple-scattering reference.
pub mod multiple;

pub use direct::DirectTransmission;
pub use extinction::{ExtinctionIngredients, MolecularAbsorption, OpticalDepthBreakdown};
pub use geometry::DirectPathGeometry;
pub use metadata::TransportModelMetadata;
pub use model::{AirmassModel, TransportModel};
pub use origin::RadianceOrigin;
