//! Public access to build-verified scientific asset metadata.
//!
//! Runtime ownership lives in the internal `data::bundled` module so data
//! loading and metadata stay grouped by responsibility. This module preserves
//! the established public API surface.

pub use crate::data::bundled::*;
