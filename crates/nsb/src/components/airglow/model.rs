use super::calibration::{load_builtin_standard, AirglowContinuum};
use super::continuum::evaluate_continuum;
use super::geometry::target_altitude;
use super::output::AirglowOutputs;
use super::units::{SolarFluxUnits, DEFAULT_SOLAR_RADIO_FLUX};
use crate::error::Result;
use crate::site::SiteProfileId;
use siderust::coordinates::centers::Geodetic;
use siderust::coordinates::frames::{EquatorialMeanJ2000, ECEF};
use siderust::coordinates::spherical::Direction as SphericalDirection;
use tempoch::{Time, UTC};

#[derive(Debug, Clone)]
pub struct Airglow {
    location: Geodetic<ECEF>,
    continuum: AirglowContinuum,
    solar_radio_flux: SolarFluxUnits,
    scale: f64,
}

impl Airglow {
    pub fn standard_clear_sky(location: Geodetic<ECEF>) -> Result<Self> {
        Ok(Self::with_continuum(location, load_builtin_standard()?))
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
            .with_scale(profile.airglow.scale))
    }

    pub fn with_continuum(location: Geodetic<ECEF>, continuum: AirglowContinuum) -> Self {
        Self {
            location,
            continuum,
            solar_radio_flux: DEFAULT_SOLAR_RADIO_FLUX,
            scale: 1.0,
        }
    }

    pub fn with_solar_radio_flux(mut self, flux: SolarFluxUnits) -> Self {
        self.solar_radio_flux = flux;
        self
    }

    pub fn with_f10_7(self, flux: SolarFluxUnits) -> Self {
        self.with_solar_radio_flux(flux)
    }

    pub fn with_scale(mut self, scale: f64) -> Self {
        self.scale = scale;
        self
    }

    pub fn compute(
        &self,
        time: Time<UTC>,
        target: SphericalDirection<EquatorialMeanJ2000>,
    ) -> Result<AirglowOutputs> {
        let altitude = target_altitude(time, self.location, target);
        Ok(evaluate_continuum(
            &self.continuum,
            time,
            self.location,
            altitude,
            self.solar_radio_flux,
            self.scale,
        ))
    }
}
