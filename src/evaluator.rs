//! NSB evaluator: point evaluation and threshold-period search.
//!
//! The threshold search runs an event-driven pipeline modelled on
//! `siderust::calculus::stellar::altitude_periods`:
//!
//! 1. Optionally pre-filter to the intersection of *Sun below twilight*
//!    and *target above horizon* sub-windows (cheap analytical engines
//!    inside `siderust`).
//! 2. Inside each surviving candidate window, run a coarse scan with
//!    Brent refinement via `siderust::calculus::math_core::intervals` to
//!    locate the radiance crossings of `threshold`.
//! 3. Take the complement (darker-than-threshold) inside each candidate
//!    window and return the concatenated list.
//!
//! This collapses ~year-long searches from tens of thousands of full NSB
//! evaluations down to a few hundred, while keeping the per-sample math
//! identical to [`NsbEvaluator::evaluate`].

use crate::components::{airglow, moonlight, starlight, zodiacal};
use crate::error::{NsbError, Result};
use crate::site::Site;
use crate::spectra;
use qtty::angular::Degrees;
use qtty::photometry::{band_flux_to_surface_brightness, SurfaceBrightness};
use qtty::radiometry::{
    PhotonsPerSquareCentimeterNanosecondSteradian as BandPhotonRadiance, S10s as S10,
};
use qtty::Second;
use siderust::bodies::{Moon, Sun as SunBody};
use siderust::calculus::altitude::AltitudePeriodsProvider;
use siderust::calculus::horizontal::star_horizontal;
use siderust::calculus::math_core::intervals;
use siderust::coordinates::centers::Geodetic;
use siderust::coordinates::frames::{ECEF, EclipticMeanJ2000, EquatorialMeanJ2000};
use siderust::coordinates::spherical::direction;
use siderust::coordinates::spherical::Direction as SphericalDirection;
use siderust::coordinates::transform::TransformFrame;
use siderust::qtty::{Days, Kilometer, Radian};
use siderust::spectra::SampledSpectrum;
use siderust::time::{intersect_periods, ModifiedJulianDate, Period as TimePeriod, TT};
use tempoch::{Period, Time, MJD, UTC};

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
    /// Target ecliptic latitude (radians, J2000 ecliptic) — fixed for the window.
    ecliptic_lat_rad: f64,
    /// Target ecliptic longitude (radians, J2000 ecliptic) — fixed for the window.
    ecliptic_lon_rad: f64,
    /// Constant starlight integrated radiance contribution, when enabled.
    starlight_integrated: BandPhotonRadiance,
}

/// Reusable evaluator with cached spectral inputs.
pub struct NsbEvaluator {
    solar: SampledSpectrum<siderust::qtty::Nanometer, siderust::qtty::length::Meter, f64>,
}

