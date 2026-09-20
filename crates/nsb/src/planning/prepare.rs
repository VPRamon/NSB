//! Target-independent and target-specific preparation for window searches.

use super::types::{PreparedThresholdQuery, SiteWindowContext, ThresholdQuery};
use crate::components::airglow;
use crate::error::{NsbError, Result};
use crate::evaluator::{ComponentMask, NsbEvaluator};
use qtty::radiometry::PhotonsPerSquareCentimeterNanosecondSteradian as BandPhotonRadiance;
use qtty::Second;
use siderust::bodies::Moon as MoonBody;
use siderust::coordinates::spherical::direction;
use siderust::event::altitude::{above_threshold as altitude_above_threshold, SearchOpts};
use siderust::qtty::Degrees;
use siderust::time::{intersect_periods, Interval as TimePeriod, ModifiedJulianDate};
use std::sync::Arc;
#[cfg(feature = "window-search-diagnostics")]
use std::time::Instant;

pub(crate) fn validate_threshold(query: &ThresholdQuery) -> Result<()> {
    if !query.threshold.is_finite() {
        return Err(NsbError::OutOfRange("threshold must be finite".to_string()));
    }
    if !query.sample_step.is_finite() || query.sample_step <= Second::new(0.0) {
        return Err(NsbError::OutOfRange(
            "sample_step must be finite and greater than zero".to_string(),
        ));
    }
    if query.window.start > query.window.end {
        return Err(NsbError::OutOfRange(
            "query window start must not be after end".to_string(),
        ));
    }
    Ok(())
}

pub(crate) fn prepare_site_context(
    evaluator: &NsbEvaluator,
    query: &ThresholdQuery,
    tt_window: TimePeriod<ModifiedJulianDate>,
) -> Result<SiteWindowContext> {
    let uses_airglow = query.components.contains(ComponentMask::AIRGLOW);
    let uses_moon = query.components.contains(ComponentMask::MOON);
    let prepare_nights = || {
        if uses_airglow {
            airglow::temporal::astronomical_nights_for_window(tt_window, query.observer)
        } else {
            Vec::new()
        }
    };
    let prepare_moon = || {
        uses_moon.then(|| {
            altitude_above_threshold(
                &MoonBody,
                &query.observer,
                tt_window,
                Degrees::new(0.0),
                SearchOpts::default(),
            )
        })
    };
    #[cfg(feature = "window-search-diagnostics")]
    let (astronomical_night_periods, moon_visible_periods) = {
        let phase_started = Instant::now();
        let nights = prepare_nights();
        let night_elapsed = phase_started.elapsed();
        let phase_started = Instant::now();
        let moon = prepare_moon();
        let moon_elapsed = phase_started.elapsed();
        super::diagnostics::update(|diagnostics| {
            diagnostics.astronomical_night_preparation += night_elapsed;
            diagnostics.moon_visibility += moon_elapsed;
        });
        (nights, moon)
    };
    #[cfg(not(feature = "window-search-diagnostics"))]
    let (astronomical_night_periods, moon_visible_periods) =
        rayon::join(prepare_nights, prepare_moon);
    #[cfg(feature = "window-search-diagnostics")]
    let phase_started = Instant::now();
    let sun_filter_periods = match query.sun_altitude_ceiling {
        Some(sun_max) if uses_airglow && airglow::temporal::is_astronomical_twilight(sun_max) => {
            airglow::temporal::clipped_night_periods(&astronomical_night_periods, tt_window)
        }
        Some(sun_max) => {
            airglow::temporal::sun_below_threshold_periods(tt_window, query.observer, sun_max)
        }
        None => vec![tt_window],
    };
    #[cfg(feature = "window-search-diagnostics")]
    super::diagnostics::update(|diagnostics| {
        diagnostics.sun_filtering += phase_started.elapsed();
    });
    let airglow_phase_periods = if uses_airglow {
        airglow::temporal::airglow_phase_periods_for_window(&astronomical_night_periods, tt_window)
    } else {
        Vec::new()
    };
    let airglow_model = uses_airglow.then(|| {
        let profile = evaluator
            .model_config()
            .site_profile
            .profile(query.observer);
        airglow::Airglow::with_shared_continuum(
            query.observer,
            Arc::clone(evaluator.airglow_continuum()),
        )
        .with_atmosphere(profile.atmosphere)
        .with_geometry(evaluator.model_config().airglow_geometry.clone())
        .with_scale(profile.airglow.scale)
    });
    let solar_activity_cache = uses_airglow
        .then(|| {
            crate::solar_activity::SolarActivityValueCache::new(
                &evaluator.model_config().solar_activity,
            )
        })
        .transpose()?;
    Ok(SiteWindowContext {
        evaluator_identity: Arc::clone(evaluator.identity()),
        observer: query.observer,
        window: query.window,
        components: query.components,
        sun_altitude_ceiling: query.sun_altitude_ceiling,
        tt_window,
        sun_filter_periods: Arc::from(sun_filter_periods),
        astronomical_night_periods: Arc::from(astronomical_night_periods),
        airglow_phase_periods: Arc::from(airglow_phase_periods),
        airglow_model,
        solar_activity_cache,
        moon_visible_periods: moon_visible_periods.map(Arc::from),
    })
}

