use crate::error::Result;
use crate::evaluator::Target;
use crate::spectra::solar;
use crate::units::SolarSpectralIrradianceUnit;
use optica::spectrum::SampledSpectrum;
use qtty::length::Nanometer;
use siderust::coordinates::centers::Geodetic;
use siderust::coordinates::frames::ECEF;
use tempoch::{Time, UTC};

use super::extinction::ZodiacalExtinction;
use super::geometry;
use super::output::ZodiacalOutputs;
use super::spectrum as zl_spectrum;

/// Supported Zodiacal-light scientific source models.
///
/// This selects the celestial source parameterization independently from
/// atmospheric propagation. Additional independently validated models may be
/// added in future releases; downstream matches should include a wildcard arm.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum ZodiacalModel {
    /// Leinert et al. (1998) empirical directional brightness model.
    Leinert1998,
}

impl ZodiacalModel {
    /// Stable machine-readable scientific model identity.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Leinert1998 => "leinert-1998",
        }
    }
}

#[derive(Debug, Clone)]
pub(super) enum ZodiacalBrightnessModel {
    Leinert1998,
}

#[derive(Debug, Clone)]
pub(crate) struct ZodiacalLight {
    brightness_model: ZodiacalBrightnessModel,
    solar_spectrum: SampledSpectrum<Nanometer, SolarSpectralIrradianceUnit>,
    extinction: ZodiacalExtinction,
}

impl ZodiacalLight {
    pub(crate) fn leinert1998() -> Result<Self> {
        Ok(Self {
            brightness_model: ZodiacalBrightnessModel::Leinert1998,
            solar_spectrum: solar::load()?,
            extinction: ZodiacalExtinction::Noll2012Approx,
        })
    }

    pub(crate) fn with_extinction(mut self, extinction: ZodiacalExtinction) -> Self {
        self.extinction = extinction;
        self
    }

    pub(crate) fn compute(
        &self,
        time: Time<UTC>,
        location: Geodetic<ECEF>,
        target: Target,
    ) -> Result<ZodiacalOutputs> {
        self.compute_observed(time, location, target)
    }

    pub(crate) fn compute_observed(
        &self,
        time: Time<UTC>,
        location: Geodetic<ECEF>,
        target: Target,
    ) -> Result<ZodiacalOutputs> {
        let geom = geometry::compute_observed(time, location, target)?;
        if is_below_horizon(&geom) {
            return Ok(zero_outputs());
        }
        self.evaluate_geometry(&geom, self.extinction)
    }

    fn evaluate_geometry(
        &self,
        geom: &geometry::ZodiacalGeometry,
        extinction: ZodiacalExtinction,
    ) -> Result<ZodiacalOutputs> {
        match &self.brightness_model {
            ZodiacalBrightnessModel::Leinert1998 => {
                zl_spectrum::compute_outputs(geom, &self.solar_spectrum, extinction)
            }
        }
    }
}

fn is_below_horizon(geom: &geometry::ZodiacalGeometry) -> bool {
    geom.zenith
        .map(|zenith| (qtty::angular::Degrees::new(90.0) - zenith).value() <= 0.0)
        .unwrap_or(false)
}

fn zero_outputs() -> ZodiacalOutputs {
    use qtty::radiometry::{
        PhotonsPerSquareCentimeterNanosecondSteradian as BandPhotonRadiance, S10s as S10,
    };
    ZodiacalOutputs {
        integrated: BandPhotonRadiance::new(0.0),
        b_flux_s10: S10::new(0.0),
        v_flux_s10: S10::new(0.0),
    }
}

#[cfg(test)]
pub(super) mod test_support {
    use super::*;

    pub(in crate::components::zodiacal) fn compute_exoatmospheric(
        model: &ZodiacalLight,
        time: Time<UTC>,
        target: Target,
    ) -> Result<ZodiacalOutputs> {
        let geom = geometry::test_support::compute_exoatmospheric(time, target)?;
        model.evaluate_geometry(&geom, ZodiacalExtinction::None)
    }
}
