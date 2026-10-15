// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Vallés Puig, Ramon
//! Astronomical-night domains for query-window partitioning.
//! PALACE runtime climatology uses month/hour bins.

/// Astronomical-night phase used by the window-search partitioning contract.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum AirglowNightPhase {
    /// Full astronomical night, used when a continuous night cannot be bounded by
    /// both `-18°` solar-altitude crossings.
    FullNight,
    /// First third of a bounded astronomical night.
    FirstThird,
    /// Middle third of a bounded astronomical night.
    MiddleThird,
    /// Final third of a bounded astronomical night.
    LastThird,
}
