//! NSB evaluator: point evaluation and threshold-period search.
//!
//! This module is the library-facing orchestration layer. It accepts typed
//! observing inputs, invokes the physical component models, sums their
//! radiances, and provides an event-driven planning search. CLI concerns such
//! as named-site parsing and timestamp parsing intentionally live outside this
//! crate.

use crate::components::airglow::AirglowContinuum;
use crate::components::zodiacal::{ZodiacalExtinction, ZodiacalLight};
use crate::components::{airglow, moonlight, starlight};
use crate::error::{NsbError, Result};
use crate::site::SiteProfileId;
use crate::NSB_S10_ZP;
use qtty::angular::Degrees;
use qtty::photometry::{s10_to_surface_brightness, SurfaceBrightness};
use qtty::radiometry::{
    PhotonsPerSquareCentimeterNanosecondSteradian as BandPhotonRadiance, S10s as S10,
};
use qtty::{Quantity, Second, Unit};
use siderust::bodies::Sun as SunBody;
use siderust::coordinates::centers::Geodetic;
use siderust::coordinates::frames::{EquatorialMeanJ2000, ECEF};
use siderust::coordinates::spherical::direction;
use siderust::coordinates::spherical::Direction as SphericalDirection;
use siderust::event::altitude::{AltitudeEventsExt, SearchOpts};
use siderust::qtty::{Day, Days};
use siderust::time::{intersect_periods, Interval as TimePeriod, ModifiedJulianDate, TT};
use tempoch::{Period, Time, MJD, UTC};

bitflags::bitflags! {
    /// Which components to include in the calculation.
    #[derive(Debug, Clone, Copy)]
    pub struct ComponentMask: u8 {
        const ZODIACAL  = 0b0001;
        const STARLIGHT = 0b0010;
        const AIRGLOW   = 0b0100;
        const MOON      = 0b1000;

        /// Generic clear-sky component set evaluable with bundled runtime data.
        const DEFAULT   = Self::ZODIACAL.bits()
                        | Self::AIRGLOW.bits()
                        | Self::MOON.bits();

        /// Generic clear-sky alias for the default component set.
        ///
        /// This intentionally excludes unresolved Galactic starlight until a
        /// catalogue-derived map with provenance is bundled and validated.
        const ALL       = Self::DEFAULT.bits();

        /// All implemented components, including opt-in components that may
        /// require caller-supplied or future bundled data.
        const ALL_SUPPORTED = Self::DEFAULT.bits()
                            | Self::STARLIGHT.bits();
    }
}

/// Ground observer geodetic location.
pub type Observer = Geodetic<ECEF>;

/// Equatorial (ICRS / J2000) target direction.
pub type Target = SphericalDirection<EquatorialMeanJ2000>;

/// Single-instant NSB query.
#[derive(Debug, Clone)]
pub struct PointQuery {
    pub observer: Observer,
    pub time: Time<UTC>,
    pub target: Target,
    pub components: ComponentMask,
}

/// Threshold-window NSB query: find sub-periods darker than `threshold`.
///
/// The optional `sun_altitude_ceiling` and `target_altitude_floor` knobs
/// pre-filter the search to dark, observable sub-windows. Set them to `None`
/// to disable the pre-filter and recover the legacy uniform-scan semantics.
///
/// Threshold search is fail-closed and all-or-nothing: if any selected
/// component cannot be evaluated at a sampled or refined timestamp,
/// [`NsbEvaluator::periods_below_threshold`] returns `Err` and no observing
/// windows are reported.
#[derive(Debug, Clone)]
pub struct ThresholdQuery {
    pub observer: Observer,
    pub target: Target,
    pub window: Period<UTC>,
    pub threshold: BandPhotonRadiance,
    pub components: ComponentMask,
    /// Coarse scan cadence used to bracket threshold crossings.
    pub sample_step: Second,
    /// Pre-filter: keep only sub-windows where the Sun is at or below this
    /// altitude (e.g. `-18°` for astronomical twilight). `None` disables it.
    pub sun_altitude_ceiling: Option<Degrees>,
    /// Pre-filter: keep only sub-windows where the target is at or above this
    /// altitude (e.g. `0°` for the geometric horizon). `None` disables it.
    pub target_altitude_floor: Option<Degrees>,
}

