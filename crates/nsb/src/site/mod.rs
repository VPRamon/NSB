//! Scientific site profiles, shared atmospheric assumptions, and calibration metadata.
//!
//! NSB deliberately separates observer location from scientific site profiles.
//! A location answers where the observer is; [`SiteProfile<P>`](SiteProfile)
//! answers which NSB assumptions are selected. The marker type `P` carries
//! compile-time identity, while strings are reporting metadata only.
//! Observatory- or project-named planning markers belong at an application or
//! data layer; the core crate only provides the generic mechanism.

/// Shared atmospheric assumptions used by site-aware NSB components.
pub(crate) mod atmosphere;
/// Versioned evidence contract for dedicated site-calibration assets.
pub(crate) mod calibration;

pub use atmosphere::AtmosphericConditions;

use crate::{NsbError, Result};
use crate::units::dimensionless::Ratios;
use siderust::coordinates::centers::Geodetic;
use siderust::coordinates::frames::ECEF;
use siderust::qtty::Kilometers;
use std::marker::PhantomData;

/// Compile-time identity for a scientific site profile.
///
/// Application crates may implement this trait for their own zero-sized marker
/// types. `NAME` is used only for diagnostics and serialized metadata; it never
/// selects scientific behavior or calibration maturity.
pub trait SiteProfileTag: 'static {
    /// Stable presentation/serialization name for this profile.
    const NAME: &'static str;
}

/// Type-level identity of NSB's observer-derived generic clear-sky fallback.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct GenericClearSky;

impl SiteProfileTag for GenericClearSky {
    const NAME: &'static str = "generic-clear-sky";
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

/// Airglow-side assumptions used by every currently supported site profile.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct AirglowSiteCalibration {
    pub(crate) scale: Ratios,
    pub(crate) provenance: &'static str,
    pub(crate) assumptions: &'static str,
}

impl AirglowSiteCalibration {
    fn skycalc_neutral() -> Self {
        Self {
            scale: Ratios::new(1.0),
            provenance: concat!(
                "Bundled Paranal-derived (Noll/SkyCalc/FORS1) empirical continuum ",
                "reused as an explicit generic/planning proxy; neutral site scale; ",
                "not site-calibrated."
            ),
            assumptions: concat!(
                "No site-specific airglow continuum scale is bundled by the core ",
                "library; named application-layer profiles should record this ",
                "explicitly instead of silently claiming a calibrated site airglow ",
                "model. Arbitrary-location and Paranal-location results remain ",
                "planning approximations unless an explicit validated scientific ",
                "profile is selected; provenance from Paranal is not calibration ",
                "evidence for the observer location."
            ),
        }
    }
}

/// Opaque scientific profile with compile-time identity `P`.
///
/// External marker types provide identity only. Public planning construction
/// always produces [`CalibrationStatus::PlanningPreset`]; it cannot claim a
/// calibrated profile or choose an unevaluated Airglow asset.
pub struct SiteProfile<P: SiteProfileTag> {
    config: SiteProfileConfig,
    marker: PhantomData<fn() -> P>,
}

impl<P: SiteProfileTag> std::fmt::Debug for SiteProfile<P> {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("SiteProfile")
            .field("name", &P::NAME)
            .field("calibration_status", &self.config.calibration_status)
            .finish_non_exhaustive()
    }
}

impl<P: SiteProfileTag> Clone for SiteProfile<P> {
    fn clone(&self) -> Self {
        Self {
            config: self.config.clone(),
            marker: PhantomData,
        }
    }
}

