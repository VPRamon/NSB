//! PyO3 bindings.
//!
//! Build with `maturin develop --features python`. Exposes a small surface
//! mirroring the Rust API; this is intentionally minimal and focuses on
//! what the cross-validation harness needs.

use chrono::{DateTime, NaiveDateTime, Utc};
use pyo3::prelude::*;
use tempoch::{Time, UTC};

use crate::{calculate, ComponentMask, ObservationRequest, Site, Source};

#[pyclass(name = "NsbComponent")]
#[derive(Clone)]
pub struct PyNsbComponent {
    #[pyo3(get)] pub name: String,
    #[pyo3(get)] pub integrated: f64,
    #[pyo3(get)] pub b_s10: f64,
    #[pyo3(get)] pub v_s10: f64,
}

#[pyclass(name = "NsbResult")]
#[derive(Clone)]
pub struct PyNsbResult {
    #[pyo3(get)] pub integrated: f64,
    #[pyo3(get)] pub b_mag: f64,
    #[pyo3(get)] pub v_mag: f64,
    #[pyo3(get)] pub components: Vec<PyNsbComponent>,
}

#[pyfunction]
#[pyo3(signature = (site, obstime, source, components = None))]
fn calculate_py(
    site: &str,
    obstime: &str,
    source: &str,
    components: Option<Vec<String>>,
) -> PyResult<PyNsbResult> {
    let site = Site::from_name(site).map_err(to_py)?;
    let time = parse_iso_utc(obstime)?;
    let mask = match components {
        None => ComponentMask::ZODIACAL | ComponentMask::STARLIGHT | ComponentMask::AIRGLOW,
        Some(list) => {
            let mut m = ComponentMask::empty();
            for c in list {
                match c.to_lowercase().as_str() {
                    "zodiacal" | "zl" => m |= ComponentMask::ZODIACAL,
                    "starlight" | "sl" => m |= ComponentMask::STARLIGHT,
                    "airglow"   | "ag" => m |= ComponentMask::AIRGLOW,
                    "moon"      | "moonlight" => m |= ComponentMask::MOON,
                    "all" => m |= ComponentMask::ALL,
                    other => return Err(pyo3::exceptions::PyValueError::new_err(
                        format!("unknown component: {other}"))),
                }
            }
            m
        }
    };
    let req = ObservationRequest {
        site, time,
        source: Source::Named(source.to_string()),
        components: mask,
    };
    let r = calculate(&req).map_err(to_py)?;
    Ok(PyNsbResult {
        integrated: r.integrated.value(),
        b_mag: r.b_mag.value(),
        v_mag: r.v_mag.value(),
        components: r.components.into_iter().map(|c| PyNsbComponent {
            name: c.name.to_string(),
            integrated: c.integrated.value(),
            b_s10: c.b_flux_s10.value(),
            v_s10: c.v_flux_s10.value(),
        }).collect(),
    })
}

fn to_py<E: std::fmt::Display>(e: E) -> PyErr {
    pyo3::exceptions::PyValueError::new_err(e.to_string())
}

fn parse_iso_utc(s: &str) -> PyResult<Time<UTC>> {
    let ndt = NaiveDateTime::parse_from_str(s, "%Y-%m-%d %H:%M:%S")
        .or_else(|_| NaiveDateTime::parse_from_str(s, "%Y-%m-%dT%H:%M:%S"))
        .map_err(|_| pyo3::exceptions::PyValueError::new_err(format!("invalid time {s:?}")))?;
    let dt = DateTime::<Utc>::from_naive_utc_and_offset(ndt, Utc);
    Time::<UTC>::try_from_chrono(dt).map_err(to_py)
}

#[pymodule]
fn _native(_py: Python<'_>, m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<PyNsbComponent>()?;
    m.add_class::<PyNsbResult>()?;
    m.add_function(wrap_pyfunction!(calculate_py, m)?)?;
    Ok(())
}
