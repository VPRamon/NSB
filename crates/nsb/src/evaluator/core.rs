use super::metadata::{
    airglow_metadata, moonlight_metadata, starlight_metadata, zodiacal_metadata,
};
use super::search::{
    above_threshold_periods, adaptive_above_threshold_periods, coalesce_periods,
    complement_periods, tt_mjd_period_to_utc, tt_mjd_to_utc_time, utc_period_to_tt_mjd,
    validated_above_threshold_periods,
};
use super::types::*;
use crate::components::airglow::AirglowContinuum;
use crate::components::zodiacal::ZodiacalLight;
use crate::components::{airglow, moonlight, starlight};
use crate::error::{NsbError, Result};
use crate::NSB_S10_ZP;
use qtty::photometry::s10_to_surface_brightness;
use qtty::radiometry::{
    PhotonsPerSquareCentimeterNanosecondSteradian as BandPhotonRadiance, S10s as S10,
};
use qtty::Second;
#[cfg(not(feature = "window-search-diagnostics"))]
use rayon::prelude::*;
use siderust::astro::apparent::CorrectionPolicy;
use siderust::coordinates::spherical::direction;
use siderust::event::altitude::AltitudeProvider;
use siderust::event::horizontal::{star_horizontal, star_horizontal_with_policy};
use siderust::event::lunar::meeus_ch47::moon_position_meeus_ch47;
use siderust::qtty::{Day, Degree, Degrees, Hours};
use siderust::time::{intersect_periods, Interval as TimePeriod, ModifiedJulianDate};
use std::cell::RefCell;
use std::collections::HashMap;
use std::sync::Arc;
#[cfg(feature = "window-search-diagnostics")]
use std::time::Instant;
use tempoch::{Time, UTC};

/// Reusable evaluator with parsed immutable component data.
pub struct NsbEvaluator {
    identity: Arc<()>,
    zodiacal: ZodiacalLight,
    airglow_continuum: Arc<AirglowContinuum>,
    starlight: Option<starlight::Starlight>,
    config: NsbModelConfig,
}

impl NsbEvaluator {
    /// Construct the generic production-safe planning configuration.
    pub fn new() -> Result<Self> {
        Self::with_config(NsbModelConfig::generic_clear_sky())
    }

    /// Construct from explicit immutable model choices.
    pub fn with_config(config: NsbModelConfig) -> Result<Self> {
        let zodiacal = ZodiacalLight::leinert1998()?.with_extinction(config.zodiacal_extinction);
        let starlight = match config.starlight_model.as_ref() {
            None => None,
            Some(StarlightModel::BundledProductionGaiaDr3) => {
                Some(starlight::Starlight::bundled_production_model()?)
            }
            Some(StarlightModel::ExperimentalMap(map)) => {
                Some(starlight::Starlight::with_map((**map).clone()))
            }
            Some(StarlightModel::ValidatedExternalMap(map)) => {
                Some(starlight::Starlight::with_map(map.map().clone()))
            }
        };
        Ok(Self {
            identity: Arc::new(()),
            zodiacal,
            airglow_continuum: Arc::new(airglow::load_builtin_standard()?),
            starlight,
            config,
        })
    }

    /// Return a clone of the evaluator configuration.
    pub fn config(&self) -> NsbModelConfig {
        self.config.clone()
    }

    /// Describe selected components without performing a time-dependent
    /// radiance evaluation.
    pub fn describe_components(
        &self,
        observer: Observer,
        components: ComponentMask,
    ) -> Result<Vec<NsbComponentDescriptor>> {
        let mut descriptions = Vec::new();
        if components.contains(ComponentMask::ZODIACAL) {
            descriptions.push(NsbComponentDescriptor {
                name: "zodiacal",
                metadata: zodiacal_metadata(),
            });
        }
        if components.contains(ComponentMask::STARLIGHT) {
            if self.starlight.is_none() {
                return Err(NsbError::Unsupported(
                    "starlight component requested but no starlight model is configured".into(),
                ));
            }
            descriptions.push(NsbComponentDescriptor {
                name: "starlight",
                metadata: starlight_metadata(
                    self.config.starlight_model.as_ref(),
                    self.starlight
                        .as_ref()
                        .map(|model| model.map().provenance()),
                ),
            });
        }
        if components.contains(ComponentMask::AIRGLOW) {
            descriptions.push(NsbComponentDescriptor {
                name: "airglow",
                metadata: airglow_metadata(
                    self.config.site_profile,
                    observer,
                    None,
                    &self.config.airglow_geometry,
                ),
            });
        }
        if components.contains(ComponentMask::MOON) {
            descriptions.push(NsbComponentDescriptor {
                name: "moon",
                metadata: moonlight_metadata(
                    self.config.moonlight_model,
                    self.config.site_profile,
                    observer,
                ),
            });
        }
        Ok(descriptions)
    }

    /// Evaluate selected components for one point query.
    pub fn evaluate(&self, query: &PointQuery) -> Result<NsbResult> {
        let prepared = Self::prepare_point(query.observer, query.target, query.components);
        self.evaluate_full(&prepared, query.time)
    }

    /// Find filtered UTC periods whose integrated radiance is at or below the threshold.
    pub fn periods_below_threshold(&self, query: &ThresholdQuery) -> Result<ThresholdQueryResult> {
        let context = self.prepare_site_window_context(query)?;
        self.periods_below_threshold_with_context(&context, query)
    }