pub(crate) fn validate_site_context(
    evaluator: &NsbEvaluator,
    context: &SiteWindowContext,
    query: &ThresholdQuery,
) -> Result<()> {
    if !Arc::ptr_eq(&context.evaluator_identity, evaluator.identity()) {
        return Err(NsbError::OutOfRange(
            "site window context belongs to a different evaluator".to_string(),
        ));
    }
    if context.observer != query.observer
        || context.window != query.window
        || context.components != query.components
        || context.sun_altitude_ceiling != query.sun_altitude_ceiling
    {
        return Err(NsbError::OutOfRange(
            "query site, window, components, or Sun filter is incompatible with the site window context"
                .to_string(),
        ));
    }
    Ok(())
}

pub(crate) fn prepare_target_threshold(
    evaluator: &NsbEvaluator,
    context: &SiteWindowContext,
    query: &ThresholdQuery,
) -> Result<PreparedThresholdQuery> {
    let starlight_integrated = if query.components.contains(ComponentMask::STARLIGHT) {
        evaluator.evaluate_starlight(query.target)?.integrated
    } else {
        BandPhotonRadiance::new(0.0)
    };
    #[cfg(feature = "window-search-diagnostics")]
    let phase_started = Instant::now();
    let target_visible_periods = if let Some(target_min) = query.target_altitude_floor {
        let target_dir = direction::ICRS::new(query.target.ra(), query.target.dec());
        altitude_above_threshold(
            &target_dir,
            &query.observer,
            context.tt_window,
            target_min,
            SearchOpts::default(),
        )
    } else {
        vec![context.tt_window]
    };
    #[cfg(feature = "window-search-diagnostics")]
    super::diagnostics::update(|diagnostics| {
        diagnostics.target_visibility += phase_started.elapsed();
    });
    let candidate_windows =
        intersect_periods(context.sun_filter_periods.as_ref(), &target_visible_periods);
    let prepared = PreparedThresholdQuery {
        observer: query.observer,
        target: query.target,
        components: query.components,
        starlight_integrated,
        tt_window: context.tt_window,
        astronomical_night_periods: Arc::clone(&context.astronomical_night_periods),
        candidate_windows,
        airglow_phase_periods: Arc::clone(&context.airglow_phase_periods),
        airglow_model: context.airglow_model.clone(),
        solar_activity_cache: context.solar_activity_cache.clone(),
        moon_visible_periods: context.moon_visible_periods.clone(),
    };
    Ok(prepared)
}

#[cfg(test)]
pub(crate) fn prepare_threshold(
    evaluator: &NsbEvaluator,
    query: &ThresholdQuery,
    tt_window: TimePeriod<ModifiedJulianDate>,
) -> Result<PreparedThresholdQuery> {
    let context = prepare_site_context(evaluator, query, tt_window)?;
    prepare_target_threshold(evaluator, &context, query)
}
