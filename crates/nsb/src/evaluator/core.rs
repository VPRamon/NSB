use super::metadata::{airglow_metadata, moonlight_metadata, starlight_metadata, zodiacal_metadata};
use super::search::{
    above_threshold_periods, complement_periods, tt_mjd_period_to_utc, tt_mjd_to_utc_time,
    utc_period_to_tt_mjd,
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
use qtty::{Quantity, Second};
use siderust::bodies::Sun as SunBody;
use siderust::coordinates::spherical::direction;
use siderust::event::altitude::{AltitudeEventsExt, SearchOpts};
use siderust::qtty::{Day, Days};
use siderust::time::{intersect_periods, Interval as TimePeriod, ModifiedJulianDate};
use tempoch::{Time, UTC};

pub struct NsbEvaluator {
    zodiacal: ZodiacalLight,
    airglow_continuum: AirglowContinuum,
    config: NsbModelConfig,
}

impl NsbEvaluator {
    pub fn new() -> Result<Self> {
        Self::with_config(NsbModelConfig::generic_clear_sky())
    }

    pub fn with_config(config: NsbModelConfig) -> Result<Self> {
        let zodiacal = ZodiacalLight::leinert1998()?.with_extinction(config.zodiacal_extinction);
        Ok(Self {
            zodiacal,
            airglow_continuum: airglow::load_builtin_standard()?,
            config,
        })
    }

    #[doc(hidden)]
    pub fn python_parity() -> Result<Self> {
        Self::with_config(NsbModelConfig::python_parity())
    }

    pub fn config(&self) -> NsbModelConfig {
        self.config.clone()
    }

    pub fn evaluate(&self, query: &PointQuery) -> Result<NsbResult> {
        let prepared = Self::prepare_point(query.observer, query.target, query.components);
        self.evaluate_full(&prepared, query.time)
    }

    pub fn periods_below_threshold(&self, query: &ThresholdQuery) -> Result<ThresholdQueryResult> {
        Self::validate_threshold(query)?;

        let prepared = self.prepare_threshold(query)?;
        let tt_window = utc_period_to_tt_mjd(query.window);
        let step = query.sample_step.to::<Day>();
        let candidate_windows = self.candidate_windows(query, &prepared, tt_window);
        let f = |mjd_tt: ModifiedJulianDate| -> Result<BandPhotonRadiance> {
            self.evaluate_integrated(&prepared, mjd_tt)
        };

        let mut darker_periods: Vec<TimePeriod<ModifiedJulianDate>> = Vec::new();
        for cw in candidate_windows {
            let brighter = above_threshold_periods(cw, step, &f, query.threshold)?;
            darker_periods.extend(complement_periods(cw, &brighter));
        }

        Ok(ThresholdQueryResult {
            threshold: query.threshold,
            periods: darker_periods
                .into_iter()
                .map(|p| tt_mjd_period_to_utc(p, tt_window, query.window))
                .collect(),
        })
    }

    #[doc(hidden)]
    pub fn periods_below_threshold_legacy(
        &self,
        query: &ThresholdQuery,
    ) -> Result<ThresholdQueryResult> {
        Self::validate_threshold(query)?;
        if query.components.contains(ComponentMask::STARLIGHT) {
            self.evaluate_starlight(query.target)?;
        }

        let prepared = Self::prepare_point(query.observer, query.target, query.components);
        let tt_window = utc_period_to_tt_mjd(query.window);
        let step = query.sample_step.to::<Day>();
        let integrated_at = |mjd_tt: ModifiedJulianDate| -> Result<BandPhotonRadiance> {
            let time_utc = tt_mjd_to_utc_time(mjd_tt);
            Ok(self.evaluate_full(&prepared, time_utc)?.integrated)
        };

        let brighter_than_threshold =
            above_threshold_periods(tt_window, step, &integrated_at, query.threshold)?;
        let darker_than_threshold = complement_periods(tt_window, &brighter_than_threshold);

        Ok(ThresholdQueryResult {
            threshold: query.threshold,
            periods: darker_than_threshold
                .into_iter()
                .map(|period| tt_mjd_period_to_utc(period, tt_window, query.window))
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

    fn prepare_threshold(&self, query: &ThresholdQuery) -> Result<PreparedThresholdQuery> {
        let starlight_integrated = if query.components.contains(ComponentMask::STARLIGHT) {
            self.evaluate_starlight(query.target)?.integrated
        } else {
            BandPhotonRadiance::new(0.0)
        };
        Ok(PreparedThresholdQuery {
            observer: query.observer,
            target: query.target,
            components: query.components,
            starlight_integrated,
        })
    }

    fn candidate_windows(
        &self,
        query: &ThresholdQuery,
        prepared: &PreparedThresholdQuery,
        tt_window: TimePeriod<ModifiedJulianDate>,
    ) -> Vec<TimePeriod<ModifiedJulianDate>> {
        let mut current: Vec<TimePeriod<ModifiedJulianDate>> = vec![tt_window];

        if let Some(sun_max) = query.sun_altitude_ceiling {
            let nights = SunBody.below_threshold(
                &prepared.observer,
                tt_window,
                sun_max,
                SearchOpts::default(),
            );
            current = intersect_periods(&current, &nights);
            if current.is_empty() {
                return current;
            }
        }
        if let Some(target_min) = query.target_altitude_floor {
            let target_dir = direction::ICRS::new(prepared.target.ra(), prepared.target.dec());
            let above = target_dir.above_threshold(
                &prepared.observer,
                tt_window,
                target_min,
                SearchOpts::default(),
            );
            current = intersect_periods(&current, &above);
        }
        current
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
            let out = self.evaluate_airglow(prepared.observer, time, prepared.target)?;
            total += out.integrated;
        }
        if prepared.components.contains(ComponentMask::MOON) {
            let out = self.evaluate_moonlight(prepared.observer, time, prepared.target)?;
            total += out.integrated;
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
                relative_uncertainty: None,
                metadata: starlight_metadata(&self.config.starlight_model),
            });
        }
        if query.components.contains(ComponentMask::AIRGLOW) {
            let out = self.evaluate_airglow(query.observer, time, query.target)?;
            total += out.integrated;
            b_total += out.b_flux_s10;
            v_total += out.v_flux_s10;
            components.push(NsbComponent {
                name: "airglow",
                integrated: out.integrated,
                b_flux_s10: out.b_flux_s10,
                v_flux_s10: out.v_flux_s10,
                relative_uncertainty: out.relative_uncertainty,
                metadata: airglow_metadata(self.config.site_profile, query.observer),
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
                NSB_S10_ZP,
            ),
            v_mag: s10_to_surface_brightness(
                v_total.max(S10::new(f64::MIN_POSITIVE)),
                NSB_S10_ZP,
            ),
            components,
            band_diagnostic: super::BandDiagnostic::MONOCHROMATIC_S10_PROXY,
        })
    }

    fn evaluate_airglow(
        &self,
        observer: Observer,
        time: Time<UTC>,
        target: Target,
    ) -> Result<airglow::AirglowOutputs> {
        let profile = self.config.site_profile.profile(observer);
        airglow::Airglow::with_continuum(observer, self.airglow_continuum.clone())
            .with_solar_radio_flux(self.config.solar_radio_flux)
            .with_scale(profile.airglow.scale)
            .compute(time, target)
    }

    fn evaluate_starlight(&self, target: Target) -> Result<starlight::StarlightOutputs> {
        let model = match &self.config.starlight_model {
            StarlightModel::Disabled => {
                return Err(NsbError::Unsupported(
                    concat!(
                        "starlight component requested but no starlight model is configured; ",
                        "use StarlightModel::with_map(...) or StarlightModel::BundledCatalogueMap ",
                        "after the catalogue-derived map is bundled and validated"
                    )
                    .to_string(),
                ));
            }
            StarlightModel::BundledCatalogueMap => starlight::Starlight::catalogue_galactic_model()?,
            StarlightModel::CustomMap(map) => starlight::Starlight::with_map((**map).clone()),
        };
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
                moonlight::KrisciunasSchaefer1991::standard_clear_sky(observer).compute(time, target)
            }
            MoonlightModel::Jones2013Spectral => {
                moonlight::Jones2013Spectral::for_site_profile(observer, self.config.site_profile)
                    .compute(time, target)
            }
        }
    }
}
