use super::calibration::{load_builtin_standard, AirglowContinuum};
use super::continuum::{
    evaluate_continuum, evaluate_continuum_with_time_bin, AirglowEvaluationContext,
};
use super::geometry::{target_altitude, AirglowGeometryModel, VanRhijnConfig};
use super::output::AirglowOutputs;
use super::units::{SolarFluxUnits, DEFAULT_SOLAR_RADIO_FLUX};
use crate::components::moonlight::AtmosphericConditions;
use crate::error::Result;
use crate::site::SiteProfileId;
use crate::units::ScaleFactors;
use siderust::coordinates::centers::Geodetic;
use siderust::coordinates::frames::{EquatorialMeanJ2000, ECEF};
use siderust::coordinates::spherical::Direction as SphericalDirection;
use std::sync::Arc;
use tempoch::{Time, UTC};

#[derive(Debug, Clone)]
/// Empirical airglow continuum evaluator for an arbitrary Earth location.
pub struct Airglow {
    location: Geodetic<ECEF>,
    continuum: Arc<AirglowContinuum>,
    atmosphere: AtmosphericConditions,
    geometry: AirglowGeometryModel,
    solar_radio_flux: SolarFluxUnits,
    scale: ScaleFactors,
}

impl Airglow {
    /// Build the generic clear-sky airglow model.
    ///
    /// Uses altitude-derived generic clear-sky [`AtmosphericConditions`] for the
    /// Noll effective Rayleigh/Mie scattering stage.
    pub fn standard_clear_sky(location: Geodetic<ECEF>) -> Result<Self> {
        Ok(Self::with_continuum(location, load_builtin_standard()?)
            .with_atmosphere(AtmosphericConditions::generic_clear_sky(location)))
    }

    /// Build an airglow model from a named NSB site profile.
    ///
    /// CTAO profiles currently use the bundled SkyCalc-derived continuum with a
    /// neutral site scale and explicit uncalibrated provenance. This constructor
    /// is still preferred over `standard_clear_sky` for CTAO call sites because
    /// the selected assumptions are machine-readable instead of implicit.
    pub fn for_site_profile(location: Geodetic<ECEF>, site_profile: SiteProfileId) -> Result<Self> {
        let profile = site_profile.profile(location);
        Ok(Self::with_continuum(location, load_builtin_standard()?)
            .with_atmosphere(profile.atmosphere)
            .with_scale(profile.airglow.scale))
    }

    /// Build an airglow model with caller-provided continuum calibration.
    ///
    /// Atmospheric scattering defaults to generic clear-sky conditions derived
    /// from `location`. Override with [`Self::with_atmosphere`] when needed.
    pub fn with_continuum(location: Geodetic<ECEF>, continuum: AirglowContinuum) -> Self {
        Self::with_shared_continuum(location, Arc::new(continuum))
    }

    pub(crate) fn with_shared_continuum(
        location: Geodetic<ECEF>,
        continuum: Arc<AirglowContinuum>,
    ) -> Self {
        let geometry = AirglowGeometryModel::VanRhijn(VanRhijnConfig::from_continuum_height(
            continuum.emission_height_km,
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

    /// Select atmospheric pressure/Rayleigh/Mie assumptions for Noll scattering.
    pub fn with_atmosphere(mut self, atmosphere: AtmosphericConditions) -> Self {
        self.atmosphere = atmosphere;
        self
    }

    /// Select the emitting-volume line-of-sight geometry model.
    ///
    /// This does not change atmospheric extinction/scattering. Van Rhijn is the
    /// default; vertical profiles must be selected explicitly.
    pub fn with_geometry(mut self, geometry: AirglowGeometryModel) -> Self {
        self.geometry = geometry;
        self
    }

    /// Return the selected emitting-volume geometry model.
    pub fn geometry(&self) -> &AirglowGeometryModel {
        &self.geometry
    }

    /// Set the F10.7 solar-radio-flux input.
    pub fn with_solar_radio_flux(mut self, flux: SolarFluxUnits) -> Self {
        self.solar_radio_flux = flux;
        self
    }

    /// Alias for [`Self::with_solar_radio_flux`].
    pub fn with_f10_7(self, flux: SolarFluxUnits) -> Self {
        self.with_solar_radio_flux(flux)
    }

    /// Apply an explicit multiplicative continuum scale.
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

    pub(crate) fn compute_with_time_of_night_bin(
        &self,
        time: Time<UTC>,
        target: SphericalDirection<EquatorialMeanJ2000>,
        time_bin: usize,
    ) -> Result<AirglowOutputs> {
        let altitude = target_altitude(time, self.location, target);
        evaluate_continuum_with_time_bin(
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
            time_bin,
        )
    }
}
