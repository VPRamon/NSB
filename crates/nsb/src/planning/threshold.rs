//! Threshold search orchestration and integrated radiance sampling.

use super::scan::{
    authoritative_above_threshold_periods, coalesce_periods, complement_periods,
    tt_mjd_period_to_utc, tt_mjd_to_utc_time, utc_period_to_tt_mjd,
};
use super::types::{
    PreparedThresholdQuery, SiteWindowContext, ThresholdQuery, ThresholdQueryResult,
};
use crate::error::Result;
use crate::evaluator::{ComponentMask, NsbEvaluator};
use qtty::radiometry::PhotonsPerSquareCentimeterNanosecondSteradian as BandPhotonRadiance;
#[cfg(not(feature = "window-search-diagnostics"))]
use rayon::prelude::*;
use siderust::qtty::Day;
use siderust::time::{Interval as TimePeriod, ModifiedJulianDate};
#[cfg(feature = "window-search-diagnostics")]
use std::time::Instant;

pub(crate) fn evaluate_integrated(
    evaluator: &NsbEvaluator,
    prepared: &PreparedThresholdQuery,
    mjd_tt: ModifiedJulianDate,
) -> Result<BandPhotonRadiance> {
    #[cfg(feature = "window-search-diagnostics")]
    super::diagnostics::update(|diagnostics| diagnostics.integrated_evaluations += 1);
    let time = tt_mjd_to_utc_time(mjd_tt);
    let mut total = BandPhotonRadiance::new(0.0);

    if prepared.components.contains(ComponentMask::ZODIACAL) {
        #[cfg(feature = "window-search-diagnostics")]
        super::diagnostics::update(|diagnostics| diagnostics.zodiacal_evaluations += 1);
        #[cfg(feature = "window-search-diagnostics")]
        let component_started = Instant::now();
        let out =
            evaluator
                .zodiacal()
                .compute_observed(time, prepared.observer, prepared.target)?;
        total += out.integrated;
        #[cfg(feature = "window-search-diagnostics")]
        super::diagnostics::update(|diagnostics| {
            diagnostics.exact_zodiacal_time += component_started.elapsed();
        });
    }
    if prepared.components.contains(ComponentMask::STARLIGHT) {
        total += prepared.starlight_integrated;
    }
    if prepared.components.contains(ComponentMask::AIRGLOW) {
        if let Some(phase) = super::filters::airglow_night_phase(prepared, mjd_tt) {
            #[cfg(feature = "window-search-diagnostics")]
            super::diagnostics::update(|diagnostics| diagnostics.airglow_evaluations += 1);
            #[cfg(feature = "window-search-diagnostics")]
            let component_started = Instant::now();
            let solar = prepared
                .solar_activity_cache
                .as_ref()
                .expect("solar activity cache is prepared with airglow")
                .value_at(time)?;
            total += prepared
                .airglow_model
                .as_ref()
                .expect("airglow model is prepared when airglow is selected")
                .compute_integrated_with_night_phase(time, prepared.target, phase, solar)?;
            #[cfg(feature = "window-search-diagnostics")]
            super::diagnostics::update(|diagnostics| {
                diagnostics.exact_airglow_time += component_started.elapsed();
            });
        }
    }
    if prepared.components.contains(ComponentMask::MOON) {
        let moon_visible = prepared
            .moon_visible_periods
            .as_ref()
            .is_none_or(|periods| super::filters::contains_time(periods, mjd_tt));
        if moon_visible {
            #[cfg(feature = "window-search-diagnostics")]
            super::diagnostics::update(|diagnostics| diagnostics.moonlight_evaluations += 1);
            #[cfg(feature = "window-search-diagnostics")]
            let component_started = Instant::now();
            let out = evaluator.evaluate_moonlight(prepared.observer, time, prepared.target)?;
            total += out.integrated;
            #[cfg(feature = "window-search-diagnostics")]
            super::diagnostics::update(|diagnostics| {
                diagnostics.exact_moonlight_time += component_started.elapsed();
            });
        }
    }

    Ok(total)
}

fn search_prepared(
    evaluator: &NsbEvaluator,
    query: &ThresholdQuery,
    prepared: PreparedThresholdQuery,
) -> Result<ThresholdQueryResult> {
    #[cfg(feature = "window-search-diagnostics")]
    super::diagnostics::begin_threshold_search();
    let step = query.sample_step.to::<Day>();
    #[cfg(feature = "window-search-diagnostics")]
    let search_started = Instant::now();
    #[cfg(feature = "window-search-diagnostics")]
    let mut darker_periods: Vec<TimePeriod<ModifiedJulianDate>> = Vec::new();
    #[cfg(feature = "window-search-diagnostics")]
    for window in super::filters::smooth_threshold_windows(&prepared) {
        let exact_f =
            |mjd_tt: ModifiedJulianDate| evaluate_integrated(evaluator, &prepared, mjd_tt);
        let brighter =
            authoritative_above_threshold_periods(window, step, &exact_f, query.threshold)?;
        darker_periods.extend(complement_periods(window, &brighter));
    }
    #[cfg(not(feature = "window-search-diagnostics"))]
    let mut darker_periods: Vec<TimePeriod<ModifiedJulianDate>> =
        super::filters::smooth_threshold_windows(&prepared)
            .into_par_iter()
            .map(|window| -> Result<Vec<TimePeriod<ModifiedJulianDate>>> {
                let exact_f =
                    |mjd_tt: ModifiedJulianDate| evaluate_integrated(evaluator, &prepared, mjd_tt);
                let brighter =
                    authoritative_above_threshold_periods(window, step, &exact_f, query.threshold)?;
                Ok(complement_periods(window, &brighter))
            })
            .collect::<Result<Vec<_>>>()?
            .into_iter()
            .flatten()
            .collect();
    coalesce_periods(&mut darker_periods);
    #[cfg(feature = "window-search-diagnostics")]
    super::diagnostics::update(|diagnostics| {
        diagnostics.threshold_search += search_started.elapsed();
    });

    Ok(ThresholdQueryResult {
        threshold: query.threshold,
        periods: darker_periods
            .into_iter()
            .map(|p| tt_mjd_period_to_utc(p, prepared.tt_window, query.window))
            .collect(),
    })
}

