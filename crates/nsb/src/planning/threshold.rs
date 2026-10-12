// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Vallés Puig, Ramon
//! Threshold search orchestration and integrated radiance sampling.

use super::adaptive::{discover, Controls};
use super::bounds::prepared_bounds;
use super::scan::{
    authoritative_above_threshold_periods, certified_tt_mjd_period_to_utc, coalesce_periods,
    complement_periods, tt_mjd_period_to_utc, tt_mjd_to_utc_time, utc_period_to_tt_mjd,
};
use super::types::{
    PreparedThresholdQuery, SiteWindowContext, ThresholdQuery, ThresholdQueryResult,
};
use crate::error::Result;
use crate::evaluator::{ComponentMask, NsbEvaluator};
use crate::units::radiometry::PhotonsPerSquareCentimeterNanosecondSteradian as BandPhotonRadiance;
use rayon::prelude::*;
use siderust::qtty::Day;
use siderust::time::{Interval as TimePeriod, ModifiedJulianDate};

pub(crate) fn evaluate_integrated(
    evaluator: &NsbEvaluator,
    prepared: &PreparedThresholdQuery,
    mjd_tt: ModifiedJulianDate,
) -> Result<BandPhotonRadiance> {
    let time = tt_mjd_to_utc_time(mjd_tt);
    let mut total = BandPhotonRadiance::new(0.0);

    if prepared.components.contains(ComponentMask::ZODIACAL) {
        let out =
            evaluator
                .zodiacal()
                .compute_observed(time, prepared.observer, prepared.target)?;
        total += out.integrated;
    }
    if prepared.components.contains(ComponentMask::STARLIGHT) {
        total += prepared.starlight_integrated;
    }
    if prepared.components.contains(ComponentMask::AIRGLOW) {
        let solar = prepared
            .solar_activity_cache
            .as_ref()
            .expect("solar activity cache is prepared with airglow")
            .value_at(time)?;
        let airglow = prepared
            .airglow_model
            .as_ref()
            .expect("airglow model is prepared when airglow is selected");
        if let Some(phase) = super::filters::airglow_night_phase(prepared, mjd_tt) {
            total +=
                airglow.compute_integrated_with_night_phase(time, prepared.target, phase, solar)?;
        } else {
            // Outside night contributes physical zero, but invalid inputs must
            // still fail rather than masquerading as inactivity (#151/#175).
            airglow.validate_inputs_for_query(time, prepared.target, solar)?;
        }
    }
    if prepared.components.contains(ComponentMask::MOON) {
        let moon_visible = prepared
            .moon_visible_periods
            .as_ref()
            .is_none_or(|periods| super::filters::contains_time(periods, mjd_tt));
        if moon_visible {
            let out = evaluator.evaluate_moonlight(prepared.observer, time, prepared.target)?;
            total += out.integrated;
        }
    }

    Ok(total)
}

