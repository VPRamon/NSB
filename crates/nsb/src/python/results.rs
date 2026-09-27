use chrono::{DateTime, Utc};
use pyo3::prelude::*;

use crate::{
    BandDiagnostic, NsbComponent, NsbComponentDescriptor, NsbComponentMetadata, NsbResult,
    ThresholdQueryResult,
};

use super::queries::time_to_py_datetime;

#[pyclass(name = "BandDiagnostic", frozen, module = "nsb")]
#[derive(Clone)]
pub(super) struct PyBandDiagnostic {
    convention: String,
    b_reference_nm: f64,
    v_reference_nm: f64,
    zero_point_mag_per_arcsec2: f64,
}

impl From<BandDiagnostic> for PyBandDiagnostic {
    fn from(value: BandDiagnostic) -> Self {
        Self {
            convention: value.convention.to_string(),
            b_reference_nm: value.b_reference.value(),
            v_reference_nm: value.v_reference.value(),
            zero_point_mag_per_arcsec2: value.zero_point.value(),
        }
    }
}

#[pymethods]
impl PyBandDiagnostic {
    #[getter]
    fn convention(&self) -> &str {
        &self.convention
    }

    #[getter]
    fn b_reference_nm(&self) -> f64 {
        self.b_reference_nm
    }

    #[getter]
    fn v_reference_nm(&self) -> f64 {
        self.v_reference_nm
    }

    #[getter]
    fn zero_point_mag_per_arcsec2(&self) -> f64 {
        self.zero_point_mag_per_arcsec2
    }
}

#[pyclass(name = "NsbComponentMetadata", frozen, module = "nsb")]
#[derive(Clone)]
pub(super) struct PyNsbComponentMetadata {
    status: String,
    provenance: String,
    validated_domain: String,
    band_diagnostic: PyBandDiagnostic,
    airglow_selection_kind: Option<String>,
    airglow_requested_model: Option<String>,
    airglow_resolved_model: Option<String>,
    airglow_used_automatic_fallback: Option<bool>,
    airglow_fallback_reason: Option<String>,
    airglow_physical_outcome: Option<String>,
    airglow_physical_zero_reason: Option<String>,
    solar_activity_provenance: Option<String>,
}

impl From<NsbComponentMetadata> for PyNsbComponentMetadata {
    fn from(value: NsbComponentMetadata) -> Self {
        let selection = value.airglow_selection.as_ref();
        let evaluation = value.airglow_evaluation.as_ref();
        Self {
            status: value.status.as_str().to_string(),
            provenance: value.provenance.into_owned(),
            validated_domain: value.validated_domain.into_owned(),
            band_diagnostic: value.band_diagnostic.into(),
            airglow_selection_kind: selection
                .map(|selection| selection.selection_kind.as_str().to_string()),
            airglow_requested_model: selection
                .and_then(|selection| selection.requested_model)
                .map(|model| model.as_str().to_string()),
            airglow_resolved_model: selection
                .and_then(|selection| selection.resolved_model)
                .map(|model| model.as_str().to_string()),
            airglow_used_automatic_fallback: selection
                .map(|selection| selection.used_automatic_fallback),
            airglow_fallback_reason: selection
                .and_then(|selection| selection.fallback_reason)
                .map(|reason| reason.as_str().to_string()),
            airglow_physical_outcome: evaluation
                .map(|evaluation| evaluation.physical_outcome.as_str().to_string()),
            airglow_physical_zero_reason: evaluation
                .and_then(|evaluation| evaluation.physical_zero_reason)
                .map(|reason| reason.as_str().to_string()),
            solar_activity_provenance: value
                .solar_activity
                .as_ref()
                .map(|solar| solar.provenance_fragment()),
        }
    }
}

#[pymethods]
impl PyNsbComponentMetadata {
    #[getter]
    fn status(&self) -> &str {
        &self.status
    }

    #[getter]
    fn provenance(&self) -> &str {
        &self.provenance
    }

    #[getter]
    fn validated_domain(&self) -> &str {
        &self.validated_domain
    }

    #[getter]
    fn band_diagnostic(&self, py: Python<'_>) -> PyResult<Py<PyBandDiagnostic>> {
        Py::new(py, self.band_diagnostic.clone())
    }

