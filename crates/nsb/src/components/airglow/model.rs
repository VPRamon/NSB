#[cfg(test)]
use super::calibration::load_builtin_standard;
use super::calibration::AirglowContinuum;
use super::continuum::{
    evaluate_continuum, evaluate_continuum_with_night_phase, AirglowEvaluationContext,
};
use super::domain::AirglowNightPhase;
use super::geometry::{target_altitude, AirglowGeometryModel, VanRhijnConfig};
use super::output::AirglowOutputs;
use super::units::{SolarFluxUnits, DEFAULT_SOLAR_RADIO_FLUX};
use crate::error::Result;
use crate::site::{AtmosphericConditions, CalibrationStatus, SiteProfileId};
use crate::units::ScaleFactors;
use siderust::coordinates::centers::Geodetic;
use siderust::coordinates::frames::{EquatorialMeanJ2000, ECEF};
use siderust::coordinates::spherical::Direction as SphericalDirection;
use std::sync::Arc;
use tempoch::{Time, UTC};

/// Scientific profile selected by an Airglow configuration.
///
/// Location and operational settings are deliberately absent from this enum:
/// coordinates, atmosphere, emitting-volume geometry, F10.7 and user scaling
/// may change a numerical result, but they cannot upgrade its scientific
/// calibration maturity.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum AirglowScientificProfile {
    /// One of NSB's explicit built-in scientific assumption profiles.
    BuiltIn(SiteProfileId),
}

impl AirglowScientificProfile {
    /// Stable machine-readable profile identifier.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::BuiltIn(profile) => profile.as_str(),
        }
    }

    /// Evidence-backed calibration maturity for this profile.
    pub const fn calibration_status(self) -> CalibrationStatus {
        match self {
            Self::BuiltIn(profile) => profile.calibration_status(),
        }
    }

    /// Return the built-in site profile when one was explicitly selected.
    pub const fn site_profile(self) -> Option<SiteProfileId> {
        match self {
            Self::BuiltIn(profile) => Some(profile),
        }
    }

    /// Return true only for a dedicated scientifically calibrated profile.
    pub const fn is_site_calibrated(self) -> bool {
        matches!(self.calibration_status(), CalibrationStatus::Calibrated)
    }
}

#[derive(Debug, Clone)]
/// Empirical airglow continuum evaluator for an arbitrary Earth location.
///
/// Geographic support is independent of scientific calibration maturity. The
/// bundled continuum is Paranal-derived and is used as a generic/planning proxy
/// unless a future, explicit validated calibration path selects otherwise.
pub(crate) struct Airglow {
    location: Geodetic<ECEF>,
    continuum: Arc<AirglowContinuum>,
    atmosphere: AtmosphericConditions,
    geometry: AirglowGeometryModel,
    solar_radio_flux: SolarFluxUnits,
    scale: ScaleFactors,
}

impl Airglow {
    /// Build the generic clear-sky Airglow planning proxy for component tests.
    ///
    /// The supplied location controls geometry and altitude-derived generic
    /// atmospheric conditions. It does not make the bundled Paranal-derived
    /// continuum calibrated for that location, including when `location` is
    /// Paranal itself.
    #[cfg(test)]
    pub(crate) fn standard_clear_sky(location: Geodetic<ECEF>) -> Result<Self> {
        let continuum = Arc::new(load_builtin_standard()?);
        Ok(Self::with_shared_continuum(location, continuum)
            .with_atmosphere(AtmosphericConditions::generic_clear_sky(location)))
    }

    /// Build an Airglow model from an explicitly selected NSB site profile.
    ///
    /// CTAO profiles currently use the bundled Paranal-derived continuum with a
    /// neutral site scale and [`CalibrationStatus::PlanningPreset`] maturity.
    /// Selecting a profile is distinct from selecting an observatory/location.
    #[cfg(test)]
    pub(crate) fn for_site_profile(
        location: Geodetic<ECEF>,
        site_profile: SiteProfileId,
    ) -> Result<Self> {
        let profile = site_profile.profile(location);
        let continuum = Arc::new(load_builtin_standard()?);
        Ok(Self::with_shared_continuum(location, continuum)
            .with_atmosphere(profile.atmosphere)
            .with_scale(profile.airglow.scale))
    }

