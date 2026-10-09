// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Vallés Puig, Ramon
//! Serde adapters for externally defined photometric types.
//!
//! The physical types remain owned by QTTY; these adapters only encode their
//! existing scalar representation for NSB consumers using Serde.

/// Serialize or deserialize QTTY surface brightness in mag/arcsec² as a JSON number.
///
/// QTTY owns `SurfaceBrightness`, so NSB cannot implement Serde's traits on
/// it directly. Annotate an application field with
/// `#[serde(with = "nsb::serde_support::surface_brightness")]` instead.
///
/// This adapter preserves the numeric wire representation; it intentionally
/// does not impose application-specific positivity, finiteness, or range rules.
pub mod surface_brightness {
    use crate::units::photometry::SurfaceBrightness;
    use serde::{Deserialize, Deserializer, Serializer};

    /// Serialize a surface brightness as its underlying mag/arcsec² scalar.
    pub fn serialize<S>(value: &SurfaceBrightness, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_f64(value.value())
    }

    /// Deserialize a numeric mag/arcsec² scalar as `SurfaceBrightness`.
    pub fn deserialize<'de, D>(deserializer: D) -> Result<SurfaceBrightness, D::Error>
    where
        D: Deserializer<'de>,
    {
        f64::deserialize(deserializer).map(SurfaceBrightness::new)
    }
}
