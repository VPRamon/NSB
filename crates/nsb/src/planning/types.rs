// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Vallés Puig, Ramon
use crate::components::airglow;
use crate::evaluator::{ComponentMask, Observer, TargetDirection};
use crate::units::angular::Degrees;
use crate::units::radiometry::PhotonsPerSquareCentimeterNanosecondSteradian as BandPhotonRadiance;
use siderust::time::{Interval as TimePeriod, ModifiedJulianDate};
use std::sync::Arc;
use tempoch::{Period, UTC};

#[cfg_attr(feature = "python", pyo3::pyclass(frozen, module = "nsb"))]
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
    /// Target direction. Equatorial targets keep a fixed celestial footprint;
    /// horizontal targets keep fixed Alt/Az and resolve per sample time.
    pub target: TargetDirection,
    /// Inclusive search extent in UTC.
    pub window: Period<UTC>,
    /// Maximum accepted integrated radiance.
    pub threshold: BandPhotonRadiance,
    /// Components to compose.
    pub components: ComponentMask,
    /// Optional maximum Sun altitude pre-filter.
    pub sun_altitude_ceiling: Option<Degrees>,
    /// Optional minimum target altitude pre-filter.
    pub target_altitude_floor: Option<Degrees>,
}

impl ThresholdQuery {
    /// Default astronomical-night Sun-altitude ceiling.
    pub const DEFAULT_SUN_ALTITUDE_CEILING: Degrees = Degrees::new(-18.0);
    /// Default target-above-horizon altitude floor.
    pub const DEFAULT_TARGET_ALTITUDE_FLOOR: Degrees = Degrees::new(0.0);

    /// Window search with production-safe default components and night/horizon filters.
    ///
    /// `target` accepts a mean-J2000 [`crate::Target`], a geometric
    /// [`crate::HorizontalTarget`], or a [`TargetDirection`].
    pub fn new(
        observer: Observer,
        target: impl Into<TargetDirection>,
        window: Period<UTC>,
        threshold: BandPhotonRadiance,
    ) -> Self {
        Self {
            observer,
            target: target.into(),
            window,
            threshold,
            components: ComponentMask::ALL,
            sun_altitude_ceiling: Some(Self::DEFAULT_SUN_ALTITUDE_CEILING),
            target_altitude_floor: Some(Self::DEFAULT_TARGET_ALTITUDE_FLOOR),
        }
    }

    /// Restrict or expand the composed contributors.
    pub fn with_components(mut self, components: ComponentMask) -> Self {
        self.components = components;
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
/// or radiance threshold. The planning layer rejects contexts created
/// by another evaluator or for incompatible site/window/component settings.
/// Recreate the context after refreshing Tempoch time data. The active time-data
/// bundle must remain fixed during a query; observed changes fail explicitly.
#[derive(Clone)]
pub struct SiteWindowContext {
    pub(crate) evaluator_identity: Arc<()>,
    pub(crate) astronomy: Arc<super::astronomy::AstronomyCache>,
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
    /// UTC periods satisfying the filters and the selected search contract.
    /// Adaptive methods assume the piecewise physical signal is numerically
    /// resolved; certified methods require interval certificates for all time.
    pub periods: Vec<Period<UTC>>,
}

/// Starlight contribution prepared for threshold sampling.
///
/// Equatorial targets keep a constant Galactic HEALPix footprint, so the
/// integrated radiance is cached once. Horizontal targets sweep the map as
/// Earth rotates; radiance must be looked up after resolving the instantaneous
/// equatorial direction. Certified search only admits the constant case.
#[derive(Debug, Clone, Copy)]
pub(crate) enum PreparedStarlight {
    /// Component not selected.
    Disabled,
    /// Fixed celestial target — one HEALPix scalar for the whole search.
    Constant(BandPhotonRadiance),
    /// Fixed horizontal pointing — resolve and look up at each sample time.
    Dynamic,
}

#[derive(Debug, Clone)]
pub(crate) struct PreparedThresholdQuery {
    pub(crate) astronomy: Arc<super::astronomy::AstronomyCache>,
    pub(crate) observer: Observer,
    pub(crate) target: TargetDirection,
    pub(crate) components: ComponentMask,
    pub(crate) starlight: PreparedStarlight,
    pub(crate) tt_window: TimePeriod<ModifiedJulianDate>,
    pub(crate) astronomical_night_periods: Arc<[airglow::temporal::AstronomicalNightPeriod]>,
    pub(crate) candidate_windows: Vec<TimePeriod<ModifiedJulianDate>>,
    pub(crate) airglow_phase_periods: Arc<[airglow::temporal::AirglowPhasePeriod]>,
    pub(crate) airglow_model: Option<airglow::Airglow>,
    pub(crate) solar_activity_cache: Option<crate::solar_activity::SolarActivityValueCache>,
    pub(crate) moon_visible_periods: Option<Arc<[TimePeriod<ModifiedJulianDate>]>>,
}