    pub(crate) fn with_shared_continuum(
        location: Geodetic<ECEF>,
        continuum: Arc<AirglowContinuum>,
    ) -> Self {
        let geometry = AirglowGeometryModel::VanRhijn(VanRhijnConfig::from_continuum_height(
            continuum.emission_height_km(),
        ));
        Self {
            location,
            continuum,
            atmosphere: AtmosphericConditions::generic_clear_sky(location),
            geometry,
            solar_radio_flux: DEFAULT_SOLAR_RADIO_FLUX,
            scale: ScaleFactors::new(1.0),
        }
    }

    #[cfg(test)]
    pub(crate) fn with_continuum(location: Geodetic<ECEF>, continuum: AirglowContinuum) -> Self {
        Self::with_shared_continuum(location, Arc::new(continuum))
    }

    /// Select atmospheric pressure/Rayleigh/Mie assumptions for Noll scattering.
    ///
    /// Atmospheric choices affect propagation only and preserve the scientific
    /// profile and calibration maturity.
    pub fn with_atmosphere(mut self, atmosphere: AtmosphericConditions) -> Self {
        self.atmosphere = atmosphere;
        self
    }

    /// Select the emitting-volume line-of-sight geometry model.
    ///
    /// This does not change atmospheric extinction/scattering or calibration
    /// maturity. Van Rhijn is the default; vertical profiles must be selected
    /// explicitly.
    pub fn with_geometry(mut self, geometry: AirglowGeometryModel) -> Self {
        self.geometry = geometry;
        self
    }

    /// Return the selected emitting-volume geometry model.
    #[cfg(test)]
    pub(crate) fn geometry(&self) -> &AirglowGeometryModel {
        &self.geometry
    }

    /// Set the F10.7 solar-radio-flux input.
    ///
    /// Solar activity changes the empirical correction only; it cannot confer
    /// site calibration.
    pub fn with_solar_radio_flux(mut self, flux: SolarFluxUnits) -> Self {
        self.solar_radio_flux = flux;
        self
    }

    /// Apply an explicit multiplicative continuum scale.
    ///
    /// A caller-provided scale is operational input, not calibration evidence,
    /// and therefore preserves the model's scientific maturity.
    pub fn with_scale(mut self, scale: ScaleFactors) -> Self {
        self.scale = scale;
        self
    }

    /// Evaluate airglow toward a target at one UTC instant.
    pub fn compute(
        &self,
        time: Time<UTC>,
        target: SphericalDirection<EquatorialMeanJ2000>,
    ) -> Result<AirglowOutputs> {
        let altitude = target_altitude(time, self.location, target);
        evaluate_continuum(
            &self.continuum,
            time,
            altitude,
            AirglowEvaluationContext {
                location: self.location,
                atmosphere: self.atmosphere,
                geometry: self.geometry.clone(),
                solar_radio_flux: self.solar_radio_flux,
                user_scale: self.scale,
            },
        )
    }

    pub(crate) fn compute_with_night_phase(
        &self,
        time: Time<UTC>,
        target: SphericalDirection<EquatorialMeanJ2000>,
        phase: AirglowNightPhase,
    ) -> Result<AirglowOutputs> {
        let altitude = target_altitude(time, self.location, target);
        evaluate_continuum_with_night_phase(
            &self.continuum,
            time,
            altitude,
            AirglowEvaluationContext {
                location: self.location,
                atmosphere: self.atmosphere,
                geometry: self.geometry.clone(),
                solar_radio_flux: self.solar_radio_flux,
                user_scale: self.scale,
            },
            phase,
        )
    }
}