impl ThresholdQuery {
    pub const DEFAULT_SAMPLE_STEP: Second = Second::new(600.0);
    pub const DEFAULT_SUN_ALTITUDE_CEILING: Degrees = Degrees::new(-18.0);
    pub const DEFAULT_TARGET_ALTITUDE_FLOOR: Degrees = Degrees::new(0.0);
}

#[derive(Debug, Clone)]
pub struct NsbComponent {
    pub name: &'static str,
    pub integrated: BandPhotonRadiance,
    pub b_flux_s10: S10,
    pub v_flux_s10: S10,
}

#[derive(Debug, Clone)]
pub struct NsbResult {
    pub integrated: BandPhotonRadiance,
    pub b_mag: SurfaceBrightness,
    pub v_mag: SurfaceBrightness,
    pub components: Vec<NsbComponent>,
}

#[derive(Debug, Clone)]
pub struct ThresholdQueryResult {
    pub threshold: BandPhotonRadiance,
    pub periods: Vec<Period<UTC>>,
}

/// Scattered-moonlight model used by [`NsbEvaluator`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MoonlightModel {
    KrisciunasSchaefer1991,
    Jones2013Spectral,
}

/// Directional unresolved-starlight model used by [`NsbEvaluator`].
#[derive(Debug, Clone)]
pub enum StarlightModel {
    /// Do not configure a starlight model. Requests that include
    /// [`ComponentMask::STARLIGHT`] fail explicitly instead of silently loading
    /// a missing or proxy map.
    Disabled,
    /// Load the bundled catalogue-derived Galactic starlight map.
    ///
    /// This variant remains opt-in until `starlight_galactic_map_v1.csv` is
    /// generated from a real catalogue with provenance, bundled with the crate,
    /// and quantitatively validated for science use.
    BundledCatalogueMap,
    /// Use a caller-supplied Galactic starlight map.
    CustomMap(Box<starlight::StarlightMap>),
}

impl StarlightModel {
    pub fn disabled() -> Self {
        Self::Disabled
    }

    pub fn bundled_catalogue_map() -> Self {
        Self::BundledCatalogueMap
    }

    pub fn with_map(map: starlight::StarlightMap) -> Self {
        Self::CustomMap(Box::new(map))
    }
}

/// Model-selection configuration for [`NsbEvaluator`].
#[derive(Debug, Clone)]
pub struct NsbModelConfig {
    pub moonlight_model: MoonlightModel,
    pub site_profile: SiteProfileId,
    pub starlight_model: StarlightModel,
    pub solar_radio_flux: airglow::SolarFluxUnits,
    pub zodiacal_extinction: ZodiacalExtinction,
}

impl NsbModelConfig {
    /// Generic clear-sky configuration for new evaluations.
    ///
    /// This is the library default used by [`NsbEvaluator::new`] and
    /// [`Default`]. It is suitable as an explicit development/planning baseline,
    /// not as a named validated science preset. Starlight is disabled until a
    /// catalogue-derived bundled map is generated, bundled, and quantitatively
    /// validated. Callers that need starlight can opt into a custom map with
    /// [`StarlightModel::with_map`] or into the future bundled catalogue map
    /// with [`StarlightModel::BundledCatalogueMap`].
    pub fn generic_clear_sky() -> Self {
        Self {
            moonlight_model: MoonlightModel::Jones2013Spectral,
            site_profile: SiteProfileId::GenericClearSky,
            starlight_model: StarlightModel::Disabled,
            solar_radio_flux: airglow::DEFAULT_SOLAR_RADIO_FLUX,
            zodiacal_extinction: ZodiacalExtinction::Noll2012Approx,
        }
    }

