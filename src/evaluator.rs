//! NSB evaluator: point evaluation and threshold-period search.
//!
//! The threshold search runs an event-driven pipeline modelled on
//! `siderust::event::stellar::altitude_periods`:
//!
//! 1. Optionally pre-filter to the intersection of *Sun below twilight*
//!    and *target above horizon* sub-windows (cheap analytical engines
//!    inside `siderust`).
//! 2. Inside each surviving candidate window, run a coarse scan with
//!    local bisection refinement to
//!    locate the radiance crossings of `threshold`.
//! 3. Take the complement (darker-than-threshold) inside each candidate
//!    window and return the concatenated list.
//!
//! This collapses ~year-long searches from tens of thousands of full NSB
//! evaluations down to a few hundred, while keeping the per-sample math
//! identical to [`NsbEvaluator::evaluate`].
//!
//! Scientific role:
//! this file is the "scientific orchestrator" of the crate. It does not define
//! new physics by itself; instead, it combines the implemented component models
//! into a site/time/target prediction of sky background.
//!
//! Contribution to the science:
//! the evaluator is where the astronomy geometry becomes operational. It:
//!
//! * turns user inputs into observer and target geometry
//! * computes target altitude and the relevant Sun/Moon geometry
//! * invokes the zodiacal-light, starlight, airglow, and moonlight models
//! * adds those contributions into a total background radiance
//! * supports threshold-window searches that are scientifically equivalent to
//!   repeated point evaluations, but much faster for long observing windows

use crate::components::{airglow, moonlight, starlight, zodiacal};
use crate::error::{NsbError, Result};
use crate::site::Site;
use crate::spectra;
use crate::spectra::airglow_cont::AirglowContinuum;
use crate::NSB_S10_ZP;
use optica::spectrum::SampledSpectrum;
use qtty::angular::Degrees;
use qtty::photometry::{s10_to_surface_brightness, SurfaceBrightness};
use qtty::radiometry::{
    PhotonsPerSquareCentimeterNanosecondSteradian as BandPhotonRadiance, S10s as S10,
};
use qtty::{Quantity, Second, Unit};
use siderust::bodies::Sun as SunBody;
use siderust::coordinates::centers::Geodetic;
use siderust::coordinates::frames::{EclipticMeanJ2000, EquatorialMeanJ2000, ECEF};
use siderust::coordinates::spherical::direction;
use siderust::coordinates::spherical::Direction as SphericalDirection;
use siderust::coordinates::transform::TransformFrame;
use siderust::event::altitude::{AltitudeEventsExt, SearchOpts};
use siderust::event::horizontal::star_horizontal;
use siderust::qtty::{Day, Days, Radian};
use siderust::time::{intersect_periods, Interval as TimePeriod, ModifiedJulianDate, TT};
use tempoch::{Period, Time, JD, MJD, UTC};

bitflags::bitflags! {
    /// Which components to include in the calculation.
    #[derive(Debug, Clone, Copy)]
    pub struct ComponentMask: u8 {
        const ZODIACAL  = 0b0001;
        const STARLIGHT = 0b0010;
        const AIRGLOW   = 0b0100;
        const MOON      = 0b1000;
        const ALL       = Self::ZODIACAL.bits()
                        | Self::STARLIGHT.bits()
                        | Self::AIRGLOW.bits()
                        | Self::MOON.bits();
    }
}

/// Observer location: a named CTAO site or arbitrary geodetic coordinates.
#[derive(Debug, Clone, Copy)]
pub enum Location {
    NamedSite(Site),
    Geodetic(Geodetic<ECEF>),
}

impl Location {
    #[inline]
    pub fn geodetic(self) -> Geodetic<ECEF> {
        match self {
            Self::NamedSite(site) => site.geodetic(),
            Self::Geodetic(geodetic) => geodetic,
        }
    }
}

impl From<Site> for Location {
    fn from(value: Site) -> Self {
        Self::NamedSite(value)
    }
}

impl From<Geodetic<ECEF>> for Location {
    fn from(value: Geodetic<ECEF>) -> Self {
        Self::Geodetic(value)
    }
}

/// Equatorial (ICRS / J2000) target direction.
pub type Target = SphericalDirection<EquatorialMeanJ2000>;

/// Single-instant NSB query.
#[derive(Debug, Clone)]
pub struct PointQuery {
    pub location: Location,
    pub time: Time<UTC>,
    pub target: Target,
    pub components: ComponentMask,
}