    /// Prepare target-independent ephemeris, filter, and model state for reuse.
    pub fn prepare_site_window_context(&self, query: &ThresholdQuery) -> Result<SiteWindowContext> {
        Self::validate_threshold(query)?;

        let tt_window = utc_period_to_tt_mjd(query.window);
        #[cfg(feature = "window-search-diagnostics")]
        let preparation_started = Instant::now();
        let context = self.prepare_site_context(query, tt_window)?;
        #[cfg(feature = "window-search-diagnostics")]
        super::diagnostics::update(|diagnostics| {
            diagnostics.threshold_preparation += preparation_started.elapsed();
        });
        Ok(context)
    }

    /// Search using target-independent preparation created for this evaluator.
    pub fn periods_below_threshold_with_context(
        &self,
        context: &SiteWindowContext,
        query: &ThresholdQuery,
    ) -> Result<ThresholdQueryResult> {
        Self::validate_threshold(query)?;
        self.validate_site_context(context, query)?;
        #[cfg(feature = "window-search-diagnostics")]
        let preparation_started = Instant::now();
        let prepared = self.prepare_target_threshold(context, query)?;
        #[cfg(feature = "window-search-diagnostics")]
        super::diagnostics::update(|diagnostics| {
            diagnostics.threshold_preparation += preparation_started.elapsed();
            diagnostics.candidate_windows += prepared.candidate_windows.len();
        });
        self.search_prepared(query, prepared)
    }

