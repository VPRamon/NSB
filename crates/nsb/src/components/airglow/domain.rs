//! Semantic domains used by the empirical Airglow calibration.

/// Empirical night-phase domain encoded by the SkyCalc-derived correction table.
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

/// Empirical seasonal domain encoded by the SkyCalc-derived correction table.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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
