// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Vallés Puig, Ramon
use super::errors::{invalid_input, to_py_err};
use crate::units::angular::Degrees;
use crate::StarlightIntervalMap;
use pyo3::prelude::*;
use siderust::coordinates::{cartesian::Direction, frames::Galactic, spherical};

fn direction(l: f64, b: f64) -> PyResult<Direction<Galactic>> {
    if !l.is_finite()
        || !b.is_finite()
        || !(0.0..360.0).contains(&l)
        || !(-90.0..=90.0).contains(&b)
    {
        return Err(invalid_input(
            "Galactic coordinates require 0<=l<360, -90<=b<=90",
        ));
    }
    Ok(spherical::Direction::<Galactic>::new(Degrees::new(l), Degrees::new(b)).to_cartesian())
}

#[pyclass(name = "StarlightIntervalMap", frozen, module = "nsb")]
pub(in crate::python) struct PyStarlightIntervalMap {
    inner: StarlightIntervalMap,
}

#[pymethods]
impl PyStarlightIntervalMap {
    /// Load exact pinned candidate bytes. Does not alter the default NSB model.
    #[staticmethod]
    fn load(path: &str, sha256: &str) -> PyResult<Self> {
        Ok(Self {
            inner: StarlightIntervalMap::from_json_file(std::path::Path::new(path), sha256)
                .map_err(to_py_err)?,
        })
    }
    #[getter]
    fn sha256(&self) -> &str {
        self.inner.sha256()
    }
    #[getter]
    fn band_nm(&self) -> [u16; 2] {
        self.inner.band_nm()
    }
    #[getter]
    fn maturity(&self) -> &str {
        self.inner.maturity()
    }
    #[getter]
    fn nside(&self) -> u32 {
        self.inner.nside()
    }
    #[getter]
    fn admitted_sources(&self) -> u64 {
        self.inner.admitted_sources()
    }
    /// TOA pixel-mean radiance and statistical sigma, in photons/(cm2 ns sr).
    fn top_of_atmosphere_pixel(&self, l_deg: f64, b_deg: f64) -> PyResult<(f64, f64)> {
        let (r, s) = self
            .inner
            .top_of_atmosphere_pixel(direction(l_deg, b_deg)?)
            .map_err(to_py_err)?;
        Ok((r.value(), s.value()))
    }
    /// Direct pixel envelope using generic aerosols and explicit local pressure.
    /// Returns midpoint, lower, upper, conservative statistical sigma bound.
    /// Does not represent a source-masked fiber or include in-scattering.
    fn direct_pixel_bounds(
        &self,
        l_deg: f64,
        b_deg: f64,
        zenith_deg: f64,
        pressure_hpa: f64,
    ) -> PyResult<(f64, f64, f64, f64)> {
        let geometry = crate::transport::DirectPathGeometry::new(Degrees::new(zenith_deg))
            .map_err(to_py_err)?;
        let atmosphere = crate::site::AtmosphericConditions::clear_sky_with_pressure(
            siderust::qtty::Hectopascals::new(pressure_hpa),
        )
        .map_err(to_py_err)?;
        let b = self
            .inner
            .direct_pixel_bounds(direction(l_deg, b_deg)?, geometry, atmosphere)
            .map_err(to_py_err)?;
        Ok((
            b.midpoint.value(),
            b.lower.value(),
            b.upper.value(),
            b.statistical_sigma_bound.value(),
        ))
    }
}
