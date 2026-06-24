//! Named atmospheric and airglow profile metadata.
//!
//! NSB deliberately separates generic fallbacks from named site profiles. The
//! CTAO entries below are first-class planning profiles with machine-readable
//! provenance and calibration maturity. They are not marked as fully
//! site-calibrated until dedicated CTAO atmospheric and airglow validation data
//! are bundled with the crate.

use crate::components::moonlight::AtmosphericConditions;
use siderust::coordinates::centers::Geodetic;
use siderust::coordinates::frames::ECEF;
use siderust::qtty::{Kilometer, Kilometers};

/// Identifier for a built-in NSB site profile.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SiteProfileId {
    /// Generic clear-sky fallback derived from the query observer altitude.
    GenericClearSky,
    /// CTAO-North planning profile for La Palma / ORM use.
    CtaNorth,
    /// CTAO-South planning profile for the Paranal/Atacama use case.
    CtaSouth,
}

/// Scientific maturity of a named site profile.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CalibrationStatus {
    /// Generic, location-agnostic fallback; not a named-site calibration.
    GenericFallback,
    /// Named-site planning preset with explicit assumptions and provenance.
    PlanningPreset,
    /// Dedicated site-calibrated profile validated against site reference data.
    Calibrated,
}

/// Airglow-side calibration assumptions associated with a site profile.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AirglowSiteCalibration {
    /// Multiplicative scale applied to the bundled continuum template.
    pub scale: f64,
    /// Continuum template used by the profile.
    pub template: &'static str,
    /// Machine-readable provenance note for the template and scale.
    pub provenance: &'static str,
    /// Human-readable calibration limitations.
    pub assumptions: &'static str,
}

impl AirglowSiteCalibration {
    fn skycalc_neutral() -> Self {
        Self {
            scale: 1.0,
            template: "NSB/data/airglow_cont.dat",
            provenance: "Bundled SkyCalc-derived empirical continuum template; neutral site scale.",
            assumptions: concat!(
                "No CTAO-specific airglow continuum scale is bundled yet; ",
                "the named profile records this explicitly instead of silently ",
                "claiming a calibrated site airglow model."
            ),
        }
    }
}

/// Complete atmospheric and airglow assumptions for a built-in site profile.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SiteProfile {
    /// Stable profile identifier.
    pub id: SiteProfileId,
    /// Human-readable maturity-bearing profile name.
    pub name: &'static str,
    /// Site calibration maturity.
    pub calibration_status: CalibrationStatus,
    /// Altitude represented by the atmospheric assumptions.
    pub representative_altitude: Kilometers,
    /// Rayleigh/Mie atmospheric conditions.
    pub atmosphere: AtmosphericConditions,
    /// Source and limitations of atmospheric assumptions.
    pub atmosphere_provenance: &'static str,
    /// Airglow calibration assumptions.
    pub airglow: AirglowSiteCalibration,
}

impl SiteProfile {
    /// Return true only for a dedicated validated site calibration.
    pub fn is_site_calibrated(&self) -> bool {
        self.calibration_status == CalibrationStatus::Calibrated
    }
}

