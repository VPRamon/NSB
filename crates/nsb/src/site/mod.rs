//! Scientific site profiles, shared atmospheric assumptions, and calibration metadata.
//!
//! NSB deliberately separates observer location from scientific site profiles.
//! A location answers where the observer is; [`SiteProfileId`] / [`SiteProfileSpec`]
//! answer which NSB assumptions and evidence-backed calibration maturity are
//! selected. Observatory- or project-named planning presets (for example CTAO
//! North/South) belong at an application or data layer; the core crate only
//! provides the generic mechanism to supply those assumptions.

/// Shared atmospheric assumptions used by site-aware NSB components.
pub(crate) mod atmosphere;
/// Versioned evidence contract for dedicated site-calibration assets.
pub(crate) mod calibration;

pub use atmosphere::AtmosphericConditions;

use qtty::dimensionless::Ratios;
use siderust::coordinates::centers::Geodetic;
use siderust::coordinates::frames::ECEF;
use siderust::qtty::{Kilometer, Kilometers};
use std::borrow::Cow;

/// Stable string-backed identifier for a scientific site profile.
///
/// This identifies assumptions and calibration maturity, not an observatory or
/// physical location. Callers may introduce arbitrary identifiers without
/// modifying this crate.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct SiteProfileId {
    id: Cow<'static, str>,
}

impl SiteProfileId {
    /// Generic clear-sky fallback derived from the query observer altitude.
    pub const GENERIC_CLEAR_SKY: Self = Self {
        id: Cow::Borrowed("generic-clear-sky"),
    };

    /// Construct a profile identifier from a borrowed or owned string.
    ///
    /// Prefer `'static` string literals when the identifier is known at compile
    /// time so the identifier remains allocation-free.
    pub fn new(id: impl Into<Cow<'static, str>>) -> Self {
        Self { id: id.into() }
    }

    /// Stable maturity-bearing profile identifier string.
    pub fn as_str(&self) -> &str {
        &self.id
    }
}

impl AsRef<str> for SiteProfileId {
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}

impl std::fmt::Display for SiteProfileId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

impl From<&'static str> for SiteProfileId {
    fn from(value: &'static str) -> Self {
        Self::new(value)
    }
}

impl From<String> for SiteProfileId {
    fn from(value: String) -> Self {
        Self::new(value)
    }
}

/// Scientific maturity of a named site profile.
///
/// Additional maturity labels may be added; match with a wildcard.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum CalibrationStatus {
    /// Generic, location-agnostic fallback; not a named-site calibration.
    GenericFallback,
    /// Named-site planning preset with explicit assumptions and provenance.
    PlanningPreset,
    /// Dedicated site-calibrated profile validated against site reference data.
    Calibrated,
}

/// How representative altitude is chosen when resolving a profile.
#[derive(Debug, Clone, Copy, PartialEq)]
#[non_exhaustive]
pub enum RepresentativeAltitude {
    /// Use the query observer's altitude.
    FromObserver,
    /// Use an explicit representative altitude for the atmospheric assumptions.
    Fixed(Kilometers),
}

/// How atmospheric conditions are obtained when resolving a profile.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub enum AtmosphereSource {
    /// Derive pressure from the query observer altitude (generic clear-sky).
    FromObserverAltitude,
    /// Use fixed pressure/Rayleigh/Mie assumptions regardless of observer altitude.
    Fixed(AtmosphericConditions),
}

/// Airglow-side calibration assumptions associated with a site profile.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub struct AirglowSiteCalibration {
    /// Multiplicative scale applied to the bundled continuum template.
    pub scale: Ratios,
    /// Continuum template used by the profile.
    pub template: Cow<'static, str>,
    /// Machine-readable provenance note for the template and scale.
    pub provenance: Cow<'static, str>,
    /// Human-readable calibration limitations.
    pub assumptions: Cow<'static, str>,
}

impl AirglowSiteCalibration {
    /// Bundled Paranal-derived continuum with a neutral site scale.
    ///
    /// This is the shared planning/generic airglow calibration used when no
    /// dedicated site continuum scale is available. Callers constructing custom
    /// profiles may reuse it or supply their own evidence-backed values.
    pub fn skycalc_neutral() -> Self {
        Self {
            scale: Ratios::new(1.0),
            template: Cow::Borrowed("NSB/data/airglow_cont.dat"),
            provenance: Cow::Borrowed(concat!(
                "Bundled Paranal-derived (Noll/SkyCalc/FORS1) empirical continuum ",
                "reused as an explicit generic/planning proxy; neutral site scale; ",
                "not site-calibrated."
            )),
            assumptions: Cow::Borrowed(concat!(
                "No site-specific airglow continuum scale is bundled by the core ",
                "library; named application-layer profiles should record this ",
                "explicitly instead of silently claiming a calibrated site airglow ",
                "model. Arbitrary-location and Paranal-location results remain ",
                "planning approximations unless an explicit validated scientific ",
                "profile is selected; provenance from Paranal is not calibration ",
                "evidence for the observer location."
            )),
        }
    }
}