/// Find filtered UTC periods whose integrated radiance is at or below the threshold.
pub fn periods_below_threshold(
    evaluator: &NsbEvaluator,
    query: &ThresholdQuery,
) -> Result<ThresholdQueryResult> {
    let context = prepare_site_window_context(evaluator, query)?;
    periods_below_threshold_with_context(evaluator, &context, query)
}

/// Prepare target-independent ephemeris, filter, and model state for reuse.
pub fn prepare_site_window_context(
    evaluator: &NsbEvaluator,
    query: &ThresholdQuery,
) -> Result<SiteWindowContext> {
    super::prepare::validate_threshold(query)?;

    let tt_window = utc_period_to_tt_mjd(query.window);
    #[cfg(feature = "window-search-diagnostics")]
    let preparation_started = Instant::now();
    let context = super::prepare::prepare_site_context(evaluator, query, tt_window)?;
    #[cfg(feature = "window-search-diagnostics")]
    super::diagnostics::update(|diagnostics| {
        diagnostics.threshold_preparation += preparation_started.elapsed();
    });
    Ok(context)
}

/// Search using target-independent preparation created for this evaluator.
pub fn periods_below_threshold_with_context(
    evaluator: &NsbEvaluator,
    context: &SiteWindowContext,
    query: &ThresholdQuery,
) -> Result<ThresholdQueryResult> {
    super::prepare::validate_threshold(query)?;
    super::prepare::validate_site_context(evaluator, context, query)?;
    #[cfg(feature = "window-search-diagnostics")]
    let preparation_started = Instant::now();
    let prepared = super::prepare::prepare_target_threshold(evaluator, context, query)?;
    #[cfg(feature = "window-search-diagnostics")]
    super::diagnostics::update(|diagnostics| {
        diagnostics.threshold_preparation += preparation_started.elapsed();
        diagnostics.candidate_windows += prepared.candidate_windows.len();
    });
    search_prepared(evaluator, query, prepared)
}

/// Run one search while collecting compile-time-gated benchmark diagnostics.
#[cfg(feature = "window-search-diagnostics")]
pub fn periods_below_threshold_diagnosed(
    evaluator: &NsbEvaluator,
    query: &ThresholdQuery,
) -> Result<(ThresholdQueryResult, super::WindowSearchDiagnostics)> {
    let (_, result, diagnostics) = prepare_and_search_diagnosed(evaluator, query)?;
    Ok((result, diagnostics))
}

/// Prepare a reusable context and run one diagnosed search.
#[cfg(feature = "window-search-diagnostics")]
pub fn prepare_and_search_diagnosed(
    evaluator: &NsbEvaluator,
    query: &ThresholdQuery,
) -> Result<(
    SiteWindowContext,
    ThresholdQueryResult,
    super::WindowSearchDiagnostics,
)> {
    super::diagnostics::reset();
    let context = prepare_site_window_context(evaluator, query)?;
    let result = periods_below_threshold_with_context(evaluator, &context, query)?;
    Ok((context, result, super::diagnostics::snapshot()))
}

impl NsbEvaluator {
    /// Find filtered UTC periods whose integrated radiance is at or below the threshold.
    pub fn periods_below_threshold(&self, query: &ThresholdQuery) -> Result<ThresholdQueryResult> {
        periods_below_threshold(self, query)
    }

    /// Prepare target-independent ephemeris, filter, and model state for reuse.
    pub fn prepare_site_window_context(&self, query: &ThresholdQuery) -> Result<SiteWindowContext> {
        prepare_site_window_context(self, query)
    }

    /// Search using target-independent preparation created for this evaluator.
    pub fn periods_below_threshold_with_context(
        &self,
        context: &SiteWindowContext,
        query: &ThresholdQuery,
    ) -> Result<ThresholdQueryResult> {
        periods_below_threshold_with_context(self, context, query)
    }

    /// Run one search while collecting compile-time-gated benchmark diagnostics.
    #[cfg(feature = "window-search-diagnostics")]
    pub fn periods_below_threshold_diagnosed(
        &self,
        query: &ThresholdQuery,
    ) -> Result<(ThresholdQueryResult, super::WindowSearchDiagnostics)> {
        periods_below_threshold_diagnosed(self, query)
    }

    /// Prepare a reusable context and run one diagnosed search.
    #[cfg(feature = "window-search-diagnostics")]
    pub fn prepare_and_search_diagnosed(
        &self,
        query: &ThresholdQuery,
    ) -> Result<(
        SiteWindowContext,
        ThresholdQueryResult,
        super::WindowSearchDiagnostics,
    )> {
        prepare_and_search_diagnosed(self, query)
    }
}