impl<P: SiteProfileTag> PartialEq for SiteProfile<P> {
    fn eq(&self, other: &Self) -> bool {
        self.config == other.config
    }
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct SiteProfileConfig {
    name: String,
    calibration_status: CalibrationStatus,
    _representative_altitude: Option<Kilometers>,
    atmosphere: Option<AtmosphericConditions>,
    atmosphere_provenance: String,
    airglow: AirglowSiteCalibration,
}

impl SiteProfile<GenericClearSky> {
    /// Observer-derived generic clear-sky fallback.
    ///
    /// Pressure is derived from the query observer altitude when resolved.
    pub fn generic_clear_sky() -> Self {
        Self {
            config: SiteProfileConfig::generic_clear_sky(),
            marker: PhantomData,
        }
    }
}

impl<P: SiteProfileTag> SiteProfile<P> {
    /// Explicit planning preset with fixed atmospheric assumptions.
    ///
    /// `P` supplies compile-time identity. Its [`SiteProfileTag::NAME`] is
    /// retained only as reporting metadata. Nonphysical inputs are rejected.
    pub fn planning(
        representative_altitude: Kilometers,
        atmosphere: AtmosphericConditions,
        atmosphere_provenance: impl Into<String>,
    ) -> Result<Self> {
        let atmosphere_provenance = atmosphere_provenance.into();
        validate_nonempty(&atmosphere_provenance, "site-profile atmosphere provenance")?;
        validate_finite(representative_altitude.value(), "representative altitude")?;
        atmosphere.validate()?;

        Ok(Self {
            config: SiteProfileConfig {
                name: P::NAME.to_owned(),
                calibration_status: CalibrationStatus::PlanningPreset,
                _representative_altitude: Some(representative_altitude),
                atmosphere: Some(atmosphere),
                atmosphere_provenance,
                airglow: AirglowSiteCalibration::skycalc_neutral(),
            },
            marker: PhantomData,
        })
    }

    /// Evidence-backed calibration maturity for this scientific profile.
    pub const fn calibration_status(&self) -> CalibrationStatus {
        self.config.calibration_status
    }

    /// Human-readable profile name.
    pub fn name(&self) -> &str {
        &self.config.name
    }

    /// Provenance and limitations of the atmospheric assumptions.
    pub fn atmosphere_provenance(&self) -> &str {
        &self.config.atmosphere_provenance
    }

    pub(crate) fn erase(self) -> SiteProfileConfig {
        self.config
    }
}

impl SiteProfileConfig {
    fn generic_clear_sky() -> Self {
        Self {
            name: GenericClearSky::NAME.to_owned(),
            calibration_status: CalibrationStatus::GenericFallback,
            _representative_altitude: None,
            atmosphere: None,
            atmosphere_provenance: concat!(
                "Pressure estimated from observer altitude with the NSB ",
                "generic barometric fallback; Rayleigh scale height and Mie ",
                "parameters use the bundled clear-sky defaults."
            )
            .to_owned(),
            airglow: AirglowSiteCalibration::skycalc_neutral(),
        }
    }

    pub(crate) fn name(&self) -> &str {
        &self.name
    }

    pub(crate) const fn calibration_status(&self) -> CalibrationStatus {
        self.calibration_status
    }

    pub(crate) const fn is_site_calibrated(&self) -> bool {
        matches!(self.calibration_status, CalibrationStatus::Calibrated)
    }

