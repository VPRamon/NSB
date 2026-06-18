use crate::error::{NsbError, Result};
use crate::evaluator::Target;
use crate::reference::solar;
use optica::spectrum::SampledSpectrum;
use qtty::radiometry::S10s as S10;
use siderust::coordinates::centers::Geodetic;
use siderust::coordinates::frames::ECEF;
use siderust::qtty::{length::Meter, Nanometer};
use tempoch::{Time, UTC};

use super::extinction::ZodiacalExtinction;
use super::geometry;
use super::output::{ZodiacalOutputs, ZodiacalSpectrum};
use super::spectrum as zl_spectrum;

#[derive(Debug, Clone)]
pub struct ZodiacalBrightnessGrid {
    pub(super) beta_axis: Vec<f64>,
    pub(super) delta_lambda_axis: Vec<f64>,
    pub(super) s10_values: Vec<Vec<f64>>,
    pub provenance: Option<String>,
}

impl ZodiacalBrightnessGrid {
    pub fn new(
        beta_axis: Vec<f64>,
        delta_lambda_axis: Vec<f64>,
        s10_values: Vec<Vec<f64>>,
        provenance: Option<String>,
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
        if *beta_axis.first().unwrap() < 0.0 || *beta_axis.last().unwrap() > 90.0 {
            return Err(NsbError::OutOfRange(
                "beta_axis values must be in [0, 90] degrees".to_string(),
            ));
        }
        if *delta_lambda_axis.first().unwrap() < 0.0 || *delta_lambda_axis.last().unwrap() > 180.0 {
            return Err(NsbError::OutOfRange(
                "delta_lambda_axis values must be in [0, 180] degrees".to_string(),
            ));
        }
        if s10_values.len() != beta_axis.len() {
            return Err(NsbError::OutOfRange(format!(
                "s10_values row count {} != beta_axis length {}",
                s10_values.len(), beta_axis.len()
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
            for &v in row {
                if !v.is_finite() || v < 0.0 {
                    return Err(NsbError::OutOfRange(format!(
                        "s10_values[{i}] contains non-finite or negative value: {v}"
                    )));
                }
            }
        }
        Ok(Self {
            beta_axis,
            delta_lambda_axis,
            s10_values,
            provenance,
        })
    }

    pub(super) fn lookup_s10(&self, beta_deg: f64, delta_lambda_deg: f64) -> Result<S10> {
        let beta_deg = beta_deg.abs().min(90.0);
        let delta_lambda_deg = delta_lambda_deg.abs().min(180.0);
        let (ib0, ib1, tb) = bracket(&self.beta_axis, beta_deg);
        let (il0, il1, tl) = bracket(&self.delta_lambda_axis, delta_lambda_deg);
        Ok(S10::new(bilinear(
            self.s10_values[ib0][il0],
            self.s10_values[ib0][il1],
            self.s10_values[ib1][il0],
            self.s10_values[ib1][il1],
            tb,
            tl,
        )))
    }
}

fn is_strictly_increasing(v: &[f64]) -> bool {
    v.windows(2).all(|w| w[1] > w[0])
}

fn bracket(axis: &[f64], value: f64) -> (usize, usize, f64) {
    let pos = axis.partition_point(|&x| x <= value);
    let i1 = pos.min(axis.len() - 1);
    let i0 = if i1 == 0 { 0 } else { i1 - 1 };
    let t = if axis[i1] > axis[i0] {
        (value - axis[i0]) / (axis[i1] - axis[i0])
    } else {
        0.0
    };
    (i0, i1, t.clamp(0.0, 1.0))
}

fn bilinear(v00: f64, v01: f64, v10: f64, v11: f64, tx: f64, ty: f64) -> f64 {
    let r0 = v00 + tx * (v10 - v00);
    let r1 = v01 + tx * (v11 - v01);
    r0 + ty * (r1 - r0)
}

#[derive(Debug, Clone)]
pub enum ZodiacalBrightnessModel {
    Leinert1998,
    CustomGrid(ZodiacalBrightnessGrid),
}

#[derive(Debug, Clone)]
pub struct ZodiacalLight {
    brightness_model: ZodiacalBrightnessModel,
    solar_spectrum: SampledSpectrum<Nanometer, Meter>,
    extinction: ZodiacalExtinction,
}

impl ZodiacalLight {
    pub fn leinert1998() -> Result<Self> {
        Ok(Self {
            brightness_model: ZodiacalBrightnessModel::Leinert1998,
            solar_spectrum: solar::load()?,
            extinction: ZodiacalExtinction::Noll2012Approx,
        })
    }

    pub fn with_brightness_model(model: ZodiacalBrightnessModel) -> Result<Self> {
        Ok(Self {
            brightness_model: model,
            solar_spectrum: solar::load()?,
            extinction: ZodiacalExtinction::Noll2012Approx,
        })
    }

    pub fn with_solar_spectrum(mut self, solar_spectrum: SampledSpectrum<Nanometer, Meter>) -> Self {
        self.solar_spectrum = solar_spectrum;
        self
    }

    pub fn with_extinction(mut self, extinction: ZodiacalExtinction) -> Self {
        self.extinction = extinction;
        self
    }

    pub fn compute(
        &self,
        time: Time<UTC>,
        location: Geodetic<ECEF>,
        target: Target,
    ) -> Result<ZodiacalOutputs> {
        self.compute_observed(time, location, target)
    }

    pub fn compute_exoatmospheric(
        &self,
        time: Time<UTC>,
        target: Target,
    ) -> Result<ZodiacalOutputs> {
        let geom = geometry::compute_exoatmospheric(time, target)?;
        self.evaluate_geometry(&geom, ZodiacalExtinction::None)
    }

    pub fn compute_observed(
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

    pub fn compute_spectrum(
        &self,
        time: Time<UTC>,
        location: Geodetic<ECEF>,
        target: Target,
    ) -> Result<ZodiacalSpectrum> {
        let geom = geometry::compute_observed(time, location, target)?;
        if is_below_horizon(&geom) {
            return zero_spectrum();
        }
        self.evaluate_geometry_spectrum(&geom, self.extinction)
    }

    pub fn compute_spectrum_exoatmospheric(
        &self,
        time: Time<UTC>,
        target: Target,
    ) -> Result<ZodiacalSpectrum> {
        let geom = geometry::compute_exoatmospheric(time, target)?;
        self.evaluate_geometry_spectrum(&geom, ZodiacalExtinction::None)
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
            ZodiacalBrightnessModel::CustomGrid(grid) => {
                let s10_500 = self.custom_s10_500(geom, grid)?;
                zl_spectrum::compute_outputs_with_s10(
                    geom,
                    &self.solar_spectrum,
                    extinction,
                    s10_500,
                )
            }
        }
    }

    fn evaluate_geometry_spectrum(
        &self,
        geom: &geometry::ZodiacalGeometry,
        extinction: ZodiacalExtinction,
    ) -> Result<ZodiacalSpectrum> {
        match &self.brightness_model {
            ZodiacalBrightnessModel::Leinert1998 => {
                zl_spectrum::compute_spectrum(geom, &self.solar_spectrum, extinction)
            }
            ZodiacalBrightnessModel::CustomGrid(grid) => {
                let s10_500 = self.custom_s10_500(geom, grid)?;
                zl_spectrum::compute_spectrum_with_s10(
                    geom,
                    &self.solar_spectrum,
                    extinction,
                    s10_500,
                )
            }
        }
    }

    fn custom_s10_500(
        &self,
        geom: &geometry::ZodiacalGeometry,
        grid: &ZodiacalBrightnessGrid,
    ) -> Result<S10> {
        use qtty::angular::Degree;
        let beta_deg = geom.beta.abs().to::<Degree>().value();
        let dl_deg = geom.delta_lambda.to::<Degree>().value().abs().min(180.0);
        grid.lookup_s10(beta_deg, dl_deg)
    }
}

fn is_below_horizon(geom: &geometry::ZodiacalGeometry) -> bool {
    geom.zenith
        .map(|z| (qtty::angular::Degrees::new(90.0) - z).value() <= 0.0)
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

fn zero_spectrum() -> Result<ZodiacalSpectrum> {
    use optica::data::Provenance;
    use optica::grid::OutOfRange;
    use optica::spectrum::Interpolation;
    use qtty::radiometry::{
        PhotonsPerSquareCentimeterNanosecondSteradian as BandPhotonRadiance, S10s as S10,
    };

    let spectrum = SampledSpectrum::<Nanometer, Meter>::from_raw(
        vec![300.0, 650.0],
        vec![0.0, 0.0],
        Interpolation::Linear,
        OutOfRange::ClampToEndpoints,
        Some(Provenance::computed("zodiacal-zero")),
    )
    .map_err(|e| NsbError::Interpolation(format!("zodiacal zero spectrum: {e}")))?;

    Ok(ZodiacalSpectrum {
        spectrum,
        integrated: BandPhotonRadiance::new(0.0),
        b_flux_s10: S10::new(0.0),
        v_flux_s10: S10::new(0.0),
    })
}