/// Threshold-window NSB query: find sub-periods darker than `threshold`.
///
/// The optional `sun_altitude_ceiling` and `target_altitude_floor` knobs
/// pre-filter the search to dark, observable sub-windows. Set them to
/// `None` to disable the pre-filter and recover the legacy uniform-scan
/// semantics (useful for cross-validation).
#[derive(Debug, Clone)]
pub struct ThresholdQuery {
    pub location: Location,
    pub target: Target,
    pub window: Period<UTC>,
    pub threshold: BandPhotonRadiance,
    pub components: ComponentMask,
    /// Coarse scan cadence used to bracket threshold crossings.
    pub sample_step: Second,
    /// Pre-filter: keep only sub-windows where the Sun is at or below this
    /// altitude (e.g. `-18°` for astronomical twilight). `None` disables
    /// the filter.
    pub sun_altitude_ceiling: Option<Degrees>,
    /// Pre-filter: keep only sub-windows where the target is at or above
    /// this altitude (e.g. `0°` for the geometric horizon). `None`
    /// disables the filter.
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
    /// Krisciunas & Schaefer (1991), preserving the previous NSB behavior.
    KrisciunasSchaefer1991,
    /// Jones et al. (2013)-style wavelength-resolved scattered moonlight.
    Jones2013Spectral,
}

/// Model-selection configuration for [`NsbEvaluator`].
#[derive(Debug, Clone, Copy)]
pub struct NsbModelConfig {
    pub moonlight_model: MoonlightModel,
    pub solar_radio_flux: airglow::SolarFluxUnits,
}

impl NsbModelConfig {
    /// Best validated science path. This is the default for new evaluators.
    pub fn best_science() -> Self {
        Self {
            moonlight_model: MoonlightModel::Jones2013Spectral,
            solar_radio_flux: airglow::DEFAULT_SOLAR_RADIO_FLUX,
        }
    }

    /// Historical preset retained for moonlight-model regression.
    pub fn python_parity() -> Self {
        Self {
            moonlight_model: MoonlightModel::KrisciunasSchaefer1991,
            solar_radio_flux: airglow::DEFAULT_SOLAR_RADIO_FLUX,
        }
    }
}

impl Default for NsbModelConfig {
    fn default() -> Self {
        Self::best_science()
    }
}

#[derive(Debug, Clone, Copy)]
struct PreparedPointQuery {
    observer: Geodetic<ECEF>,
    target: Target,
    components: ComponentMask,
}

/// Per-query precomputed values shared by all samples in the threshold loop.
#[derive(Debug, Clone, Copy)]
struct PreparedThresholdQuery {
    observer: Geodetic<ECEF>,
    target: Target,
    components: ComponentMask,
    /// Target ecliptic latitude (J2000 ecliptic) — fixed for the window.
    ecliptic_lat: siderust::qtty::Radians,
    /// Target ecliptic longitude (J2000 ecliptic) — fixed for the window.
    ecliptic_lon: siderust::qtty::Radians,
    /// Constant starlight integrated radiance contribution, when enabled.
    starlight_integrated: BandPhotonRadiance,
}

/// Reusable evaluator with cached spectral inputs.
pub struct NsbEvaluator {
    solar: SampledSpectrum<siderust::qtty::Nanometer, siderust::qtty::length::Meter>,
    airglow_continuum: AirglowContinuum,
    config: NsbModelConfig,
}

impl NsbEvaluator {
    pub fn new() -> Result<Self> {
        Self::with_config(NsbModelConfig::best_science())
    }

    pub fn with_config(config: NsbModelConfig) -> Result<Self> {
        Ok(Self {
            solar: spectra::solar::load()?,
            airglow_continuum: spectra::airglow_cont::load()?,
            config,
        })
    }

    pub fn python_parity() -> Result<Self> {
        Self::with_config(NsbModelConfig::python_parity())
    }

    pub fn config(&self) -> NsbModelConfig {
        self.config
    }

    pub fn evaluate(&self, query: &PointQuery) -> Result<NsbResult> {
        let prepared = Self::prepare_point(query.location, query.target, query.components);
        self.evaluate_full(&prepared, query.time)
    }