    fn search_prepared(
        &self,
        query: &ThresholdQuery,
        prepared: PreparedThresholdQuery,
    ) -> Result<ThresholdQueryResult> {
        #[cfg(feature = "window-search-diagnostics")]
        super::diagnostics::begin_threshold_search();
        let step = query.sample_step.to::<Day>();
        #[cfg(feature = "window-search-diagnostics")]
        let exact_evaluations = RefCell::new(HashMap::<u64, BandPhotonRadiance>::new());
        #[cfg(feature = "window-search-diagnostics")]
        let exact_f = |mjd_tt: ModifiedJulianDate| -> Result<BandPhotonRadiance> {
            let key = mjd_tt.raw().value().to_bits();
            if let Some(value) = exact_evaluations.borrow().get(&key) {
                return Ok(*value);
            }
            let value = self.evaluate_integrated(&prepared, mjd_tt)?;
            exact_evaluations.borrow_mut().insert(key, value);
            Ok(value)
        };
        #[cfg(feature = "window-search-diagnostics")]
        let discovery_evaluations = RefCell::new(HashMap::<u64, BandPhotonRadiance>::new());
        #[cfg(feature = "window-search-diagnostics")]
        let discovery_f = |mjd_tt: ModifiedJulianDate| -> Result<BandPhotonRadiance> {
            let key = mjd_tt.raw().value().to_bits();
            if let Some(value) = discovery_evaluations.borrow().get(&key) {
                return Ok(*value);
            }
            let value = self.evaluate_integrated_discovery(&prepared, mjd_tt)?;
            discovery_evaluations.borrow_mut().insert(key, value);
            Ok(value)
        };

        #[cfg(feature = "window-search-diagnostics")]
        let search_started = Instant::now();
        #[cfg(feature = "window-search-diagnostics")]
        let mut darker_periods: Vec<TimePeriod<ModifiedJulianDate>> = Vec::new();
        #[cfg(feature = "window-search-diagnostics")]
        for window in smooth_threshold_windows(&prepared) {
            let brighter = adaptive_above_threshold_periods(
                window,
                step,
                &discovery_f,
                &exact_f,
                query.threshold,
            )?;
            darker_periods.extend(complement_periods(window, &brighter));
        }
        #[cfg(not(feature = "window-search-diagnostics"))]
        let mut darker_periods: Vec<TimePeriod<ModifiedJulianDate>> =
            smooth_threshold_windows(&prepared)
                .into_par_iter()
                .map(|window| -> Result<Vec<TimePeriod<ModifiedJulianDate>>> {
                    let exact_evaluations = RefCell::new(HashMap::<u64, BandPhotonRadiance>::new());
                    let exact_f = |mjd_tt: ModifiedJulianDate| -> Result<BandPhotonRadiance> {
                        let key = mjd_tt.raw().value().to_bits();
                        if let Some(value) = exact_evaluations.borrow().get(&key) {
                            return Ok(*value);
                        }
                        let value = self.evaluate_integrated(&prepared, mjd_tt)?;
                        exact_evaluations.borrow_mut().insert(key, value);
                        Ok(value)
                    };
                    let discovery_evaluations =
                        RefCell::new(HashMap::<u64, BandPhotonRadiance>::new());
                    let discovery_f = |mjd_tt: ModifiedJulianDate| -> Result<BandPhotonRadiance> {
                        let key = mjd_tt.raw().value().to_bits();
                        if let Some(value) = discovery_evaluations.borrow().get(&key) {
                            return Ok(*value);
                        }
                        let value = self.evaluate_integrated_discovery(&prepared, mjd_tt)?;
                        discovery_evaluations.borrow_mut().insert(key, value);
                        Ok(value)
                    };
                    let brighter = adaptive_above_threshold_periods(
                        window,
                        step,
                        &discovery_f,
                        &exact_f,
                        query.threshold,
                    )?;
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

    /// Run one search while collecting compile-time-gated benchmark diagnostics.
    #[cfg(feature = "window-search-diagnostics")]
    pub fn periods_below_threshold_diagnosed(
        &self,
        query: &ThresholdQuery,
    ) -> Result<(ThresholdQueryResult, super::WindowSearchDiagnostics)> {
        super::diagnostics::reset();
        let result = self.periods_below_threshold(query)?;
        Ok((result, super::diagnostics::snapshot()))
    }

    fn validate_threshold(query: &ThresholdQuery) -> Result<()> {
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

    fn prepare_point(
        observer: Observer,
        target: Target,
        components: ComponentMask,
    ) -> PreparedPointQuery {
        PreparedPointQuery {
            observer,
            target,
            components,
        }
    }

    fn prepare_site_context(
        &self,
        query: &ThresholdQuery,
        tt_window: TimePeriod<ModifiedJulianDate>,
    ) -> Result<SiteWindowContext> {
        let uses_airglow = query.components.contains(ComponentMask::AIRGLOW);
        #[cfg(feature = "window-search-diagnostics")]
        let phase_started = Instant::now();
        let astronomical_night_periods = if uses_airglow {
            airglow::temporal::astronomical_nights_for_window(tt_window, query.observer)
        } else {
            Vec::new()
        };
        #[cfg(feature = "window-search-diagnostics")]
        super::diagnostics::update(|diagnostics| {
            diagnostics.astronomical_night_preparation += phase_started.elapsed();
        });
        #[cfg(feature = "window-search-diagnostics")]
        let phase_started = Instant::now();
        let sun_filter_periods = match query.sun_altitude_ceiling {
            Some(sun_max)
                if uses_airglow && airglow::temporal::is_astronomical_twilight(sun_max) =>
            {
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
            airglow::temporal::airglow_phase_periods_for_window(
                &astronomical_night_periods,
                tt_window,
            )
        } else {
            Vec::new()
        };
        let airglow_model = uses_airglow.then(|| {
            let profile = self.config.site_profile.profile(query.observer);
            airglow::Airglow::with_shared_continuum(
                query.observer,
                Arc::clone(&self.airglow_continuum),
            )
            .with_atmosphere(profile.atmosphere)
            .with_geometry(self.config.airglow_geometry.clone())
            .with_scale(profile.airglow.scale)
        });
        let solar_activity_cache = uses_airglow
            .then(|| {
                crate::solar_activity::SolarActivityValueCache::new(&self.config.solar_activity)
            })
            .transpose()?;
        #[cfg(feature = "window-search-diagnostics")]
        let phase_started = Instant::now();
        let moon_visible_periods = if query.components.contains(ComponentMask::MOON) {
            Some(conservative_moon_candidate_periods(
                tt_window,
                query.observer,
            )?)
        } else {
            None
        };
        #[cfg(feature = "window-search-diagnostics")]
        super::diagnostics::update(|diagnostics| {
            diagnostics.moon_visibility += phase_started.elapsed();
        });
        Ok(SiteWindowContext {
            evaluator_identity: Arc::clone(&self.identity),
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

    fn validate_site_context(
        &self,
        context: &SiteWindowContext,
        query: &ThresholdQuery,
    ) -> Result<()> {
        if !Arc::ptr_eq(&context.evaluator_identity, &self.identity) {
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

    fn prepare_target_threshold(
        &self,
        context: &SiteWindowContext,
        query: &ThresholdQuery,
    ) -> Result<PreparedThresholdQuery> {
        let starlight_integrated = if query.components.contains(ComponentMask::STARLIGHT) {
            self.evaluate_starlight(query.target)?.integrated
        } else {
            BandPhotonRadiance::new(0.0)
        };
        #[cfg(feature = "window-search-diagnostics")]
        let phase_started = Instant::now();
        let target_visible_periods = if let Some(target_min) = query.target_altitude_floor {
            let target_dir = direction::ICRS::new(query.target.ra(), query.target.dec());
            let approximate_altitude = |mjd: ModifiedJulianDate| -> Result<Degrees> {
                Ok(star_horizontal(
                    query.target.ra().to::<Degree>(),
                    query.target.dec().to::<Degree>(),
                    &query.observer,
                    mjd.to::<siderust::JD>(),
                )
                .alt())
            };
            let exact_altitude = |mjd: ModifiedJulianDate| -> Result<Degrees> {
                Ok(target_dir.altitude_at(&query.observer, mjd).to::<Degree>())
            };
            validated_above_threshold_periods(
                context.tt_window,
                Hours::new(4.0).to::<Day>(),
                &approximate_altitude,
                &exact_altitude,
                target_min,
            )?
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
    fn prepare_threshold(
        &self,
        query: &ThresholdQuery,
        tt_window: TimePeriod<ModifiedJulianDate>,
    ) -> Result<PreparedThresholdQuery> {
        let context = self.prepare_site_context(query, tt_window)?;
        self.prepare_target_threshold(&context, query)
    }

    fn evaluate_integrated(
        &self,
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
            let out = self
                .zodiacal
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
            if let Some(phase) = airglow_night_phase(prepared, mjd_tt) {
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
                .is_none_or(|periods| contains_time(periods, mjd_tt));
            if moon_visible {
                #[cfg(feature = "window-search-diagnostics")]
                super::diagnostics::update(|diagnostics| diagnostics.moonlight_evaluations += 1);
                #[cfg(feature = "window-search-diagnostics")]
                let component_started = Instant::now();
                let out = self.evaluate_moonlight(prepared.observer, time, prepared.target)?;
                total += out.integrated;
                #[cfg(feature = "window-search-diagnostics")]
                super::diagnostics::update(|diagnostics| {
                    diagnostics.exact_moonlight_time += component_started.elapsed();
                });
            }
        }

        Ok(total)
    }

    fn evaluate_integrated_discovery(
        &self,
        prepared: &PreparedThresholdQuery,
        mjd_tt: ModifiedJulianDate,
    ) -> Result<BandPhotonRadiance> {
        #[cfg(feature = "window-search-diagnostics")]
        super::diagnostics::update(|diagnostics| diagnostics.discovery_evaluations += 1);
        let time = tt_mjd_to_utc_time(mjd_tt);
        let mut total = BandPhotonRadiance::new(0.0);

        if prepared.components.contains(ComponentMask::ZODIACAL) {
            #[cfg(feature = "window-search-diagnostics")]
            let component_started = Instant::now();
            total += self
                .zodiacal
                .compute_observed_for_discovery(time, prepared.observer, prepared.target)?
                .integrated;
            #[cfg(feature = "window-search-diagnostics")]
            super::diagnostics::update(|diagnostics| {
                diagnostics.discovery_zodiacal_time += component_started.elapsed();
            });
        }
        if prepared.components.contains(ComponentMask::STARLIGHT) {
            total += prepared.starlight_integrated;
        }
        if prepared.components.contains(ComponentMask::AIRGLOW) {
            if let Some(phase) = airglow_night_phase(prepared, mjd_tt) {
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
                    diagnostics.discovery_airglow_time += component_started.elapsed();
                });
            }
        }
        if prepared.components.contains(ComponentMask::MOON) {
            let moon_visible = prepared
                .moon_visible_periods
                .as_ref()
                .is_none_or(|periods| contains_time(periods, mjd_tt));
            if moon_visible {
                #[cfg(feature = "window-search-diagnostics")]
                let component_started = Instant::now();
                total += self
                    .evaluate_moonlight_discovery(prepared.observer, time, prepared.target)?
                    .integrated;
                #[cfg(feature = "window-search-diagnostics")]
                super::diagnostics::update(|diagnostics| {
                    diagnostics.discovery_moonlight_time += component_started.elapsed();
                });
            }
        }

        Ok(total)
    }

    fn evaluate_full(&self, query: &PreparedPointQuery, time: Time<UTC>) -> Result<NsbResult> {
        let mut components = Vec::new();
        let mut total = BandPhotonRadiance::new(0.0);
        let (mut b_total, mut v_total) = (S10::new(0.0), S10::new(0.0));

        if query.components.contains(ComponentMask::ZODIACAL) {
            let out = self.zodiacal.compute(time, query.observer, query.target)?;
            total += out.integrated;
            b_total += out.b_flux_s10;
            v_total += out.v_flux_s10;
            components.push(NsbComponent {
                name: "zodiacal",
                integrated: out.integrated,
                b_flux_s10: out.b_flux_s10,
                v_flux_s10: out.v_flux_s10,
                relative_uncertainty: None,
                statistical_uncertainty: None,
                systematic_uncertainty: None,
                total_uncertainty: None,
                metadata: zodiacal_metadata(),
            });
        }
        if query.components.contains(ComponentMask::STARLIGHT) {
            let out = self.evaluate_starlight(query.target)?;
            total += out.integrated;
            b_total += out.b_flux_s10;
            v_total += out.v_flux_s10;
            components.push(NsbComponent {
                name: "starlight",
                integrated: out.integrated,
                b_flux_s10: out.b_flux_s10,
                v_flux_s10: out.v_flux_s10,
                relative_uncertainty: out.relative_uncertainty(),
                statistical_uncertainty: out.statistical_uncertainty,
                systematic_uncertainty: out.systematic_uncertainty,
                total_uncertainty: out.total_uncertainty,
                metadata: starlight_metadata(
                    self.config.starlight_model.as_ref(),
                    self.starlight
                        .as_ref()
                        .map(|model| model.map().provenance()),
                ),
            });
        }
        if query.components.contains(ComponentMask::AIRGLOW) {
            let (out, solar) =
                self.evaluate_airglow_resolved(query.observer, time, query.target)?;
            total += out.integrated;
            b_total += out.b_flux_s10;
            v_total += out.v_flux_s10;
            components.push(NsbComponent {
                name: "airglow",
                integrated: out.integrated,
                b_flux_s10: out.b_flux_s10,
                v_flux_s10: out.v_flux_s10,
                relative_uncertainty: out.relative_uncertainty,
                statistical_uncertainty: None,
                systematic_uncertainty: None,
                total_uncertainty: None,
                metadata: airglow_metadata(
                    self.config.site_profile,
                    query.observer,
                    Some(&solar),
                    &self.config.airglow_geometry,
                ),
            });
        }
        if query.components.contains(ComponentMask::MOON) {
            let out = self.evaluate_moonlight(query.observer, time, query.target)?;
            total += out.integrated;
            b_total += out.b_flux_s10;
            v_total += out.v_flux_s10;
            components.push(NsbComponent {
                name: "moon",
                integrated: out.integrated,
                b_flux_s10: out.b_flux_s10,
                v_flux_s10: out.v_flux_s10,
                relative_uncertainty: None,
                statistical_uncertainty: None,
                systematic_uncertainty: None,
                total_uncertainty: None,
                metadata: moonlight_metadata(
                    self.config.moonlight_model,
                    self.config.site_profile,
                    query.observer,
                ),
            });
        }

        Ok(NsbResult {
            integrated: total,
            b_mag: s10_to_surface_brightness(
                b_total.max(S10::new(f64::MIN_POSITIVE)),
                NSB_S10_ZP.value(),
            ),
            v_mag: s10_to_surface_brightness(
                v_total.max(S10::new(f64::MIN_POSITIVE)),
                NSB_S10_ZP.value(),
            ),
            components,
            band_diagnostic: super::BandDiagnostic::MONOCHROMATIC_S10_PROXY,
        })
    }

    fn evaluate_airglow_resolved(
        &self,
        observer: Observer,
        time: Time<UTC>,
        target: Target,
    ) -> Result<(
        airglow::AirglowOutputs,
        crate::solar_activity::ResolvedSolarActivity,
    )> {
        let solar = crate::solar_activity::resolve_f107(time, &self.config.solar_activity)?;
        let profile = self.config.site_profile.profile(observer);
        let outputs =
            airglow::Airglow::with_shared_continuum(observer, Arc::clone(&self.airglow_continuum))
                .with_atmosphere(profile.atmosphere)
                .with_geometry(self.config.airglow_geometry.clone())
                .with_solar_radio_flux(solar.value)
                .with_scale(profile.airglow.scale)
                .compute(time, target)?;
        Ok((outputs, solar))
    }

    fn evaluate_starlight(&self, target: Target) -> Result<starlight::StarlightOutputs> {
        let model = self.starlight.as_ref().ok_or_else(|| {
            NsbError::Unsupported(
                concat!(
                    "starlight component requested but no starlight model is configured; ",
                    "provide a validated map with StarlightModel::validated_external(...), ",
                    "use StarlightModel::bundled_production_gaia_dr3(), or ",
                    "explicitly opt into StarlightModel::with_experimental_map(...)"
                )
                .to_string(),
            )
        })?;
        model.compute(target)
    }

    fn evaluate_moonlight(
        &self,
        observer: Observer,
        time: Time<UTC>,
        target: Target,
    ) -> Result<moonlight::MoonOutputs> {
        match self.config.moonlight_model {
            MoonlightModel::KrisciunasSchaefer1991 => {
                moonlight::KrisciunasSchaefer1991::standard_clear_sky(observer)
                    .compute(time, target)
            }
            MoonlightModel::Jones2013Spectral => {
                moonlight::Jones2013Spectral::for_site_profile(observer, self.config.site_profile)
                    .compute(time, target)
            }
        }
    }

    fn evaluate_moonlight_discovery(
        &self,
        observer: Observer,
        time: Time<UTC>,
        target: Target,
    ) -> Result<moonlight::MoonOutputs> {
        match self.config.moonlight_model {
            MoonlightModel::KrisciunasSchaefer1991 => {
                moonlight::KrisciunasSchaefer1991::standard_clear_sky(observer)
                    .compute_for_discovery(time, target)
            }
            MoonlightModel::Jones2013Spectral => {
                moonlight::Jones2013Spectral::for_site_profile(observer, self.config.site_profile)
                    .compute_for_discovery(time, target)
            }
        }
    }
}

/// Cheap Moon-up discovery for threshold searches.
///
/// The reduced Meeus model is used only to exclude times when the Moon is well
/// below the horizon. A five-degree guard covers the measured reduced-geometry
/// envelope, topocentric parallax approximation, and frame-conversion drift.
/// Every
/// retained sample is still evaluated by the authoritative configured
/// moonlight model.
fn conservative_moon_candidate_periods(
    window: TimePeriod<ModifiedJulianDate>,
    observer: Observer,
) -> Result<Vec<TimePeriod<ModifiedJulianDate>>> {
    const CONSERVATIVE_ALTITUDE: Degrees = Degrees::new(-5.0);
    let altitude = |mjd: ModifiedJulianDate| -> Result<Degrees> {
        let moon = moon_position_meeus_ch47(mjd.to::<siderust::JD>());
        let horizontal = star_horizontal_with_policy(
            moon.ra.to::<Degree>(),
            moon.dec.to::<Degree>(),
            &observer,
            mjd.to::<siderust::JD>(),
            CorrectionPolicy::GEOMETRIC,
        );
        Ok(horizontal.alt())
    };
    above_threshold_periods(
        window,
        Hours::new(4.0).to::<Day>(),
        &altitude,
        CONSERVATIVE_ALTITUDE,
    )
}

fn contains_time(periods: &[TimePeriod<ModifiedJulianDate>], time: ModifiedJulianDate) -> bool {
    periods
        .iter()
        .any(|period| period.start <= time && time <= period.end)
}

fn smooth_threshold_windows(
    prepared: &PreparedThresholdQuery,
) -> Vec<TimePeriod<ModifiedJulianDate>> {
    let mut out = Vec::new();
    for candidate in &prepared.candidate_windows {
        let mut boundaries = vec![candidate.start, candidate.end];
        if prepared.components.contains(ComponentMask::AIRGLOW) {
            for phase in prepared.airglow_phase_periods.iter() {
                collect_internal_boundaries(&mut boundaries, phase.period, *candidate);
            }
        }
        if let Some(moon_visible_periods) = &prepared.moon_visible_periods {
            for moon_period in moon_visible_periods.iter() {
                collect_internal_boundaries(&mut boundaries, *moon_period, *candidate);
            }
        }

        boundaries.sort_by(|lhs, rhs| lhs.raw().value().total_cmp(&rhs.raw().value()));
        boundaries
            .dedup_by(|lhs, rhs| (lhs.raw().value() - rhs.raw().value()).abs() <= f64::EPSILON);
        for pair in boundaries.windows(2) {
            let start = pair[0];
            let end = pair[1];
            if start < end {
                out.push(TimePeriod::new(start, end));
            }
        }
    }
    out
}

fn collect_internal_boundaries(
    boundaries: &mut Vec<ModifiedJulianDate>,
    period: TimePeriod<ModifiedJulianDate>,
    window: TimePeriod<ModifiedJulianDate>,
) {
    for boundary in [period.start, period.end] {
        if window.start < boundary && boundary < window.end {
            boundaries.push(boundary);
        }
    }
}

fn airglow_night_phase(
    prepared: &PreparedThresholdQuery,
    time: ModifiedJulianDate,
) -> Option<airglow::AirglowNightPhase> {
    let phase =
        airglow::temporal::night_phase_from_nights(time, &prepared.astronomical_night_periods);
    let precomputed_phase =
        airglow::temporal::night_phase_from_phase_periods(time, &prepared.airglow_phase_periods);
    if let (Some(phase), Some(precomputed_phase)) = (phase, precomputed_phase) {
        debug_assert_eq!(phase, precomputed_phase);
    }
    phase
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{DateTime, Duration, Utc};
    use qtty::radiometry::PhotonsPerSquareCentimeterNanosecondSteradian as BandPhotonRadiance;
    use siderust::catalogs::observatories;
    use siderust::coordinates::centers::Geodetic;
    use siderust::coordinates::frames::ECEF;
    use siderust::qtty::{Degrees, Meters};
    use tempoch::Period;

    fn parse(input: &str) -> Time<UTC> {
        Time::<UTC>::from_chrono(
            DateTime::parse_from_rfc3339(input)
                .unwrap()
                .with_timezone(&Utc),
        )
    }

    fn tt_time(time: Time<UTC>) -> ModifiedJulianDate {
        ModifiedJulianDate::from(time.to::<siderust::time::TT>().to::<tempoch::MJD>())
    }

    fn paranal() -> Geodetic<ECEF> {
        observatories::EL_PARANAL.geodetic()
    }

    fn high_arctic() -> Geodetic<ECEF> {
        Geodetic::new_raw(Degrees::new(0.0), Degrees::new(89.0), Meters::new(0.0))
    }

    fn target_sgr_a() -> Target {
        Target::new(266.41683 * crate::DEG, -29.00781 * crate::DEG)
    }

    fn polar_target() -> Target {
        Target::new(0.0 * crate::DEG, 89.0 * crate::DEG)
    }

    fn threshold_query(
        observer: Observer,
        target: Target,
        start: &str,
        hours: i64,
        components: ComponentMask,
    ) -> ThresholdQuery {
        let start = parse(start);
        let end = Time::<UTC>::from_chrono(start.to_chrono().unwrap() + Duration::hours(hours));
        ThresholdQuery {
            observer,
            target,
            window: Period::new(start, end),
            threshold: BandPhotonRadiance::new(0.21),
            components,
            sample_step: Second::new(1_800.0),
            sun_altitude_ceiling: Some(ThresholdQuery::DEFAULT_SUN_ALTITUDE_CEILING),
            target_altitude_floor: Some(ThresholdQuery::DEFAULT_TARGET_ALTITUDE_FLOOR),
        }
    }

    fn scan_threshold_periods(
        evaluator: &NsbEvaluator,
        query: &ThresholdQuery,
    ) -> Result<ThresholdQueryResult> {
        NsbEvaluator::validate_threshold(query)?;
        let tt_window = utc_period_to_tt_mjd(query.window);
        let prepared = evaluator.prepare_threshold(query, tt_window)?;
        let step = query.sample_step.to::<Day>();
        let f = |mjd_tt: ModifiedJulianDate| -> Result<BandPhotonRadiance> {
            evaluator.evaluate_integrated(&prepared, mjd_tt)
        };

        let mut darker_periods = Vec::new();
        for candidate in &prepared.candidate_windows {
            let brighter = super::super::search::above_threshold_periods(
                *candidate,
                step,
                &f,
                query.threshold,
            )?;
            darker_periods.extend(complement_periods(*candidate, &brighter));
        }
        coalesce_periods(&mut darker_periods);

        Ok(ThresholdQueryResult {
            threshold: query.threshold,
            periods: darker_periods
                .into_iter()
                .map(|period| tt_mjd_period_to_utc(period, prepared.tt_window, query.window))
                .collect(),
        })
    }

    fn assert_periods_match_within_seconds(
        actual: &ThresholdQueryResult,
        expected: &ThresholdQueryResult,
        tolerance_seconds: i64,
    ) {
        assert_eq!(actual.periods.len(), expected.periods.len());
        for (actual, expected) in actual.periods.iter().zip(&expected.periods) {
            let start_delta = (actual.start.to_chrono().unwrap()
                - expected.start.to_chrono().unwrap())
            .num_seconds()
            .abs();
            let end_delta = (actual.end.to_chrono().unwrap() - expected.end.to_chrono().unwrap())
                .num_seconds()
                .abs();
            assert!(
                start_delta <= tolerance_seconds,
                "period start differs by {start_delta}s"
            );
            assert!(
                end_delta <= tolerance_seconds,
                "period end differs by {end_delta}s"
            );
        }
    }

    #[test]
    fn threshold_airglow_does_not_use_point_night_search_hot_path() {
        let evaluator = NsbEvaluator::new().unwrap();
        let query = threshold_query(
            paranal(),
            target_sgr_a(),
            "2023-09-04T00:00:00Z",
            12,
            ComponentMask::AIRGLOW,
        );

        airglow::temporal::forbid_point_night_search_for_test(|| {
            evaluator.periods_below_threshold(&query).unwrap();
        });
    }

    #[test]
    fn adaptive_threshold_search_matches_scan_oracle_for_representative_window() {
        let evaluator = NsbEvaluator::new().unwrap();
        let start = parse("2023-09-04T02:00:00Z");
        let end = Time::<UTC>::from_chrono(start.to_chrono().unwrap() + Duration::hours(4));
        let query = ThresholdQuery::new(
            paranal(),
            target_sgr_a(),
            Period::new(start, end),
            BandPhotonRadiance::new(0.21),
        )
        .with_components(ComponentMask::ZODIACAL)
        .with_sample_step(Second::new(1_800.0))
        .with_sun_altitude_ceiling(None)
        .with_target_altitude_floor(None);

        let adaptive = evaluator.periods_below_threshold(&query).unwrap();
        let scan = scan_threshold_periods(&evaluator, &query).unwrap();

        assert_periods_match_within_seconds(&adaptive, &scan, 2);
    }

    #[test]
    fn adaptive_search_matches_exact_scan_across_components_and_year_boundary() {
        let evaluator = NsbEvaluator::new().unwrap();
        for (start, hours, target, components, threshold) in [
            (
                "2026-12-30T00:00:00Z",
                72,
                target_sgr_a(),
                ComponentMask::ALL,
                0.25,
            ),
            (
                "2026-01-14T00:00:00Z",
                72,
                Target::new(83.6331 * crate::DEG, 22.0145 * crate::DEG),
                ComponentMask::MOON,
                0.02,
            ),
            (
                "2026-06-01T00:00:00Z",
                48,
                Target::new(120.0 * crate::DEG, -10.0 * crate::DEG),
                ComponentMask::AIRGLOW | ComponentMask::ZODIACAL,
                0.20,
            ),
        ] {
            let mut query = threshold_query(paranal(), target, start, hours, components);
            query.threshold = BandPhotonRadiance::new(threshold);
            query.sample_step = Second::new(600.0);
            let adaptive = evaluator.periods_below_threshold(&query).unwrap();
            let scan = scan_threshold_periods(&evaluator, &query).unwrap();
            assert_periods_match_within_seconds(&adaptive, &scan, 2);
        }
    }

    #[test]
    fn reusable_site_context_matches_independent_queries_for_multiple_targets() {
        let evaluator = NsbEvaluator::new().unwrap();
        let first = threshold_query(
            paranal(),
            target_sgr_a(),
            "2026-01-01T00:00:00Z",
            72,
            ComponentMask::ALL,
        );
        let context = evaluator.prepare_site_window_context(&first).unwrap();

        for (ra, dec, floor, threshold) in [
            (83.6331, 22.0145, 20.0, 0.25),
            (120.0, -10.0, 30.0, 0.22),
            (266.41683, -29.00781, 0.0, 0.20),
        ] {
            let mut query = first.clone();
            query.target = Target::new(ra * crate::DEG, dec * crate::DEG);
            query.target_altitude_floor = Some(Degrees::new(floor));
            query.threshold = BandPhotonRadiance::new(threshold);
            let reused = evaluator
                .periods_below_threshold_with_context(&context, &query)
                .unwrap();
            let independent = evaluator.periods_below_threshold(&query).unwrap();
            assert_eq!(reused.periods, independent.periods);
        }
    }

    #[test]
    fn site_context_rejects_another_evaluator_or_incompatible_site_state() {
        let evaluator = NsbEvaluator::new().unwrap();
        let other = NsbEvaluator::new().unwrap();
        let query = threshold_query(
            paranal(),
            target_sgr_a(),
            "2026-01-01T00:00:00Z",
            24,
            ComponentMask::ALL,
        );
        let context = evaluator.prepare_site_window_context(&query).unwrap();
        assert!(other
            .periods_below_threshold_with_context(&context, &query)
            .is_err());

        let mut incompatible = query.clone();
        incompatible.sun_altitude_ceiling = Some(Degrees::new(-12.0));
        assert!(evaluator
            .periods_below_threshold_with_context(&context, &incompatible)
            .is_err());
    }

    #[test]
    fn target_visibility_matches_precise_altitude_scan_across_regimes() {
        let evaluator = NsbEvaluator::new().unwrap();
        for (observer, target, floor) in [
            (paranal(), target_sgr_a(), 20.0),
            (paranal(), polar_target(), 20.0),
            (high_arctic(), polar_target(), 20.0),
        ] {
            let start = parse("2026-01-01T00:00:00Z");
            let end = Time::<UTC>::from_chrono(start.to_chrono().unwrap() + Duration::days(30));
            let query = ThresholdQuery::new(
                observer,
                target,
                Period::new(start, end),
                BandPhotonRadiance::new(0.25),
            )
            .with_components(ComponentMask::ZODIACAL)
            .with_sun_altitude_ceiling(None)
            .with_target_altitude_floor(Some(Degrees::new(floor)));
            let tt_window = utc_period_to_tt_mjd(query.window);
            let prepared = evaluator.prepare_threshold(&query, tt_window).unwrap();
            let direction = direction::ICRS::new(target.ra(), target.dec());
            let exact_altitude = |time: ModifiedJulianDate| -> Result<Degrees> {
                Ok(direction.altitude_at(&observer, time).to::<Degree>())
            };
            let exact = above_threshold_periods(
                tt_window,
                Hours::new(10.0 / 60.0).to::<Day>(),
                &exact_altitude,
                Degrees::new(floor),
            )
            .unwrap();

            assert_eq!(prepared.candidate_windows.len(), exact.len());
            for (actual, expected) in prepared.candidate_windows.iter().zip(exact) {
                assert!(
                    (actual.start.raw().value() - expected.start.raw().value()).abs() * 86_400.0
                        <= 2.0
                );
                assert!(
                    (actual.end.raw().value() - expected.end.raw().value()).abs() * 86_400.0 <= 2.0
                );
            }
        }
    }

    #[test]
    fn empty_visibility_candidates_produce_no_periods() {
        let evaluator = NsbEvaluator::new().unwrap();
        let query = threshold_query(
            paranal(),
            polar_target(),
            "2026-01-01T00:00:00Z",
            72,
            ComponentMask::ALL,
        );
        let result = evaluator.periods_below_threshold(&query).unwrap();
        assert!(result.periods.is_empty());
    }

    #[test]
    fn threshold_airglow_context_matches_exact_point_airglow() {
        let evaluator = NsbEvaluator::new().unwrap();
        let observer = paranal();
        let target = target_sgr_a();
        let query = threshold_query(
            observer,
            target,
            "2023-09-04T00:00:00Z",
            12,
            ComponentMask::AIRGLOW,
        );
        let time = parse("2023-09-04T04:00:00Z");
        let prepared = evaluator
            .prepare_threshold(&query, utc_period_to_tt_mjd(query.window))
            .unwrap();

        let context = evaluator
            .evaluate_integrated(&prepared, tt_time(time))
            .unwrap();
        let exact = evaluator
            .evaluate_airglow_resolved(observer, time, target)
            .unwrap()
            .0;

        assert!((context.value() - exact.integrated.value()).abs() < 1.0e-12);
    }

    #[test]
    fn threshold_airglow_context_matches_continuous_high_latitude_night() {
        let evaluator = NsbEvaluator::new().unwrap();
        let observer = high_arctic();
        let target = polar_target();
        let query = threshold_query(
            observer,
            target,
            "2023-12-20T00:00:00Z",
            72,
            ComponentMask::AIRGLOW,
        );
        let time = parse("2023-12-21T12:00:00Z");
        let prepared = evaluator
            .prepare_threshold(&query, utc_period_to_tt_mjd(query.window))
            .unwrap();

        let context = evaluator
            .evaluate_integrated(&prepared, tt_time(time))
            .unwrap();
        let exact = evaluator
            .evaluate_airglow_resolved(observer, time, target)
            .unwrap()
            .0;

        assert!((context.value() - exact.integrated.value()).abs() < 1.0e-12);
    }

    #[test]
    fn threshold_moon_context_skips_only_moon_down_samples() {
        let evaluator = NsbEvaluator::new().unwrap();
        let observer = paranal();
        let target = target_sgr_a();
        let query = threshold_query(
            observer,
            target,
            "2023-09-04T00:00:00Z",
            168,
            ComponentMask::MOON,
        );
        let prepared = evaluator
            .prepare_threshold(&query, utc_period_to_tt_mjd(query.window))
            .unwrap();
        let moon_periods = prepared
            .moon_visible_periods
            .as_ref()
            .expect("moon visibility context");
        let start = query.window.start.to_chrono().unwrap();
        let mut checked_down = false;
        let mut checked_up = false;

        for hour in 0..168 {
            let time = Time::<UTC>::from_chrono(start + Duration::hours(hour));
            let mjd = tt_time(time);
            let context = evaluator.evaluate_integrated(&prepared, mjd).unwrap();
            let exact = evaluator
                .evaluate_moonlight(observer, time, target)
                .unwrap();

            if contains_time(moon_periods, mjd) {
                assert!((context.value() - exact.integrated.value()).abs() < 1.0e-12);
                checked_up = true;
            } else {
                assert_eq!(context.value(), 0.0);
                assert_eq!(exact.integrated.value(), 0.0);
                checked_down = true;
            }
        }

        assert!(checked_down, "test window should include Moon-down samples");
        assert!(checked_up, "test window should include Moon-up samples");
    }
}