    #[getter]
    fn airglow_selection_kind(&self) -> Option<&str> {
        self.airglow_selection_kind.as_deref()
    }

    #[getter]
    fn airglow_requested_model(&self) -> Option<&str> {
        self.airglow_requested_model.as_deref()
    }

    #[getter]
    fn airglow_resolved_model(&self) -> Option<&str> {
        self.airglow_resolved_model.as_deref()
    }

    #[getter]
    fn airglow_used_automatic_fallback(&self) -> Option<bool> {
        self.airglow_used_automatic_fallback
    }

    #[getter]
    fn airglow_fallback_reason(&self) -> Option<&str> {
        self.airglow_fallback_reason.as_deref()
    }

    #[getter]
    fn airglow_physical_outcome(&self) -> Option<&str> {
        self.airglow_physical_outcome.as_deref()
    }

    #[getter]
    fn airglow_physical_zero_reason(&self) -> Option<&str> {
        self.airglow_physical_zero_reason.as_deref()
    }

    #[getter]
    fn solar_activity_provenance(&self) -> Option<&str> {
        self.solar_activity_provenance.as_deref()
    }
}

#[pyclass(name = "NsbComponent", frozen, module = "nsb")]
#[derive(Clone)]
pub(super) struct PyNsbComponent {
    name: String,
    integrated_photons_cm2_ns_sr: f64,
    b_flux_s10: f64,
    v_flux_s10: f64,
    relative_uncertainty: Option<f64>,
    statistical_uncertainty_photons_cm2_ns_sr: Option<f64>,
    systematic_uncertainty_photons_cm2_ns_sr: Option<f64>,
    total_uncertainty_photons_cm2_ns_sr: Option<f64>,
    metadata: PyNsbComponentMetadata,
}

impl From<NsbComponent> for PyNsbComponent {
    fn from(value: NsbComponent) -> Self {
        Self {
            name: value.name.to_string(),
            integrated_photons_cm2_ns_sr: value.integrated.value(),
            b_flux_s10: value.b_flux_s10.value(),
            v_flux_s10: value.v_flux_s10.value(),
            relative_uncertainty: value.relative_uncertainty,
            statistical_uncertainty_photons_cm2_ns_sr: value
                .statistical_uncertainty
                .map(|quantity| quantity.value()),
            systematic_uncertainty_photons_cm2_ns_sr: value
                .systematic_uncertainty
                .map(|quantity| quantity.value()),
            total_uncertainty_photons_cm2_ns_sr: value
                .total_uncertainty
                .map(|quantity| quantity.value()),
            metadata: value.metadata.into(),
        }
    }
}

#[pymethods]
impl PyNsbComponent {
    #[getter]
    fn name(&self) -> &str {
        &self.name
    }

    #[getter]
    fn integrated_photons_cm2_ns_sr(&self) -> f64 {
        self.integrated_photons_cm2_ns_sr
    }

    #[getter]
    fn b_flux_s10(&self) -> f64 {
        self.b_flux_s10
    }

    #[getter]
    fn v_flux_s10(&self) -> f64 {
        self.v_flux_s10
    }

    #[getter]
    fn relative_uncertainty(&self) -> Option<f64> {
        self.relative_uncertainty
    }

    #[getter]
    fn statistical_uncertainty_photons_cm2_ns_sr(&self) -> Option<f64> {
        self.statistical_uncertainty_photons_cm2_ns_sr
    }

    #[getter]
    fn systematic_uncertainty_photons_cm2_ns_sr(&self) -> Option<f64> {
        self.systematic_uncertainty_photons_cm2_ns_sr
    }

    #[getter]
    fn total_uncertainty_photons_cm2_ns_sr(&self) -> Option<f64> {
        self.total_uncertainty_photons_cm2_ns_sr
    }

    #[getter]
    fn metadata(&self, py: Python<'_>) -> PyResult<Py<PyNsbComponentMetadata>> {
        Py::new(py, self.metadata.clone())
    }
}

#[pyclass(name = "NsbComponentDescriptor", frozen, module = "nsb")]
#[derive(Clone)]
pub(super) struct PyNsbComponentDescriptor {
    name: String,
    metadata: PyNsbComponentMetadata,
}