    /// Optimized threshold search.
    ///
    /// See module docs for the algorithm. When both pre-filter knobs on
    /// [`ThresholdQuery`] are `None` this still benefits from the
    /// allocation-free inner loop, but degenerates to a single coarse
    /// scan over the full UTC window.
    pub fn periods_below_threshold(&self, query: &ThresholdQuery) -> Result<ThresholdQueryResult> {
        Self::validate_threshold(query)?;

        let prepared = Self::prepare_threshold(query)?;
        let tt_window = utc_period_to_tt_mjd(query.window);
        let step = query.sample_step.to::<Day>();

        let candidate_windows = self.candidate_windows(query, &prepared, tt_window);

        let f = |mjd_tt: ModifiedJulianDate| -> BandPhotonRadiance {
            self.evaluate_integrated(&prepared, mjd_tt)
        };

        let mut darker_periods: Vec<TimePeriod<ModifiedJulianDate>> = Vec::new();
        for cw in candidate_windows {
            let brighter = above_threshold_periods(cw, step, &f, query.threshold);
            let darker = complement_periods(cw, &brighter);
            darker_periods.extend(darker);
        }

        Ok(ThresholdQueryResult {
            threshold: query.threshold,
            periods: darker_periods
                .into_iter()
                .map(|p| tt_mjd_period_to_utc(p, tt_window, query.window))
                .collect(),
        })
    }

    /// Legacy uniform-scan threshold search, retained for cross-validation
    /// and benches.  Equivalent to the pre-optimization implementation:
    /// no pre-filter, full per-sample evaluation.
    #[doc(hidden)]
    pub fn periods_below_threshold_legacy(
        &self,
        query: &ThresholdQuery,
    ) -> Result<ThresholdQueryResult> {
        Self::validate_threshold(query)?;

        let prepared = Self::prepare_point(query.location, query.target, query.components);
        let tt_window = utc_period_to_tt_mjd(query.window);
        let step = query.sample_step.to::<Day>();
        let integrated_at = |mjd_tt: ModifiedJulianDate| -> BandPhotonRadiance {
            let time_utc = tt_mjd_to_utc_time(mjd_tt);
            self.evaluate_full(&prepared, time_utc)
                .expect("prepared NSB threshold query must remain within the evaluator domain")
                .integrated
        };

        let brighter_than_threshold =
            above_threshold_periods(tt_window, step, &integrated_at, query.threshold);
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
        location: Location,
        target: Target,
        components: ComponentMask,
    ) -> PreparedPointQuery {
        PreparedPointQuery {
            observer: location.geodetic(),
            target,
            components,
        }
    }