fn search_prepared(
    evaluator: &NsbEvaluator,
    query: &ThresholdQuery,
    prepared: PreparedThresholdQuery,
    require_certification: bool,
) -> Result<ThresholdQueryResult> {
    let step = query.sample_step.to::<Day>();
    let controls = Controls {
        // Real-model jumps do not have smooth envelopes yet. Bounds may still
        // isolate Moon-down interiors, without fitting across those jumps.
        fit_depth: 0,
        max_evaluations: 0,
        ..Controls::default()
    };
    let windows = super::filters::smooth_threshold_windows(&prepared);
    let certificates = if require_certification {
        Some(discover(
            &windows,
            query.threshold,
            |t| evaluate_integrated(evaluator, &prepared, t),
            |w| prepared_bounds(&prepared, w),
            controls,
        )?)
    } else {
        None
    };
    debug_assert!(certificates.as_ref().is_none_or(|certificates| certificates
        .brackets
        .iter()
        .all(
            |b| (b.left > query.threshold) != (b.right > query.threshold)
                && (!b.tolerance_met || b.interval.length() <= controls.time_tolerance)
        )));
    let mut darker_periods: Vec<TimePeriod<ModifiedJulianDate>> = if let Some(certificates) =
        certificates
    {
        certificates.certified_periods()?
    } else {
        // Frozen sampled contract: use the original physical windows so the
        // grid phase is unchanged. Practical adaptive discovery has its own
        // explicit API and never falls back through this branch.
        windows
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
            .collect()
    };
    coalesce_periods(&mut darker_periods);

    Ok(ThresholdQueryResult {
        threshold: query.threshold,
        periods: darker_periods
            .into_iter()
            .map(|p| {
                if require_certification {
                    certified_tt_mjd_period_to_utc(p, prepared.tt_window, query.window)
                } else {
                    Ok(tt_mjd_period_to_utc(p, prepared.tt_window, query.window))
                }
            })
            .collect::<Result<Vec<_>>>()?,
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
    let context = super::prepare::prepare_site_context(evaluator, query, tt_window)?;
    Ok(context)
}

/// Search using target-independent preparation created for this evaluator.
pub fn periods_below_threshold_with_context(
    evaluator: &NsbEvaluator,
    context: &SiteWindowContext,
    query: &ThresholdQuery,
) -> Result<ThresholdQueryResult> {
    super::prepare::validate_threshold(query)?;
    super::prepare::validate_sample_step(query)?;
    super::prepare::validate_site_context(evaluator, context, query)?;
    let prepared = super::prepare::prepare_target_threshold(evaluator, context, query)?;
    search_prepared(evaluator, query, prepared, false)
}

fn periods_below_threshold_certified_with_context(
    evaluator: &NsbEvaluator,
    context: &SiteWindowContext,
    query: &ThresholdQuery,
) -> Result<ThresholdQueryResult> {
    super::prepare::validate_threshold(query)?;
    super::prepare::validate_site_context(evaluator, context, query)?;
    let prepared = super::prepare::prepare_target_threshold(evaluator, context, query)?;
    search_prepared(evaluator, query, prepared, true)
}

impl NsbEvaluator {
    /// Physics-partitioned, step-free adaptive threshold discovery.
    ///
    /// Uses authoritative radiances for all probes and crossing refinement
    /// (0.1 s brackets). Assumes the piecewise physical signal is resolved by
    /// independently checked degree-4/8 fits and local extrema searches.
    /// This is an empirically validated discovery contract, NOT a continuous
    /// completeness certificate. Arbitrarily hidden sub-node excursions remain
    /// possible. Resource/numerical failures return an error, never a partial
    /// schedule. `sample_step` is ignored, including its legacy validation.
    /// Existing sampled methods keep their frozen discrete contract.
    pub fn periods_below_threshold_adaptive(
        &self,
        query: &ThresholdQuery,
    ) -> Result<ThresholdQueryResult> {
        let context = prepare_site_window_context(self, query)?;
        self.periods_below_threshold_adaptive_with_context(&context, query)
    }

    /// Adaptive discovery reusing a compatible target-independent context.
    /// See [`Self::periods_below_threshold_adaptive`] for the coverage contract.
    pub fn periods_below_threshold_adaptive_with_context(
        &self,
        context: &SiteWindowContext,
        query: &ThresholdQuery,
    ) -> Result<ThresholdQueryResult> {
        super::prepare::validate_threshold(query)?;
        super::prepare::validate_site_context(self, context, query)?;
        let prepared = super::prepare::prepare_target_threshold(self, context, query)?;
        let windows = super::filters::adaptive_threshold_windows(&prepared)?;
        if windows.len() > 25_000 {
            return Err(crate::NsbError::Unsupported(
                "adaptive initial physical interval budget (25000)".into(),
            ));
        }
        let parts = windows
            .into_par_iter()
            .map(|w| {
                super::discovery::search(
                    w,
                    query.threshold,
                    |t| evaluate_integrated(self, &prepared, t),
                    prepared_bounds(&prepared, w)?,
                    super::discovery::Limits::default(),
                )
                .map(|r| {
                    log::trace!("adaptive window: bounds={}, costs={:?}", r.1.bounds, r.1);
                    r.0
                })
            })
            .collect::<Result<Vec<_>>>()?;
        let mut periods: Vec<_> = parts.into_iter().flatten().collect();
        coalesce_periods(&mut periods);
        Ok(ThresholdQueryResult {
            threshold: query.threshold,
            periods: periods
                .into_iter()
                .map(|p| certified_tt_mjd_period_to_utc(p, prepared.tt_window, query.window))
                .collect::<Result<_>>()?,
        })
    }

    /// Return a complete, interval-certified schedule or an explicit error.
    ///
    /// This method never uses `sample_step` or a scan fallback for discovery.
    /// Currently certificates cover cached Starlight, the empty sum, and Moon
    /// skipped outside the prepared visibility set (conditional on Siderust's
    /// events). Active Zodiacal, Airglow and either active Moonlight model lack
    /// validated temporal envelopes and return [`crate::NsbError::Unsupported`].
    /// Resource or numerical uncertainty also prevents a successful result.
    /// Native UTC endpoints must round-trip inside their TT certificates;
    /// boundaries may move inward by at most 0.1 s. Output adapters own their
    /// representation restrictions (including Python datetime's year range).
    /// An unrepresentable boundary returns Unsupported.
    /// The existing sampled methods retain their discrete discovery contract.
    pub fn periods_below_threshold_certified(
        &self,
        query: &ThresholdQuery,
    ) -> Result<ThresholdQueryResult> {
        let context = prepare_site_window_context(self, query)?;
        periods_below_threshold_certified_with_context(self, &context, query)
    }

    /// Complete certified search with a compatible reusable site context.
    ///
    /// Has the same guarantees and current component limitations as
    /// [`Self::periods_below_threshold_certified`]. Incomplete searches return
    /// an error, never a partial or empty schedule presented as complete.
    pub fn periods_below_threshold_certified_with_context(
        &self,
        context: &SiteWindowContext,
        query: &ThresholdQuery,
    ) -> Result<ThresholdQueryResult> {
        periods_below_threshold_certified_with_context(self, context, query)
    }
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
}