impl NsbEvaluator {
    pub fn new() -> Result<Self> {
        Ok(Self {
            solar: spectra::solar::load()?,
        })
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
        let step = sample_step_to_days(query.sample_step);

        let candidate_windows = self.candidate_windows(query, &prepared, tt_window);

        let f =
            |mjd_tt: ModifiedJulianDate| -> BandPhotonRadiance { self.evaluate_integrated(&prepared, mjd_tt) };

        let mut darker_periods: Vec<TimePeriod<ModifiedJulianDate>> = Vec::new();
        for cw in candidate_windows {
            let brighter = intervals::above_threshold_periods(cw, step, &f, query.threshold);
            let darker = intervals::complement(cw, &brighter);
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
        let step = sample_step_to_days(query.sample_step);
        let integrated_at = |mjd_tt: ModifiedJulianDate| -> BandPhotonRadiance {
            let time_utc = tt_mjd_to_utc_time(mjd_tt);
            self.evaluate_full(&prepared, time_utc)
                .expect("prepared NSB threshold query must remain within the evaluator domain")
                .integrated
        };

        let brighter_than_threshold =
            intervals::above_threshold_periods(tt_window, step, &integrated_at, query.threshold);
        let darker_than_threshold = intervals::complement(tt_window, &brighter_than_threshold);

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
            ecliptic_lat_rad: ecl.lat().to::<Radian>().value(),
            ecliptic_lon_rad: ecl.lon().to::<Radian>().value(),
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
            let nights = SunBody.below_threshold(prepared.observer, tt_window, sun_max);
            current = intersect_periods(&current, &nights);
            if current.is_empty() {
                return current;
            }
        }
        if let Some(target_min) = query.target_altitude_floor {
            let target_dir = direction::ICRS::new(prepared.target.ra(), prepared.target.dec());
            let above =
                target_dir.above_threshold(prepared.observer, tt_window, target_min);
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
        let jd = siderust::time::JulianDate::from_tempoch_utc(time);
        let hz = star_horizontal(prepared.target.ra(), prepared.target.dec(), &prepared.observer, jd);
        let altitude_deg = hz.alt().value();
        let source_zenith = Degrees::new(90.0 - altitude_deg);
        let zenith_deg = source_zenith.value();

        let mut total = BandPhotonRadiance::new(0.0);

        if prepared.components.contains(ComponentMask::ZODIACAL) {
            let lambda_sun = siderust::bodies::Sun::ecliptic_longitude_geocentric(jd);
            let delta_lambda = (prepared.ecliptic_lon_rad - lambda_sun.value())
                .rem_euclid(std::f64::consts::TAU);
            let delta_lambda = if delta_lambda > std::f64::consts::PI {
                std::f64::consts::TAU - delta_lambda
            } else {
                delta_lambda
            };
            let out = zodiacal::compute(
                &zodiacal::ZlInputs {
                    beta_rad: prepared.ecliptic_lat_rad,
                    delta_lambda_rad: delta_lambda,
                    zenith_deg,
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
            let out = airglow::compute(&airglow::AgInputs { altitude_deg })
                .expect("prepared airglow evaluation");
            total += out.integrated;
        }
        if prepared.components.contains(ComponentMask::MOON) {
            let moon_pos = Moon::get_horizontal::<Kilometer>(jd, prepared.observer);
            let moon_dir = moon_pos.direction();
            let moon_zenith = Degrees::new(90.0 - moon_dir.alt().value());
            let separation = hz.angular_separation(&moon_dir);
            let phase = Moon::phase_geocentric(jd);
            let out = moonlight::compute(&moonlight::MoonInputs {
                separation,
                moon_zenith,
                phase,
                source_zenith,
            })
            .expect("prepared moon evaluation");
            total += out.integrated;
        }

        total
    }

    fn evaluate_full(&self, query: &PreparedPointQuery, time: Time<UTC>) -> Result<NsbResult> {
        let jd = siderust::time::JulianDate::from_tempoch_utc(time);
        let hz = star_horizontal(query.target.ra(), query.target.dec(), &query.observer, jd);
        let altitude = hz.alt();
        let altitude_deg = altitude.value();
        let source_zenith = Degrees::new(90.0 - altitude_deg);
        let zenith_deg = source_zenith.value();
        let ecl: SphericalDirection<EclipticMeanJ2000> = query.target.to_frame();
        let ecliptic_lat = ecl.lat().to::<Radian>();
        let ecliptic_lon = ecl.lon().to::<Radian>();
        let lambda_sun = siderust::bodies::Sun::ecliptic_longitude_geocentric(jd);
        let delta_lambda = ecliptic_lon.abs_separation(lambda_sun).value();

        let mut components = Vec::new();
        let mut total = BandPhotonRadiance::new(0.0);
        let (mut b_total, mut v_total) = (0.0, 0.0);

        if query.components.contains(ComponentMask::ZODIACAL) {
            let out = zodiacal::compute(
                &zodiacal::ZlInputs {
                    beta_rad: ecliptic_lat.value(),
                    delta_lambda_rad: delta_lambda,
                    zenith_deg,
                },
                &self.solar,
            )?;
            total += out.integrated;
            b_total += out.b_flux_s10.value();
            v_total += out.v_flux_s10.value();
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
            b_total += out.b_flux_s10.value();
            v_total += out.v_flux_s10.value();
            components.push(NsbComponent {
                name: "starlight",
                integrated: out.integrated,
                b_flux_s10: out.b_flux_s10,
                v_flux_s10: out.v_flux_s10,
            });
        }
        if query.components.contains(ComponentMask::AIRGLOW) {
            let out = airglow::compute(&airglow::AgInputs { altitude_deg })?;
            total += out.integrated;
            b_total += out.b_flux_s10.value();
            v_total += out.v_flux_s10.value();
            components.push(NsbComponent {
                name: "airglow",
                integrated: out.integrated,
                b_flux_s10: out.b_flux_s10,
                v_flux_s10: out.v_flux_s10,
            });
        }
        if query.components.contains(ComponentMask::MOON) {
            let moon_pos = Moon::get_horizontal::<Kilometer>(jd, query.observer);
            let moon_dir = moon_pos.direction();
            let moon_zenith = Degrees::new(90.0 - moon_dir.alt().value());
            let separation = hz.angular_separation(&moon_dir);
            let phase = Moon::phase_geocentric(jd);
            let out = moonlight::compute(&moonlight::MoonInputs {
                separation,
                moon_zenith,
                phase,
                source_zenith,
            })?;
            total += out.integrated;
            b_total += out.b_flux_s10.value();
            v_total += out.v_flux_s10.value();
            components.push(NsbComponent {
                name: "moon",
                integrated: out.integrated,
                b_flux_s10: out.b_flux_s10,
                v_flux_s10: out.v_flux_s10,
            });
        }

        Ok(NsbResult {
            integrated: total,
            b_mag: band_flux_to_surface_brightness(b_total.max(f64::MIN_POSITIVE), 27.78),
            v_mag: band_flux_to_surface_brightness(v_total.max(f64::MIN_POSITIVE), 27.78),
            components,
        })
    }
}

fn sample_step_to_days(step: Second) -> Days {
    Days::new(step.value() / 86_400.0)
}

fn utc_time_to_tt_mjd(time: Time<UTC>) -> ModifiedJulianDate {
    ModifiedJulianDate::from(time.to::<TT>().to::<MJD>())
}

fn tt_mjd_to_utc_time(time: ModifiedJulianDate) -> Time<UTC> {
    let tt_mjd: tempoch::ModifiedJulianDate<TT> = time.into();
    tt_mjd.to_time().to::<UTC>()
}

fn utc_period_to_tt_mjd(window: Period<UTC>) -> TimePeriod<ModifiedJulianDate> {
    TimePeriod::new(
        utc_time_to_tt_mjd(window.start),
        utc_time_to_tt_mjd(window.end),
    )
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