/// Caller-supplied scientific profile that can be resolved against an observer.
///
/// Construct with [`Self::generic_clear_sky`] or [`Self::planning`].
/// Scientific inputs may be customized through explicit builders, but maturity
/// is intentionally not caller-settable: a public profile cannot self-promote
/// to [`CalibrationStatus::Calibrated`] without an evidence-backed admission
/// path in the core library. Observatory catalogs and named project presets live
/// outside this crate.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub struct SiteProfileSpec {
    id: SiteProfileId,
    name: Cow<'static, str>,
    calibration_status: CalibrationStatus,
    representative_altitude: RepresentativeAltitude,
    atmosphere: AtmosphereSource,
    atmosphere_provenance: Cow<'static, str>,
    airglow: AirglowSiteCalibration,
}

impl SiteProfileSpec {
    /// Generic clear-sky planning configuration.
    ///
    /// Pressure is derived from the query observer altitude when resolved.
    pub fn generic_clear_sky() -> Self {
        Self {
            id: SiteProfileId::GENERIC_CLEAR_SKY,
            name: Cow::Borrowed("generic-clear-sky"),
            calibration_status: CalibrationStatus::GenericFallback,
            representative_altitude: RepresentativeAltitude::FromObserver,
            atmosphere: AtmosphereSource::FromObserverAltitude,
            atmosphere_provenance: Cow::Borrowed(concat!(
                "Pressure estimated from observer altitude with the NSB ",
                "generic barometric fallback; Rayleigh scale height and Mie ",
                "parameters use the bundled clear-sky defaults."
            )),
            airglow: AirglowSiteCalibration::skycalc_neutral(),
        }
    }

    /// Explicit planning preset with fixed atmospheric assumptions.
    ///
    /// Use this for observatory- or project-named planning profiles defined
    /// outside the core crate. Additional Airglow assumptions can be supplied
    /// with [`Self::with_airglow_calibration`] without changing maturity.
    pub fn planning(
        id: impl Into<Cow<'static, str>>,
        name: impl Into<Cow<'static, str>>,
        representative_altitude: Kilometers,
        atmosphere: AtmosphericConditions,
        atmosphere_provenance: impl Into<Cow<'static, str>>,
    ) -> Self {
        let id = SiteProfileId::new(id);
        let name = name.into();
        Self {
            id,
            name,
            calibration_status: CalibrationStatus::PlanningPreset,
            representative_altitude: RepresentativeAltitude::Fixed(representative_altitude),
            atmosphere: AtmosphereSource::Fixed(atmosphere),
            atmosphere_provenance: atmosphere_provenance.into(),
            airglow: AirglowSiteCalibration::skycalc_neutral(),
        }
    }

    /// Replace the Airglow assumptions without changing scientific maturity.
    ///
    /// This permits caller-defined planning inputs while keeping maturity
    /// fail-closed: this builder never promotes a profile to
    /// [`CalibrationStatus::Calibrated`].
    pub fn with_airglow_calibration(mut self, airglow: AirglowSiteCalibration) -> Self {
        self.airglow = airglow;
        self
    }

    /// Evidence-backed calibration maturity for this scientific profile.
    pub const fn calibration_status(&self) -> CalibrationStatus {
        self.calibration_status
    }

    /// Human-readable profile name.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Representative-altitude policy used when resolving this profile.
    pub const fn representative_altitude(&self) -> RepresentativeAltitude {
        self.representative_altitude
    }

    /// Atmospheric-condition policy used when resolving this profile.
    pub fn atmosphere(&self) -> &AtmosphereSource {
        &self.atmosphere
    }

    /// Provenance and limitations of the atmospheric assumptions.
    pub fn atmosphere_provenance(&self) -> &str {
        &self.atmosphere_provenance
    }

    /// Airglow assumptions carried by this profile.
    pub fn airglow(&self) -> &AirglowSiteCalibration {
        &self.airglow
    }

    /// Return true only for a dedicated validated site calibration.
    pub const fn is_site_calibrated(&self) -> bool {
        matches!(self.calibration_status, CalibrationStatus::Calibrated)
    }

    /// Stable profile identifier.
    pub fn id(&self) -> &SiteProfileId {
        &self.id
    }

