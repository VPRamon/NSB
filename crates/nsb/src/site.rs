use crate::components::moonlight::AtmosphericConditions;
use siderust::coordinates::centers::Geodetic;
use siderust::coordinates::frames::ECEF;
use siderust::qtty::{Kilometer, Kilometers};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SiteProfileId {
    GenericClearSky,
    CtaNorth,
    CtaSouth,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CalibrationStatus {
    GenericFallback,
    PlanningPreset,
    Calibrated,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AirglowSiteCalibration {
    pub scale: f64,
    pub template: &'static str,
    pub provenance: &'static str,
    pub assumptions: &'static str,
}

impl AirglowSiteCalibration {
    fn skycalc_neutral() -> Self {
        Self {
            scale: 1.0,
            template: "NSB/data/airglow_cont.dat",
            provenance: "Bundled SkyCalc-derived empirical continuum template; neutral site scale.",
            assumptions: "No CTAO-specific airglow continuum scale is bundled yet.",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SiteProfile {
    pub id: SiteProfileId,
    pub name: &'static str,
    pub calibration_status: CalibrationStatus,
    pub representative_altitude: Kilometers,
    pub atmosphere: AtmosphericConditions,
    pub atmosphere_provenance: &'static str,
    pub airglow: AirglowSiteCalibration,
}

impl SiteProfile {
    pub fn is_site_calibrated(&self) -> bool {
        self.calibration_status == CalibrationStatus::Calibrated
    }
}

impl SiteProfileId {
    pub fn profile(self, observer: Geodetic<ECEF>) -> SiteProfile {
        match self {
            Self::GenericClearSky => SiteProfile {
                id: self,
                name: "generic-clear-sky",
                calibration_status: CalibrationStatus::GenericFallback,
                representative_altitude: observer.height.to::<Kilometer>(),
                atmosphere: AtmosphericConditions::generic_clear_sky(observer),
                atmosphere_provenance: "Generic clear-sky atmosphere derived from observer altitude.",
                airglow: AirglowSiteCalibration::skycalc_neutral(),
            },
            Self::CtaNorth => SiteProfile {
                id: self,
                name: "ctao-north-planning",
                calibration_status: CalibrationStatus::PlanningPreset,
                representative_altitude: Kilometers::new(2.2),
                atmosphere: AtmosphericConditions::cta_n_clear_sky(),
                atmosphere_provenance: "CTAO-North planning atmosphere; not yet a validated CTA-N aerosol calibration.",
                airglow: AirglowSiteCalibration::skycalc_neutral(),
            },
            Self::CtaSouth => SiteProfile {
                id: self,
                name: "ctao-south-planning",
                calibration_status: CalibrationStatus::PlanningPreset,
                representative_altitude: Kilometers::new(2.1),
                atmosphere: AtmosphericConditions::cta_s_clear_sky(),
                atmosphere_provenance: "CTAO-South planning atmosphere; not yet a dedicated CTA-S aerosol calibration.",
                airglow: AirglowSiteCalibration::skycalc_neutral(),
            },
        }
    }

    pub fn all() -> [Self; 3] {
        [Self::GenericClearSky, Self::CtaNorth, Self::CtaSouth]
    }
}
