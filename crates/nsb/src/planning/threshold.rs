// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Vallés Puig, Ramon
//! Threshold search orchestration and integrated radiance sampling.

use super::bounds::prepared_bounds;
use super::intervals::{inward_tt_mjd_period_to_utc, tt_mjd_to_utc_time, utc_period_to_tt_mjd};
use super::types::{
    PreparedThresholdQuery, SiteWindowContext, ThresholdQuery, ThresholdQueryResult,
};
use crate::error::Result;
use crate::evaluator::{ComponentMask, NsbEvaluator};
use crate::units::radiometry::PhotonsPerSquareCentimeterNanosecondSteradian as BandPhotonRadiance;
use siderust::time::ModifiedJulianDate;

pub(crate) fn evaluate_integrated(
    evaluator: &NsbEvaluator,
    prepared: &PreparedThresholdQuery,
    mjd_tt: ModifiedJulianDate,
) -> Result<BandPhotonRadiance> {
    let time = tt_mjd_to_utc_time(mjd_tt);
    let mut total = BandPhotonRadiance::new(0.0);

    if prepared.components.contains(ComponentMask::ZODIACAL) {
        let out = evaluator.zodiacal().compute_with_longitude(
            time,
            prepared.observer,
            prepared.target,
            prepared.astronomy.solar(time),
        )?;
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
            let sky = prepared.astronomy.moon(time);
            let out = evaluator.evaluate_moonlight_with_sky(
                prepared.observer,
                time,
                prepared.target,
                &sky,
            )?;
            total += out.integrated;
        }
    }

    Ok(total)
}

/// Find filtered UTC periods with step-free adaptive discovery.
/// See [`NsbEvaluator::periods_below_threshold_adaptive`] for the empirical contract.
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
    super::prepare::validate_query(query)?;

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
    evaluator.search_radiance_band(context, query, None, false)
}

fn periods_below_threshold_certified_with_context(
    evaluator: &NsbEvaluator,
    context: &SiteWindowContext,
    query: &ThresholdQuery,
) -> Result<ThresholdQueryResult> {
    evaluator.search_radiance_band(context, query, None, true)
}

impl NsbEvaluator {
    /// Physics-partitioned, step-free adaptive threshold discovery.
    ///
    /// Uses authoritative radiances for all probes and crossing refinement
    /// (0.1 s brackets). Assumes the piecewise physical signal is resolved by
    /// degree-4 fits checked at every interlaced and independent interior node,
    /// upgraded degree-8 fits and local extrema searches.
    /// This is an empirically validated discovery contract, NOT a continuous
    /// completeness certificate. Arbitrarily hidden sub-node excursions remain
    /// possible. Resource/numerical failures return an error, never a partial
    /// schedule. `sample_step` is ignored, including its legacy validation.
    /// Normal threshold methods use this same engine.
    pub fn periods_below_threshold_adaptive(
        &self,
        query: &ThresholdQuery,
    ) -> Result<ThresholdQueryResult> {
        self.periods_below_threshold(query)
    }

    /// Adaptive discovery reusing a compatible target-independent context.
    /// See [`Self::periods_below_threshold_adaptive`] for the coverage contract.
    pub fn periods_below_threshold_adaptive_with_context(
        &self,
        context: &SiteWindowContext,
        query: &ThresholdQuery,
    ) -> Result<ThresholdQueryResult> {
        periods_below_threshold_with_context(self, context, query)
    }

    /// Step-free discovery of the inclusive band `minimum <= NSB <= query.threshold`.
    /// Both thresholds share the physical fit, exact probes and uncertainty
    /// brackets. Has the same empirical discovery limits as adaptive search.
    pub fn periods_in_band_adaptive(
        &self,
        query: &ThresholdQuery,
        minimum: BandPhotonRadiance,
    ) -> Result<ThresholdQueryResult> {
        let context = prepare_site_window_context(self, query)?;
        self.periods_in_band_adaptive_with_context(&context, query, minimum)
    }

    /// Adaptive radiance band with reusable site/window state.
    pub fn periods_in_band_adaptive_with_context(
        &self,
        context: &SiteWindowContext,
        query: &ThresholdQuery,
        minimum: BandPhotonRadiance,
    ) -> Result<ThresholdQueryResult> {
        self.search_radiance_band(context, query, Some(minimum), false)
    }

    /// Interval-certified inclusive band. Unsupported active-model enclosures
    /// fail explicitly; adaptive evidence never substitutes for a certificate.
    pub fn periods_in_band_certified_with_context(
        &self,
        context: &SiteWindowContext,
        query: &ThresholdQuery,
        minimum: BandPhotonRadiance,
    ) -> Result<ThresholdQueryResult> {
        self.search_radiance_band(context, query, Some(minimum), true)
    }

    fn search_radiance_band(
        &self,
        context: &SiteWindowContext,
        query: &ThresholdQuery,
        minimum: Option<BandPhotonRadiance>,
        certified: bool,
    ) -> Result<ThresholdQueryResult> {
        super::prepare::validate_query(query)?;
        if minimum.is_some_and(|v| !v.is_finite() || v.value() < 0.0 || v > query.threshold) {
            return Err(crate::NsbError::OutOfRange(
                "invalid minimum radiance".into(),
            ));
        }
        super::prepare::validate_site_context(self, context, query)?;
        let prepared = super::prepare::prepare_target_threshold(self, context, query)?;
        let windows = super::filters::adaptive_threshold_windows(&prepared)?;
        let (periods, costs) = super::discovery::search_windows(
            &windows,
            query.threshold,
            minimum,
            |t| evaluate_integrated(self, &prepared, t),
            |w| prepared_bounds(&prepared, w),
            certified,
        )?;
        let periods = periods
            .into_iter()
            .map(|p| inward_tt_mjd_period_to_utc(p, prepared.tt_window, query.window))
            .collect::<Result<_>>()?;
        super::prepare::validate_site_context(self, context, query)?;
        log::trace!("adaptive query costs: {costs:?}");
        Ok(ThresholdQueryResult {
            threshold: query.threshold,
            periods,
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
    /// Normal threshold methods use empirical adaptive discovery instead.
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
    /// Find filtered UTC periods with step-free adaptive discovery.
    /// See [`NsbEvaluator::periods_below_threshold_adaptive`] for the empirical contract.
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
