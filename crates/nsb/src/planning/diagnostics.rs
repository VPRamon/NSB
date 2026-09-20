#![allow(missing_docs)]
use std::cell::Cell;
use std::time::Duration;

/// Test/benchmark-only counters and phase timings for one threshold search.
///
/// This type and its instrumentation are compiled only with the
/// `window-search-diagnostics` feature, so production builds pay no counter or
/// clock-reading overhead.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct WindowSearchDiagnostics {
    pub integrated_evaluations: usize,
    pub zodiacal_evaluations: usize,
    pub airglow_evaluations: usize,
    pub moonlight_evaluations: usize,
    pub exact_zodiacal_time: Duration,
    pub exact_airglow_time: Duration,
    pub exact_moonlight_time: Duration,
    pub candidate_windows: usize,
    pub authoritative_scan_windows: usize,
    pub threshold_crossings: usize,
    pub crossing_refinement_evaluations: usize,
    pub threshold_preparation: Duration,
    pub astronomical_night_preparation: Duration,
    pub sun_filtering: Duration,
    pub target_visibility: Duration,
    pub moon_visibility: Duration,
    pub threshold_search: Duration,
}

thread_local! {
    static CURRENT: Cell<WindowSearchDiagnostics> = Cell::new(WindowSearchDiagnostics::default());
}

pub(crate) fn reset() {
    CURRENT.set(WindowSearchDiagnostics::default());
}

pub(crate) fn snapshot() -> WindowSearchDiagnostics {
    CURRENT.get()
}

pub(crate) fn update(f: impl FnOnce(&mut WindowSearchDiagnostics)) {
    CURRENT.set({
        let mut diagnostics = CURRENT.get();
        f(&mut diagnostics);
        diagnostics
    });
}

pub(crate) fn begin_threshold_search() {
    update(|diagnostics| {
        diagnostics.integrated_evaluations = 0;
        diagnostics.zodiacal_evaluations = 0;
        diagnostics.airglow_evaluations = 0;
        diagnostics.moonlight_evaluations = 0;
        diagnostics.exact_zodiacal_time = Duration::ZERO;
        diagnostics.exact_airglow_time = Duration::ZERO;
        diagnostics.exact_moonlight_time = Duration::ZERO;
        diagnostics.authoritative_scan_windows = 0;
        diagnostics.threshold_crossings = 0;
        diagnostics.crossing_refinement_evaluations = 0;
    });
}
