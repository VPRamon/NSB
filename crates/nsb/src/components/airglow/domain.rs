//! Legacy night/season domains retained for query-window partitioning and
//! historical validation. PALACE runtime climatology uses month/hour bins.

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

/// Historical double-month season retained for diagnostic tests.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)]
pub(crate) enum AirglowSeason {
    /// Full-year aggregate correction.
    FullYear,
    /// December and January.
    DecJan,
    /// February and March.
    FebMar,
    /// April and May.
    AprMay,
    /// June and July.
    JunJul,
    /// August and September.
    AugSep,
    /// October and November.
    OctNov,
}
