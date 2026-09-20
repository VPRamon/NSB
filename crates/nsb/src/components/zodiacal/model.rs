use crate::error::Result;
#[cfg(test)]
use crate::error::NsbError;
use crate::evaluator::Target;
use crate::reference::solar;
use crate::units::SolarSpectralIrradianceUnit;
use optica::spectrum::SampledSpectrum;
#[cfg(test)]
use qtty::angular::Degrees;
use qtty::length::Nanometer;
#[cfg(test)]
use qtty::radiometry::S10s as S10;
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

#[cfg(test)]
#[derive(Debug, Clone)]
pub(super) struct ZodiacalBrightnessGrid {
    beta_axis: Vec<Degrees>,
    delta_lambda_axis: Vec<Degrees>,
    s10_values: Vec<Vec<S10>>,
}

#[cfg(test)]
impl ZodiacalBrightnessGrid {
    pub(super) fn new(
        beta_axis: Vec<Degrees>,
        delta_lambda_axis: Vec<Degrees>,
        s10_values: Vec<Vec<S10>>,
    ) -> Result<Self> {
        if beta_axis.len() < 2 || delta_lambda_axis.len() < 2 {
            return Err(NsbError::OutOfRange(
                "custom ZodiacalBrightnessGrid axes must have at least 2 points each".to_string(),
            ));
        }
        if !is_strictly_increasing(&beta_axis) {
            return Err(NsbError::OutOfRange(
                "beta_axis must be strictly increasing".to_string(),
            ));
        }
        if !is_strictly_increasing(&delta_lambda_axis) {
            return Err(NsbError::OutOfRange(
                "delta_lambda_axis must be strictly increasing".to_string(),
            ));
        }
        if *beta_axis.first().unwrap() < Degrees::new(0.0)
            || *beta_axis.last().unwrap() > Degrees::new(90.0)
        {
            return Err(NsbError::OutOfRange(
                "beta_axis values must be in [0, 90] degrees".to_string(),
            ));
        }
        if *delta_lambda_axis.first().unwrap() < Degrees::new(0.0)
            || *delta_lambda_axis.last().unwrap() > Degrees::new(180.0)
        {
            return Err(NsbError::OutOfRange(
                "delta_lambda_axis values must be in [0, 180] degrees".to_string(),
            ));
        }
        if s10_values.len() != beta_axis.len() {
            return Err(NsbError::OutOfRange(format!(
                "s10_values row count {} != beta_axis length {}",
                s10_values.len(),
                beta_axis.len()
            )));
        }
        for (i, row) in s10_values.iter().enumerate() {
            if row.len() != delta_lambda_axis.len() {
                return Err(NsbError::OutOfRange(format!(
                    "s10_values row {} has length {} != delta_lambda_axis length {}",
                    i,
                    row.len(),
                    delta_lambda_axis.len()
                )));
            }
            for &value in row {
                if !value.is_finite() || value < S10::new(0.0) {
                    return Err(NsbError::OutOfRange(format!(
                        "s10_values[{i}] contains non-finite or negative value: {}",
                        value.value()
                    )));
                }
            }
        }
        Ok(Self {
            beta_axis,
            delta_lambda_axis,
            s10_values,
        })
    }

    fn lookup_s10(&self, beta: Degrees, delta_lambda: Degrees) -> Result<S10> {
        let beta = beta.abs().min(Degrees::new(90.0));
        let delta_lambda = delta_lambda.abs().min(Degrees::new(180.0));
        let (ib0, ib1, tb) = bracket(&self.beta_axis, beta);
        let (il0, il1, tl) = bracket(&self.delta_lambda_axis, delta_lambda);
        Ok(bilinear(
            self.s10_values[ib0][il0],
            self.s10_values[ib0][il1],
            self.s10_values[ib1][il0],
            self.s10_values[ib1][il1],
            tb,
            tl,
        ))
    }
}

#[cfg(test)]
fn is_strictly_increasing(values: &[Degrees]) -> bool {
    values.windows(2).all(|window| window[1] > window[0])
}

#[cfg(test)]
fn bracket(axis: &[Degrees], value: Degrees) -> (usize, usize, f64) {
    let pos = axis.partition_point(|&x| x <= value);
    let i1 = pos.min(axis.len() - 1);
    let i0 = if i1 == 0 { 0 } else { i1 - 1 };
    let t = if axis[i1] > axis[i0] {
        (value - axis[i0]).value() / (axis[i1] - axis[i0]).value()
    } else {
        0.0
    };
    (i0, i1, t.clamp(0.0, 1.0))
}

#[cfg(test)]
fn bilinear(v00: S10, v01: S10, v10: S10, v11: S10, tx: f64, ty: f64) -> S10 {
    let r0 = v00 + (v10 - v00) * tx;
    let r1 = v01 + (v11 - v01) * tx;
    r0 + (r1 - r0) * ty
}

#[derive(Debug, Clone)]
pub(super) enum ZodiacalBrightnessModel {
    Leinert1998,
    #[cfg(test)]
    CustomGrid(ZodiacalBrightnessGrid),
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

    #[cfg(test)]
    pub(super) fn with_brightness_model(model: ZodiacalBrightnessModel) -> Result<Self> {
        Ok(Self {
            brightness_model: model,
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

    #[cfg(test)]
    pub(super) fn compute_exoatmospheric(
        &self,
        time: Time<UTC>,
        target: Target,
    ) -> Result<ZodiacalOutputs> {
        let geom = geometry::compute_exoatmospheric(time, target)?;
        self.evaluate_geometry(&geom, ZodiacalExtinction::None)
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
            #[cfg(test)]
            ZodiacalBrightnessModel::CustomGrid(grid) => {
                let s10_500 = grid.lookup_s10(
                    geom.beta.abs().to::<qtty::angular::Degree>(),
                    geom.delta_lambda.to::<qtty::angular::Degree>(),
                )?;
                zl_spectrum::compute_outputs_with_s10(
                    geom,
                    &self.solar_spectrum,
                    extinction,
                    s10_500,
                )
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