    /// CTAO-North planning configuration.
    ///
    /// This selects explicit CTA-N profile metadata and component assumptions.
    /// The profile is not marked as fully site-calibrated until dedicated CTAO
    /// validation inputs are bundled.
    pub fn cta_n_planning() -> Self {
        Self::generic_clear_sky().with_site_profile(SiteProfileId::CtaNorth)
    }

    /// CTAO-South planning configuration.
    ///
    /// This selects explicit CTA-S profile metadata and component assumptions.
    /// The profile is not marked as fully site-calibrated until dedicated CTAO
    /// validation inputs are bundled.
    pub fn cta_s_planning() -> Self {
        Self::generic_clear_sky().with_site_profile(SiteProfileId::CtaSouth)
    }

    pub fn with_site_profile(mut self, site_profile: SiteProfileId) -> Self {
        self.site_profile = site_profile;
        self
    }

    /// Historical preset retained for regression tests.
    ///
    /// This intentionally selects legacy-compatible model choices and should not
    /// be used as a current science or planning preset.
    #[doc(hidden)]
    pub fn python_parity() -> Self {
        Self {
            moonlight_model: MoonlightModel::KrisciunasSchaefer1991,
            site_profile: SiteProfileId::GenericClearSky,
            starlight_model: StarlightModel::Disabled,
            solar_radio_flux: airglow::DEFAULT_SOLAR_RADIO_FLUX,
            zodiacal_extinction: ZodiacalExtinction::Noll2012Approx,
        }
    }
}

impl Default for NsbModelConfig {
    fn default() -> Self {
        Self::generic_clear_sky()
    }
}

#[derive(Debug, Clone, Copy)]
struct PreparedPointQuery {
    observer: Observer,
    target: Target,
    components: ComponentMask,
}

#[derive(Debug, Clone, Copy)]
struct PreparedThresholdQuery {
    observer: Observer,
    target: Target,
    components: ComponentMask,
    starlight_integrated: BandPhotonRadiance,
}

/// Reusable NSB evaluator with cached spectral inputs.
pub struct NsbEvaluator {
    zodiacal: ZodiacalLight,
    airglow_continuum: AirglowContinuum,
    config: NsbModelConfig,
}