impl SiteProfileId {
    /// Stable maturity-bearing profile identifier.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::GenericClearSky => "generic-clear-sky",
            Self::CtaNorth => "ctao-north-planning",
            Self::CtaSouth => "ctao-south-planning",
        }
    }

    /// Resolve this identifier to the concrete profile used for a query observer.
    ///
    /// [`SiteProfileId::GenericClearSky`] derives pressure from the supplied
    /// observer altitude. Named CTAO profiles use explicit pressure and aerosol
    /// assumptions while still evaluating geometry at the caller-provided
    /// observer location.
    pub fn profile(self, observer: Geodetic<ECEF>) -> SiteProfile {
        match self {
            Self::GenericClearSky => SiteProfile {
                id: self,
                name: "generic-clear-sky",
                calibration_status: CalibrationStatus::GenericFallback,
                representative_altitude: observer.height.to::<Kilometer>(),
                atmosphere: AtmosphericConditions::generic_clear_sky(observer),
                atmosphere_provenance: concat!(
                    "Pressure estimated from observer altitude with the NSB ",
                    "generic barometric fallback; Rayleigh scale height and Mie ",
                    "parameters use the bundled clear-sky defaults."
                ),
                airglow: AirglowSiteCalibration::skycalc_neutral(),
            },
            Self::CtaNorth => SiteProfile {
                id: self,
                name: "ctao-north-planning",
                calibration_status: CalibrationStatus::PlanningPreset,
                representative_altitude: Kilometers::new(2.2),
                atmosphere: AtmosphericConditions::cta_n_clear_sky(),
                atmosphere_provenance: concat!(
                    "CTAO-North planning preset: representative ORM/La Palma ",
                    "altitude, fixed planning pressure, Siderust default ",
                    "Rayleigh scale height, and bundled Paranal-like clear-sky ",
                    "Mie parameterization. This is not yet a validated CTA-N ",
                    "aerosol calibration."
                ),
                airglow: AirglowSiteCalibration::skycalc_neutral(),
            },
            Self::CtaSouth => SiteProfile {
                id: self,
                name: "ctao-south-planning",
                calibration_status: CalibrationStatus::PlanningPreset,
                representative_altitude: Kilometers::new(2.1),
                atmosphere: AtmosphericConditions::cta_s_clear_sky(),
                atmosphere_provenance: concat!(
                    "CTAO-South planning preset: Paranal-like atmosphere from ",
                    "Siderust AtmosphereProfile::EL_PARANAL, used explicitly for ",
                    "the Paranal/Atacama CTAO use case. This is not yet a ",
                    "dedicated CTA-S aerosol calibration."
                ),
                airglow: AirglowSiteCalibration::skycalc_neutral(),
            },
        }
    }

    /// Return every built-in profile identifier.
    pub fn all() -> [Self; 3] {
        [Self::GenericClearSky, Self::CtaNorth, Self::CtaSouth]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use siderust::coordinates::centers::Geodetic;
    use siderust::qtty::{Degrees, Meters};

    fn observer(height_m: f64) -> Geodetic<ECEF> {
        Geodetic::new_raw(
            Degrees::new(-17.89),
            Degrees::new(28.76),
            Meters::new(height_m),
        )
    }

    #[test]
    fn ctao_profiles_are_explicit_planning_presets() {
        let north = SiteProfileId::CtaNorth.profile(observer(2_200.0));
        let south = SiteProfileId::CtaSouth.profile(observer(2_100.0));

        assert_eq!(north.calibration_status, CalibrationStatus::PlanningPreset);
        assert_eq!(south.calibration_status, CalibrationStatus::PlanningPreset);
        assert!(!north.is_site_calibrated());
        assert!(!south.is_site_calibrated());
        assert_eq!(north.airglow.scale, 1.0);
        assert_eq!(south.airglow.scale, 1.0);
        assert!(north.atmosphere_provenance.contains("CTAO-North"));
        assert!(south.atmosphere_provenance.contains("CTAO-South"));
    }

    #[test]
    fn generic_profile_derives_pressure_from_observer_altitude() {
        let low = SiteProfileId::GenericClearSky.profile(observer(0.0));
        let high = SiteProfileId::GenericClearSky.profile(observer(2_500.0));

        assert_eq!(low.calibration_status, CalibrationStatus::GenericFallback);
        assert!(low.atmosphere.surface_pressure > high.atmosphere.surface_pressure);
        assert_ne!(low.representative_altitude, high.representative_altitude);
    }

    #[test]
    fn cta_n_profile_does_not_alias_generic_clear_sky_pressure() {
        let location = observer(2_200.0);
        let generic = SiteProfileId::GenericClearSky.profile(location);
        let cta_n = SiteProfileId::CtaNorth.profile(location);

        assert_ne!(
            generic.atmosphere.surface_pressure,
            cta_n.atmosphere.surface_pressure
        );
        assert_eq!(cta_n.atmosphere.surface_pressure.value(), 770.0);
    }
}