impl From<NsbComponentDescriptor> for PyNsbComponentDescriptor {
    fn from(value: NsbComponentDescriptor) -> Self {
        Self {
            name: value.name.to_string(),
            metadata: value.metadata.into(),
        }
    }
}

#[pymethods]
impl PyNsbComponentDescriptor {
    #[getter]
    fn name(&self) -> &str {
        &self.name
    }

    #[getter]
    fn metadata(&self, py: Python<'_>) -> PyResult<Py<PyNsbComponentMetadata>> {
        Py::new(py, self.metadata.clone())
    }
}

#[pyclass(name = "NsbResult", frozen, module = "nsb")]
pub(super) struct PyNsbResult {
    integrated_photons_cm2_ns_sr: f64,
    b_mag_per_arcsec2: f64,
    v_mag_per_arcsec2: f64,
    components: Vec<PyNsbComponent>,
    band_diagnostic: PyBandDiagnostic,
}

impl From<NsbResult> for PyNsbResult {
    fn from(value: NsbResult) -> Self {
        Self {
            integrated_photons_cm2_ns_sr: value.integrated.value(),
            b_mag_per_arcsec2: value.b_mag.value(),
            v_mag_per_arcsec2: value.v_mag.value(),
            components: value.components.into_iter().map(Into::into).collect(),
            band_diagnostic: value.band_diagnostic.into(),
        }
    }
}

#[pymethods]
impl PyNsbResult {
    #[getter]
    fn integrated_photons_cm2_ns_sr(&self) -> f64 {
        self.integrated_photons_cm2_ns_sr
    }

    #[getter]
    fn b_mag_per_arcsec2(&self) -> f64 {
        self.b_mag_per_arcsec2
    }

    #[getter]
    fn v_mag_per_arcsec2(&self) -> f64 {
        self.v_mag_per_arcsec2
    }

    #[getter]
    fn components(&self, py: Python<'_>) -> PyResult<Vec<Py<PyNsbComponent>>> {
        self.components
            .iter()
            .cloned()
            .map(|component| Py::new(py, component))
            .collect()
    }

    #[getter]
    fn band_diagnostic(&self, py: Python<'_>) -> PyResult<Py<PyBandDiagnostic>> {
        Py::new(py, self.band_diagnostic.clone())
    }
}

#[pyclass(name = "UtcPeriod", frozen, module = "nsb")]
#[derive(Clone)]
pub(super) struct PyUtcPeriod {
    start: DateTime<Utc>,
    end: DateTime<Utc>,
}

impl PyUtcPeriod {
    fn try_from_period(period: tempoch::Period<tempoch::UTC>) -> PyResult<Self> {
        Ok(Self {
            start: time_to_py_datetime(period.start)?,
            end: time_to_py_datetime(period.end)?,
        })
    }
}

#[pymethods]
impl PyUtcPeriod {
    #[getter]
    fn start(&self) -> DateTime<Utc> {
        self.start.clone()
    }

    #[getter]
    fn end(&self) -> DateTime<Utc> {
        self.end.clone()
    }

    fn __repr__(&self) -> String {
        format!("UtcPeriod(start='{}', end='{}')", self.start, self.end)
    }
}

#[pyclass(name = "ThresholdQueryResult", frozen, module = "nsb")]
pub(super) struct PyThresholdQueryResult {
    threshold_photons_cm2_ns_sr: f64,
    periods: Vec<PyUtcPeriod>,
}

impl PyThresholdQueryResult {
    pub(super) fn try_from_inner(value: ThresholdQueryResult) -> PyResult<Self> {
        Ok(Self {
            threshold_photons_cm2_ns_sr: value.threshold.value(),
            periods: value
                .periods
                .into_iter()
                .map(PyUtcPeriod::try_from_period)
                .collect::<PyResult<_>>()?,
        })
    }
}

#[pymethods]
impl PyThresholdQueryResult {
    #[getter]
    fn threshold_photons_cm2_ns_sr(&self) -> f64 {
        self.threshold_photons_cm2_ns_sr
    }

    #[getter]
    fn periods(&self, py: Python<'_>) -> PyResult<Vec<Py<PyUtcPeriod>>> {
        self.periods
            .iter()
            .cloned()
            .map(|period| Py::new(py, period))
            .collect()
    }
}
