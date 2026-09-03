use super::metadata::{
    airglow_metadata, moonlight_metadata, starlight_metadata, zodiacal_metadata,
};
use super::search::{
    adaptive_above_threshold_periods, coalesce_periods, complement_periods, tt_mjd_period_to_utc,
    tt_mjd_to_utc_time, utc_period_to_tt_mjd,
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
use siderust::bodies::Moon as MoonBody;
use siderust::bodies::Sun as SunBody;
use siderust::coordinates::spherical::direction;
use siderust::event::altitude::{AltitudeEventsExt, SearchOpts};
use siderust::qtty::Day;
use siderust::time::{intersect_periods, Interval as TimePeriod, ModifiedJulianDate};
use std::sync::Arc;
use tempoch::{Time, UTC};

/// Reusable evaluator with parsed immutable component data.
pub struct NsbEvaluator {
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
        Self::validate_threshold(query)?;

        let tt_window = utc_period_to_tt_mjd(query.window);
        let prepared = self.prepare_threshold(query, tt_window)?;
        let step = query.sample_step.to::<Day>();
        let f = |mjd_tt: ModifiedJulianDate| -> Result<BandPhotonRadiance> {
            self.evaluate_integrated(&prepared, mjd_tt)
        };

        let mut darker_periods: Vec<TimePeriod<ModifiedJulianDate>> = Vec::new();
        for window in smooth_threshold_windows(&prepared) {
            let brighter = adaptive_above_threshold_periods(window, step, &f, query.threshold)?;
            darker_periods.extend(complement_periods(window, &brighter));
        }
        coalesce_periods(&mut darker_periods);

        Ok(ThresholdQueryResult {
            threshold: query.threshold,
            periods: darker_periods
                .into_iter()
                .map(|p| tt_mjd_period_to_utc(p, prepared.tt_window, query.window))
                .collect(),
        })
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

    fn prepare_threshold(
        &self,
        query: &ThresholdQuery,
        tt_window: TimePeriod<ModifiedJulianDate>,
    ) -> Result<PreparedThresholdQuery> {
        let starlight_integrated = if query.components.contains(ComponentMask::STARLIGHT) {
            self.evaluate_starlight(query.target)?.integrated
        } else {
            BandPhotonRadiance::new(0.0)
        };
        let uses_airglow = query.components.contains(ComponentMask::AIRGLOW);
        let astronomical_night_periods = if uses_airglow {
            airglow::temporal::astronomical_nights_for_window(tt_window, query.observer)
        } else {
            Vec::new()
        };
        let sun_filter_periods = match query.sun_altitude_ceiling {
            Some(sun_max)
                if uses_airglow && airglow::temporal::is_astronomical_twilight(sun_max) =>
            {
                airglow::temporal::clipped_night_periods(&astronomical_night_periods, tt_window)
            }
            Some(sun_max) => {
                SunBody.below_threshold(&query.observer, tt_window, sun_max, SearchOpts::default())
            }
            None => vec![tt_window],
        };
        let target_visible_periods = if let Some(target_min) = query.target_altitude_floor {
            let target_dir = direction::ICRS::new(query.target.ra(), query.target.dec());
            target_dir.above_threshold(
                &query.observer,
                tt_window,
                target_min,
                SearchOpts::default(),
            )
        } else {
            vec![tt_window]
        };
        let airglow_phase_periods = if uses_airglow {
            airglow::temporal::airglow_phase_periods_for_window(
                &astronomical_night_periods,
                tt_window,
            )
        } else {
            Vec::new()
        };
        let moon_visible_periods = query.components.contains(ComponentMask::MOON).then(|| {
            MoonBody.above_threshold(
                &query.observer,
                tt_window,
                qtty::angular::Degrees::new(0.0),
                SearchOpts::default(),
            )
        });
        let mut prepared = PreparedThresholdQuery {
            observer: query.observer,
            target: query.target,
            components: query.components,
            starlight_integrated,
            tt_window,
            sun_filter_periods,
            astronomical_night_periods,
            target_visible_periods,
            candidate_windows: Vec::new(),
            airglow_phase_periods,
            moon_visible_periods,
        };
        prepared.candidate_windows = intersect_periods(
            &prepared.sun_filter_periods,
            &prepared.target_visible_periods,
        );
        Ok(prepared)
    }

    fn evaluate_integrated(
        &self,
        prepared: &PreparedThresholdQuery,
        mjd_tt: ModifiedJulianDate,
    ) -> Result<BandPhotonRadiance> {
        let time = tt_mjd_to_utc_time(mjd_tt);
        let mut total = BandPhotonRadiance::new(0.0);

        if prepared.components.contains(ComponentMask::ZODIACAL) {
            let out = self
                .zodiacal
                .compute_observed(time, prepared.observer, prepared.target)?;
            total += out.integrated;
        }
        if prepared.components.contains(ComponentMask::STARLIGHT) {
            total += prepared.starlight_integrated;
        }
        if prepared.components.contains(ComponentMask::AIRGLOW) {
            if let Some(time_bin) = airglow_time_bin(prepared, mjd_tt) {
                let out = self.evaluate_airglow_with_time_bin(
                    prepared.observer,
                    time,
                    prepared.target,
                    time_bin,
                )?;
                total += out.integrated;
            }
        }
        if prepared.components.contains(ComponentMask::MOON) {
            let moon_visible = prepared
                .moon_visible_periods
                .as_ref()
                .is_none_or(|periods| contains_time(periods, mjd_tt));
            if moon_visible {
                let out = self.evaluate_moonlight(prepared.observer, time, prepared.target)?;
                total += out.integrated;
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

    fn evaluate_airglow_with_time_bin(
        &self,
        observer: Observer,
        time: Time<UTC>,
        target: Target,
        time_bin: usize,
    ) -> Result<airglow::AirglowOutputs> {
        let solar = crate::solar_activity::resolve_f107(time, &self.config.solar_activity)?;
        let profile = self.config.site_profile.profile(observer);
        airglow::Airglow::with_shared_continuum(observer, Arc::clone(&self.airglow_continuum))
            .with_atmosphere(profile.atmosphere)
            .with_geometry(self.config.airglow_geometry.clone())
            .with_solar_radio_flux(solar.value)
            .with_scale(profile.airglow.scale)
            .compute_with_time_of_night_bin(time, target, time_bin)
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
            for phase in &prepared.airglow_phase_periods {
                collect_internal_boundaries(&mut boundaries, phase.period, *candidate);
            }
        }
        if let Some(moon_visible_periods) = &prepared.moon_visible_periods {
            for moon_period in moon_visible_periods {
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

fn airglow_time_bin(prepared: &PreparedThresholdQuery, time: ModifiedJulianDate) -> Option<usize> {
    let bin = airglow::temporal::time_of_night_bin_from_nights(
        time,
        &prepared.astronomical_night_periods,
    );
    let _phase_bin = airglow::temporal::time_of_night_bin_from_phase_periods(
        time,
        &prepared.airglow_phase_periods,
    );
    bin
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
            if checked_down && checked_up {
                break;
            }
        }

        assert!(checked_down, "test window should include Moon-down samples");
        assert!(checked_up, "test window should include Moon-up samples");
    }
}