    fn prepare_threshold(query: &ThresholdQuery) -> Result<PreparedThresholdQuery> {
        let observer = query.location.geodetic();
        let ecl: SphericalDirection<EclipticMeanJ2000> = query.target.to_frame();
        let starlight_integrated = if query.components.contains(ComponentMask::STARLIGHT) {
            starlight::compute()?.integrated
        } else {
            BandPhotonRadiance::new(0.0)
        };
        Ok(PreparedThresholdQuery {
            observer,
            target: query.target,
            components: query.components,
            ecliptic_lat: ecl.lat().to::<Radian>(),
            ecliptic_lon: ecl.lon().to::<Radian>(),
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

    /// Allocation-free integrated-only evaluation for the threshold inner loop.
    fn evaluate_integrated(
        &self,
        prepared: &PreparedThresholdQuery,
        mjd_tt: ModifiedJulianDate,
    ) -> BandPhotonRadiance {
        let time = tt_mjd_to_utc_time(mjd_tt);
        let jd = time.to::<TT>().to::<JD>();
        let hz = star_horizontal(
            prepared.target.ra(),
            prepared.target.dec(),
            &prepared.observer,
            jd,
        );
        let source_zenith = Degrees::new(90.0) - hz.alt();

        let mut total = BandPhotonRadiance::new(0.0);

        if prepared.components.contains(ComponentMask::ZODIACAL) {
            let lambda_sun = siderust::bodies::Sun::ecliptic_longitude_geocentric(jd);
            let delta_lambda = prepared.ecliptic_lon.abs_separation(lambda_sun);
            let out = zodiacal::compute(
                &zodiacal::ZlInputs {
                    beta: prepared.ecliptic_lat,
                    delta_lambda,
                    zenith: source_zenith,
                },
                &self.solar,
            )
            .expect("prepared zodiacal evaluation");
            total += out.integrated;
        }
        if prepared.components.contains(ComponentMask::STARLIGHT) {
            total += prepared.starlight_integrated;
        }
        if prepared.components.contains(ComponentMask::AIRGLOW) {
            let out = self
                .evaluate_airglow(prepared.observer, time, prepared.target)
                .expect("prepared airglow evaluation");
            total += out.integrated;
        }
        if prepared.components.contains(ComponentMask::MOON) {
            let out = self
                .evaluate_moonlight(prepared.observer, time, prepared.target)
                .expect("prepared moon evaluation");
            total += out.integrated;
        }

        total
    }

    fn evaluate_full(&self, query: &PreparedPointQuery, time: Time<UTC>) -> Result<NsbResult> {
        let jd = time.to::<TT>().to::<JD>();
        let hz = star_horizontal(query.target.ra(), query.target.dec(), &query.observer, jd);
        let source_zenith = Degrees::new(90.0) - hz.alt();
        let ecl: SphericalDirection<EclipticMeanJ2000> = query.target.to_frame();
        let ecliptic_lat = ecl.lat().to::<Radian>();
        let ecliptic_lon = ecl.lon().to::<Radian>();
        let lambda_sun = siderust::bodies::Sun::ecliptic_longitude_geocentric(jd);
        let delta_lambda = ecliptic_lon.abs_separation(lambda_sun);

        let mut components = Vec::new();
        let mut total = BandPhotonRadiance::new(0.0);
        let (mut b_total, mut v_total) = (S10::new(0.0), S10::new(0.0));

        if query.components.contains(ComponentMask::ZODIACAL) {
            let out = zodiacal::compute(
                &zodiacal::ZlInputs {
                    beta: ecliptic_lat,
                    delta_lambda,
                    zenith: source_zenith,
                },
                &self.solar,
            )?;
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
            let out = starlight::compute()?;
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
            b_mag: s10_to_surface_brightness(b_total.max(S10::new(f64::MIN_POSITIVE)), NSB_S10_ZP),
            v_mag: s10_to_surface_brightness(v_total.max(S10::new(f64::MIN_POSITIVE)), NSB_S10_ZP),
            components,
        })
    }

    fn evaluate_airglow(
        &self,
        location: Geodetic<ECEF>,
        time: Time<UTC>,
        target: Target,
    ) -> Result<airglow::AirglowOutputs> {
        airglow::Airglow::with_continuum(location, self.airglow_continuum.clone())
            .with_solar_radio_flux(self.config.solar_radio_flux)
            .compute(time, target)
    }

    fn evaluate_moonlight(
        &self,
        location: Geodetic<ECEF>,
        time: Time<UTC>,
        target: Target,
    ) -> Result<moonlight::MoonOutputs> {
        match self.config.moonlight_model {
            MoonlightModel::KrisciunasSchaefer1991 => {
                moonlight::KrisciunasSchaefer1991::standard_clear_sky(location)
                    .compute(time, target)
            }
            MoonlightModel::Jones2013Spectral => {
                moonlight::Jones2013Spectral::standard_clear_sky(location).compute(time, target)
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
) -> Vec<TimePeriod<ModifiedJulianDate>>
where
    V: Unit,
    F: Fn(ModifiedJulianDate) -> Quantity<V>,
{
    if window.start >= window.end || step <= Days::new(0.0) {
        return Vec::new();
    }

    let mut periods = Vec::new();
    let mut t0 = window.start;
    let mut y0 = f(t0);
    let mut above0 = y0 > threshold;
    let mut open_start = above0.then_some(window.start);

    while t0 < window.end {
        let t1 = add_days_clamped(t0, step, window.end);
        if t1 <= t0 {
            break;
        }

        let y1 = f(t1);
        let above1 = y1 > threshold;
        if above0 != above1 {
            let crossing = refine_threshold_crossing(t0, y0, t1, y1, f, threshold);
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

    periods
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
) -> ModifiedJulianDate
where
    V: Unit,
    F: Fn(ModifiedJulianDate) -> Quantity<V>,
{
    let lo_above = y_lo > threshold;
    for _ in 0..48 {
        let mid = midpoint_mjd(lo, hi);
        if mid <= lo || mid >= hi {
            break;
        }
        let y_mid = f(mid);
        if y_mid == threshold {
            return mid;
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

    linear_crossing_estimate(lo, y_lo, hi, y_hi, threshold)
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