impl NsbEvaluator {
    /// Create an evaluator with the explicit generic clear-sky configuration.
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

fn utc_time_to_tt_mjd(time: Time<UTC>) -> ModifiedJulianDate {
    ModifiedJulianDate::from(time.to::<TT>().to::<MJD>())
}

fn tt_mjd_to_utc_time(time: ModifiedJulianDate) -> Time<UTC> {
    tempoch::Time::<TT>::from(time).to::<UTC>()
}

fn utc_period_to_tt_mjd(window: Period<UTC>) -> TimePeriod<ModifiedJulianDate> {
    TimePeriod::new(
        utc_time_to_tt_mjd(window.start),
        utc_time_to_tt_mjd(window.end),
    )
}

fn above_threshold_periods<V, F>(
    window: TimePeriod<ModifiedJulianDate>,
    step: Days,
    f: &F,
    threshold: Quantity<V>,
) -> Result<Vec<TimePeriod<ModifiedJulianDate>>>
where
    V: Unit,
    F: Fn(ModifiedJulianDate) -> Result<Quantity<V>>,
{
    if window.start >= window.end || step <= Days::new(0.0) {
        return Ok(Vec::new());
    }

    let mut periods = Vec::new();
    let mut t0 = window.start;
    let mut y0 = f(t0)?;
    let mut above0 = y0 > threshold;
    let mut open_start = above0.then_some(window.start);

    while t0 < window.end {
        let t1 = add_days_clamped(t0, step, window.end);
        if t1 <= t0 {
            break;
        }

        let y1 = f(t1)?;
        let above1 = y1 > threshold;
        if above0 != above1 {
            let crossing = refine_threshold_crossing(t0, y0, t1, y1, f, threshold)?;
            if above0 {
                if let Some(start) = open_start.take() {
                    push_non_empty_period(&mut periods, start, crossing);
                }
            } else {
                open_start = Some(crossing);
            }
        }

        t0 = t1;
        y0 = y1;
        above0 = above1;
    }

    if let Some(start) = open_start {
        push_non_empty_period(&mut periods, start, window.end);
    }

    Ok(periods)
}

fn complement_periods(
    window: TimePeriod<ModifiedJulianDate>,
    periods: &[TimePeriod<ModifiedJulianDate>],
) -> Vec<TimePeriod<ModifiedJulianDate>> {
    let mut out = Vec::new();
    let mut cursor = window.start;

    for period in periods {
        let start = period.start.max(window.start);
        let end = period.end.min(window.end);
        if end <= window.start || start >= window.end || start >= end {
            continue;
        }
        push_non_empty_period(&mut out, cursor, start);
        cursor = cursor.max(end);
    }

    push_non_empty_period(&mut out, cursor, window.end);
    out
}

fn refine_threshold_crossing<V, F>(
    mut lo: ModifiedJulianDate,
    mut y_lo: Quantity<V>,
    mut hi: ModifiedJulianDate,
    mut y_hi: Quantity<V>,
    f: &F,
    threshold: Quantity<V>,
) -> Result<ModifiedJulianDate>
where
    V: Unit,
    F: Fn(ModifiedJulianDate) -> Result<Quantity<V>>,
{
    let lo_above = y_lo > threshold;
    for _ in 0..48 {
        let mid = midpoint_mjd(lo, hi);
        if mid <= lo || mid >= hi {
            break;
        }
        let y_mid = f(mid)?;
        if y_mid == threshold {
            return Ok(mid);
        }
        if (y_mid > threshold) == lo_above {
            lo = mid;
            y_lo = y_mid;
        } else {
            hi = mid;
            y_hi = y_mid;
        }
        if (hi.raw() - lo.raw()).abs() <= Days::new(1.0e-10) {
            break;
        }
    }

    Ok(linear_crossing_estimate(lo, y_lo, hi, y_hi, threshold))
}

fn linear_crossing_estimate<V>(
    lo: ModifiedJulianDate,
    y_lo: Quantity<V>,
    hi: ModifiedJulianDate,
    y_hi: Quantity<V>,
    threshold: Quantity<V>,
) -> ModifiedJulianDate
where
    V: Unit,
{
    let denom = y_hi.value() - y_lo.value();
    if !denom.is_finite() || denom == 0.0 {
        return midpoint_mjd(lo, hi);
    }
    let frac = ((threshold.value() - y_lo.value()) / denom).clamp(0.0, 1.0);
    let lo_raw = lo.raw().value();
    let hi_raw = hi.raw().value();
    ModifiedJulianDate::new(lo_raw + (hi_raw - lo_raw) * frac)
}

fn midpoint_mjd(lo: ModifiedJulianDate, hi: ModifiedJulianDate) -> ModifiedJulianDate {
    ModifiedJulianDate::new(0.5 * (lo.raw().value() + hi.raw().value()))
}

fn add_days_clamped(
    time: ModifiedJulianDate,
    delta: Days,
    end: ModifiedJulianDate,
) -> ModifiedJulianDate {
    let next = ModifiedJulianDate::new(time.raw().value() + delta.value());
    next.min(end)
}

fn push_non_empty_period(
    periods: &mut Vec<TimePeriod<ModifiedJulianDate>>,
    start: ModifiedJulianDate,
    end: ModifiedJulianDate,
) {
    if start < end {
        periods.push(TimePeriod::new(start, end));
    }
}

fn tt_mjd_period_to_utc(
    window: TimePeriod<ModifiedJulianDate>,
    query_window_tt: TimePeriod<ModifiedJulianDate>,
    query_window_utc: Period<UTC>,
) -> Period<UTC> {
    let start = if window.start == query_window_tt.start {
        query_window_utc.start
    } else {
        tt_mjd_to_utc_time(window.start)
    };
    let end = if window.end == query_window_tt.end {
        query_window_utc.end
    } else {
        tt_mjd_to_utc_time(window.end)
    };
    Period::new(start, end)
}