    /// Resolve this specification to the concrete profile used for a query observer.
    ///
    /// [`AtmosphereSource::FromObserverAltitude`] derives pressure from the
    /// supplied observer altitude. Fixed atmospheres keep their explicit
    /// pressure and aerosol assumptions while still evaluating geometry at the
    /// caller-provided observer location. Resolving a profile does not assert
    /// that the observer is physically located at any site named by the profile
    /// identifier.
    pub fn resolve(&self, observer: Geodetic<ECEF>) -> SiteProfile {
        let representative_altitude = match self.representative_altitude {
            RepresentativeAltitude::FromObserver => observer.height.to::<Kilometer>(),
            RepresentativeAltitude::Fixed(altitude) => altitude,
        };
        let atmosphere = match &self.atmosphere {
            AtmosphereSource::FromObserverAltitude => {
                AtmosphericConditions::generic_clear_sky(observer)
            }
            AtmosphereSource::Fixed(atmosphere) => *atmosphere,
        };
        SiteProfile {
            id: self.id.clone(),
            name: self.name.clone(),
            calibration_status: self.calibration_status,
            representative_altitude,
            atmosphere,
            atmosphere_provenance: self.atmosphere_provenance.clone(),
            airglow: self.airglow.clone(),
        }
    }
}

/// Complete atmospheric and airglow assumptions resolved for one observer.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub struct SiteProfile {
    /// Stable profile identifier.
    pub id: SiteProfileId,
    /// Human-readable maturity-bearing profile name.
    pub name: Cow<'static, str>,
    /// Site calibration maturity.
    pub calibration_status: CalibrationStatus,
    /// Altitude represented by the atmospheric assumptions.
    pub representative_altitude: Kilometers,
    /// Rayleigh/Mie atmospheric conditions.
    pub atmosphere: AtmosphericConditions,
    /// Source and limitations of atmospheric assumptions.
    pub atmosphere_provenance: Cow<'static, str>,
    /// Airglow calibration assumptions.
    pub airglow: AirglowSiteCalibration,
}

impl SiteProfile {
    /// Return true only for a dedicated validated site calibration.
    pub fn is_site_calibrated(&self) -> bool {
        self.calibration_status == CalibrationStatus::Calibrated
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use siderust::qtty::{Degrees, Hectopascals, Meters};

    fn observer(height_m: f64) -> Geodetic<ECEF> {
        Geodetic::new_raw(
            Degrees::new(-17.89),
            Degrees::new(28.76),
            Meters::new(height_m),
        )
    }

    #[test]
    fn generic_profile_derives_pressure_from_observer_altitude_without_calibrating() {
        let low = SiteProfileSpec::generic_clear_sky().resolve(observer(0.0));
        let high = SiteProfileSpec::generic_clear_sky().resolve(observer(2_500.0));

        assert_eq!(low.calibration_status, CalibrationStatus::GenericFallback);
        assert_eq!(
            SiteProfileSpec::generic_clear_sky().calibration_status(),
            CalibrationStatus::GenericFallback
        );
        assert!(!SiteProfileSpec::generic_clear_sky().is_site_calibrated());
        assert!(low.atmosphere.surface_pressure > high.atmosphere.surface_pressure);
        assert_ne!(low.representative_altitude, high.representative_altitude);
        assert_eq!(low.calibration_status, high.calibration_status);
        assert_eq!(low.id.as_str(), "generic-clear-sky");
    }

    #[test]
    fn caller_defined_planning_profile_does_not_require_core_catalog() {
        let location = observer(2_200.0);
        let spec = SiteProfileSpec::planning(
            "magic-la-palma-planning",
            "magic-la-palma-planning",
            Kilometers::new(2.2),
            AtmosphericConditions::clear_sky_with_pressure(Hectopascals::new(770.0)),
            "Caller-defined MAGIC/La Palma planning atmosphere; not a core catalog entry.",
        );
        let resolved = spec.resolve(location);

        assert_eq!(resolved.id.as_str(), "magic-la-palma-planning");
        assert_eq!(
            resolved.calibration_status,
            CalibrationStatus::PlanningPreset
        );
        assert!(!resolved.is_site_calibrated());
        assert_eq!(resolved.atmosphere.surface_pressure.value(), 770.0);
        assert!(resolved.atmosphere_provenance.contains("Caller-defined"));
        assert_ne!(
            resolved.atmosphere.surface_pressure,
            SiteProfileSpec::generic_clear_sky()
                .resolve(location)
                .atmosphere
                .surface_pressure
        );
    }

    #[test]
    fn custom_planning_profile_propagates_airglow_without_promoting_maturity() {
        let mut airglow = AirglowSiteCalibration::skycalc_neutral();
        airglow.assumptions = Cow::Borrowed("Custom airglow assumptions for regression.");
        let spec = SiteProfileSpec::planning(
            "custom-planning-v1",
            "custom-planning-v1",
            Kilometers::new(1.8),
            AtmosphericConditions::paranal_average(),
            "Custom planning atmosphere provenance.",
        )
        .with_airglow_calibration(airglow);
        let resolved = spec.resolve(observer(1_800.0));

        assert!(!spec.is_site_calibrated());
        assert!(!resolved.is_site_calibrated());
        assert_eq!(
            resolved.calibration_status,
            CalibrationStatus::PlanningPreset
        );
        assert_eq!(
            resolved.atmosphere_provenance.as_ref(),
            "Custom planning atmosphere provenance."
        );
        assert!(resolved
            .airglow
            .assumptions
            .contains("Custom airglow assumptions"));
    }
}