    pub(crate) fn resolve(&self, observer: Geodetic<ECEF>) -> ResolvedSiteProfile {
        let atmosphere = self
            .atmosphere
            .unwrap_or_else(|| AtmosphericConditions::generic_clear_sky(observer));
        ResolvedSiteProfile {
            name: self.name.clone(),
            calibration_status: self.calibration_status,
            atmosphere,
            atmosphere_provenance: self.atmosphere_provenance.clone(),
            airglow: self.airglow.clone(),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct ResolvedSiteProfile {
    pub(crate) name: String,
    pub(crate) calibration_status: CalibrationStatus,
    pub(crate) atmosphere: AtmosphericConditions,
    pub(crate) atmosphere_provenance: String,
    pub(crate) airglow: AirglowSiteCalibration,
}

fn validate_nonempty(value: &str, field: &str) -> Result<()> {
    if value.trim().is_empty() {
        return Err(NsbError::OutOfRange(format!("{field} must not be empty")));
    }
    Ok(())
}

fn validate_finite(value: f64, field: &str) -> Result<()> {
    if !value.is_finite() {
        return Err(NsbError::OutOfRange(format!("{field} must be finite")));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use siderust::qtty::{Degrees, Hectopascals, Meters};

    struct MagicLaPalma;
    impl SiteProfileTag for MagicLaPalma {
        const NAME: &'static str = "magic-la-palma-planning";
    }

    struct AliasClearSkyName;
    impl SiteProfileTag for AliasClearSkyName {
        const NAME: &'static str = "generic-clear-sky";
    }

    fn observer(height_m: f64) -> Geodetic<ECEF> {
        Geodetic::new_raw(
            Degrees::new(-17.89),
            Degrees::new(28.76),
            Meters::new(height_m),
        )
    }

    #[test]
    fn generic_profile_derives_pressure_from_observer_altitude_without_calibrating() {
        let profile = SiteProfile::<GenericClearSky>::generic_clear_sky();
        let low = profile.erase().resolve(observer(0.0));
        let high = SiteProfile::<GenericClearSky>::generic_clear_sky()
            .erase()
            .resolve(observer(2_500.0));

        assert_eq!(low.calibration_status, CalibrationStatus::GenericFallback);
        assert_eq!(
            SiteProfile::<GenericClearSky>::generic_clear_sky().calibration_status(),
            CalibrationStatus::GenericFallback
        );
        assert!(!SiteProfile::<GenericClearSky>::generic_clear_sky()
            .erase()
            .is_site_calibrated());
        assert!(low.atmosphere.surface_pressure > high.atmosphere.surface_pressure);
        assert_eq!(low.calibration_status, high.calibration_status);
        assert_eq!(
            SiteProfile::<GenericClearSky>::generic_clear_sky().name(),
            "generic-clear-sky"
        );
    }

    #[test]
    fn external_marker_defines_planning_profile_without_core_catalog() {
        let location = observer(2_200.0);
        let profile = SiteProfile::<MagicLaPalma>::planning(
            Kilometers::new(2.2),
            AtmosphericConditions::clear_sky_with_pressure(Hectopascals::new(770.0)).unwrap(),
            "Caller-defined MAGIC/La Palma planning atmosphere; not a core catalog entry.",
        )
        .unwrap();
        let resolved = profile.clone().erase().resolve(location);

        assert_eq!(profile.name(), "magic-la-palma-planning");
        assert_eq!(
            profile.calibration_status(),
            CalibrationStatus::PlanningPreset
        );
        assert_ne!(resolved.calibration_status, CalibrationStatus::Calibrated);
        assert_eq!(resolved.atmosphere.surface_pressure.value(), 770.0);
        assert!(resolved.atmosphere_provenance.contains("Caller-defined"));
        assert_ne!(
            resolved.atmosphere.surface_pressure,
            SiteProfile::<GenericClearSky>::generic_clear_sky()
                .erase()
                .resolve(location)
                .atmosphere
                .surface_pressure
        );
    }

    #[test]
    fn equal_textual_names_do_not_transfer_scientific_behavior() {
        let location = observer(2_200.0);
        let generic = SiteProfile::<GenericClearSky>::generic_clear_sky();
        let alias = SiteProfile::<AliasClearSkyName>::planning(
            Kilometers::new(2.2),
            AtmosphericConditions::clear_sky_with_pressure(Hectopascals::new(770.0)).unwrap(),
            "External marker that reuses the generic-clear-sky presentation name.",
        )
        .unwrap();

        assert_eq!(generic.name(), alias.name());
        assert_eq!(
            generic.calibration_status(),
            CalibrationStatus::GenericFallback
        );
        assert_eq!(
            alias.calibration_status(),
            CalibrationStatus::PlanningPreset
        );
        assert_ne!(
            generic
                .erase()
                .resolve(location)
                .atmosphere
                .surface_pressure,
            alias.erase().resolve(location).atmosphere.surface_pressure
        );
    }

    #[test]
    fn public_construction_cannot_claim_calibrated_maturity() {
        let profile = SiteProfile::<MagicLaPalma>::planning(
            Kilometers::new(2.2),
            AtmosphericConditions::paranal_average(),
            "planning provenance",
        )
        .unwrap();
        assert_eq!(
            profile.calibration_status(),
            CalibrationStatus::PlanningPreset
        );
        assert_ne!(profile.calibration_status(), CalibrationStatus::Calibrated);
    }

    #[test]
    fn planning_profile_rejects_invalid_scientific_inputs() {
        let mut atmosphere = AtmosphericConditions::paranal_average();
        for pressure in [0.0, -1.0, f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
            atmosphere.surface_pressure = Hectopascals::new(pressure);
            assert!(SiteProfile::<MagicLaPalma>::planning(
                Kilometers::new(1.8),
                atmosphere,
                "custom planning atmosphere provenance",
            )
            .is_err());
        }
        assert!(SiteProfile::<MagicLaPalma>::planning(
            Kilometers::new(f64::NAN),
            AtmosphericConditions::paranal_average(),
            "custom planning atmosphere provenance",
        )
        .is_err());
        assert!(SiteProfile::<MagicLaPalma>::planning(
            Kilometers::new(1.8),
            AtmosphericConditions::paranal_average(),
            "   ",
        )
        .is_err());
    }
}
