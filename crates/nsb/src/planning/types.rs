use crate::components::airglow;
use crate::evaluator::{ComponentMask, Observer, Target};
use qtty::angular::Degrees;
use qtty::radiometry::PhotonsPerSquareCentimeterNanosecondSteradian as BandPhotonRadiance;
use qtty::Second;
use siderust::time::{Interval as TimePeriod, ModifiedJulianDate};
use std::sync::Arc;
use tempoch::{Period, UTC};

#[derive(Debug, Clone)]
/// Inputs for a below-threshold observing-window search.
///
/// Construct with [`Self::new`] and the `with_*` builders. The struct is
/// `#[non_exhaustive]` so additional filters can be added without breaking
/// external struct literals.
#[non_exhaustive]
pub struct ThresholdQuery {
    /// Ground observer.
    pub observer: Observer,
    /// Equatorial target direction.
    pub target: Target,
    /// Inclusive search extent in UTC.
    pub window: Period<UTC>,
    /// Maximum accepted integrated radiance.
    pub threshold: BandPhotonRadiance,
    /// Components to compose.
    pub components: ComponentMask,
    /// Coarse radiance scan step.
    pub sample_step: Second,
    /// Optional maximum Sun altitude pre-filter.
    pub sun_altitude_ceiling: Option<Degrees>,
    /// Optional minimum target altitude pre-filter.
    pub target_altitude_floor: Option<Degrees>,
}

impl ThresholdQuery {
    /// Default ten-minute coarse scan step.
    pub const DEFAULT_SAMPLE_STEP: Second = Second::new(600.0);
    /// Default astronomical-night Sun-altitude ceiling.
    pub const DEFAULT_SUN_ALTITUDE_CEILING: Degrees = Degrees::new(-18.0);
    /// Default target-above-horizon altitude floor.
    pub const DEFAULT_TARGET_ALTITUDE_FLOOR: Degrees = Degrees::new(0.0);

    /// Window search with production-safe default components and night/horizon filters.
    pub fn new(
        observer: Observer,
        target: Target,
        window: Period<UTC>,
        threshold: BandPhotonRadiance,
    ) -> Self {
        Self {
            observer,
            target,
            window,
            threshold,
            components: ComponentMask::ALL,
            sample_step: Self::DEFAULT_SAMPLE_STEP,
            sun_altitude_ceiling: Some(Self::DEFAULT_SUN_ALTITUDE_CEILING),
            target_altitude_floor: Some(Self::DEFAULT_TARGET_ALTITUDE_FLOOR),
        }
    }

    /// Restrict or expand the composed contributors.
    pub fn with_components(mut self, components: ComponentMask) -> Self {
        self.components = components;
        self
    }

    /// Replace the coarse radiance scan step.
    pub fn with_sample_step(mut self, sample_step: Second) -> Self {
        self.sample_step = sample_step;
        self
    }

    /// Replace or clear the optional Sun-altitude pre-filter.
    pub fn with_sun_altitude_ceiling(mut self, sun_altitude_ceiling: Option<Degrees>) -> Self {
        self.sun_altitude_ceiling = sun_altitude_ceiling;
        self
    }

    /// Replace or clear the optional target-altitude pre-filter.
    pub fn with_target_altitude_floor(mut self, target_altitude_floor: Option<Degrees>) -> Self {
        self.target_altitude_floor = target_altitude_floor;
        self
    }
}

/// Reusable target-independent preparation for searches at one site and time window.
///
/// Create this with [`NsbEvaluator::prepare_site_window_context`](crate::NsbEvaluator::prepare_site_window_context)
/// and reuse it for queries that differ only in target, target-altitude floor,
/// radiance threshold, or sampling step. The planning layer rejects contexts created
/// by another evaluator or for incompatible site/window/component settings.
#[derive(Clone)]
pub struct SiteWindowContext {
    pub(crate) evaluator_identity: Arc<()>,
    pub(crate) observer: Observer,
    pub(crate) window: Period<UTC>,
    pub(crate) components: ComponentMask,
    pub(crate) sun_altitude_ceiling: Option<Degrees>,
    pub(crate) tt_window: TimePeriod<ModifiedJulianDate>,
    pub(crate) sun_filter_periods: Arc<[TimePeriod<ModifiedJulianDate>]>,
    pub(crate) astronomical_night_periods: Arc<[airglow::temporal::AstronomicalNightPeriod]>,
    pub(crate) airglow_phase_periods: Arc<[airglow::temporal::AirglowPhasePeriod]>,
    pub(crate) airglow_model: Option<airglow::Airglow>,
    pub(crate) solar_activity_cache: Option<crate::solar_activity::SolarActivityValueCache>,
    pub(crate) moon_visible_periods: Option<Arc<[TimePeriod<ModifiedJulianDate>]>>,
}

impl std::fmt::Debug for SiteWindowContext {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("SiteWindowContext")
            .field("observer", &self.observer)
            .field("window", &self.window)
            .field("components", &self.components)
            .field("sun_altitude_ceiling", &self.sun_altitude_ceiling)
            .field("sun_filter_periods", &self.sun_filter_periods.len())
            .field(
                "astronomical_night_periods",
                &self.astronomical_night_periods.len(),
            )
            .field(
                "moon_visible_periods",
                &self.moon_visible_periods.as_ref().map(|p| p.len()),
            )
            .finish_non_exhaustive()
    }
}

#[derive(Debug, Clone)]
/// Result of a below-threshold window search.
#[non_exhaustive]
pub struct ThresholdQueryResult {
    /// Threshold used by the search.
    pub threshold: BandPhotonRadiance,
    /// UTC periods satisfying all filters and the threshold.
    pub periods: Vec<Period<UTC>>,
}

#[derive(Debug, Clone)]
pub(crate) struct PreparedThresholdQuery {
    pub(crate) observer: Observer,
    pub(crate) target: Target,
    pub(crate) components: ComponentMask,
    pub(crate) starlight_integrated: BandPhotonRadiance,
    pub(crate) tt_window: TimePeriod<ModifiedJulianDate>,
    pub(crate) astronomical_night_periods: Arc<[airglow::temporal::AstronomicalNightPeriod]>,
    pub(crate) candidate_windows: Vec<TimePeriod<ModifiedJulianDate>>,
    pub(crate) airglow_phase_periods: Arc<[airglow::temporal::AirglowPhasePeriod]>,
    pub(crate) airglow_model: Option<airglow::Airglow>,
    pub(crate) solar_activity_cache: Option<crate::solar_activity::SolarActivityValueCache>,
    pub(crate) moon_visible_periods: Option<Arc<[TimePeriod<ModifiedJulianDate>]>>,
}
