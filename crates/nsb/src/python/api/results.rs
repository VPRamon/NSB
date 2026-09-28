use chrono::{DateTime, Utc};
use pyo3::prelude::*;

use crate::{NsbComponent, NsbComponentMetadata, NsbResult, ThresholdQueryResult};

use super::super::compat::period_to_datetimes;

#[pyclass(
    name = "NsbComponentMetadata",
    frozen,
    module = "nsb",
    get_all,
    skip_from_py_object
)]
#[derive(Clone)]
pub(super) struct PyNsbComponentMetadata {
    status: String,
    provenance: String,
    validated_domain: String,
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
            status: value.status.as_str().to_owned(),
            provenance: value.provenance.into_owned(),
            validated_domain: value.validated_domain.into_owned(),
            airglow_selection_kind: selection
                .map(|selection| selection.selection_kind.as_str().to_owned()),
            airglow_requested_model: selection
                .and_then(|selection| selection.requested_model)
                .map(|model| model.as_str().to_owned()),
            airglow_resolved_model: selection
                .and_then(|selection| selection.resolved_model)
                .map(|model| model.as_str().to_owned()),
            airglow_used_automatic_fallback: selection
                .map(|selection| selection.used_automatic_fallback),
            airglow_fallback_reason: selection
                .and_then(|selection| selection.fallback_reason)
                .map(|reason| reason.as_str().to_owned()),
            airglow_physical_outcome: evaluation
                .map(|evaluation| evaluation.physical_outcome.as_str().to_owned()),
            airglow_physical_zero_reason: evaluation
                .and_then(|evaluation| evaluation.physical_zero_reason)
                .map(|reason| reason.as_str().to_owned()),
            solar_activity_provenance: value
                .solar_activity
                .as_ref()
                .map(|solar| solar.provenance_fragment()),
        }
    }
}

#[pyclass(
    name = "NsbComponent",
    frozen,
    module = "nsb",
    get_all,
    skip_from_py_object
)]
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
            name: value.name.to_owned(),
            integrated_photons_cm2_ns_sr: value.integrated.value(),
            b_flux_s10: value.b_flux_s10.value(),
            v_flux_s10: value.v_flux_s10.value(),
            relative_uncertainty: value.relative_uncertainty,
            statistical_uncertainty_photons_cm2_ns_sr: value
                .statistical_uncertainty
                .map(|q| q.value()),
            systematic_uncertainty_photons_cm2_ns_sr: value
                .systematic_uncertainty
                .map(|q| q.value()),
            total_uncertainty_photons_cm2_ns_sr: value.total_uncertainty.map(|q| q.value()),
            metadata: value.metadata.into(),
        }
    }
}

#[pyclass(
    name = "NsbResult",
    frozen,
    module = "nsb",
    get_all,
    skip_from_py_object
)]
pub(super) struct PyNsbResult {
    integrated_photons_cm2_ns_sr: f64,
    b_mag_per_arcsec2: f64,
    v_mag_per_arcsec2: f64,
    components: Vec<PyNsbComponent>,
}

impl From<NsbResult> for PyNsbResult {
    fn from(value: NsbResult) -> Self {
        Self {
            integrated_photons_cm2_ns_sr: value.integrated.value(),
            b_mag_per_arcsec2: value.b_mag.value(),
            v_mag_per_arcsec2: value.v_mag.value(),
            components: value.components.into_iter().map(Into::into).collect(),
        }
    }
}

#[pyclass(
    name = "ThresholdQueryResult",
    frozen,
    module = "nsb",
    get_all,
    skip_from_py_object
)]
pub(super) struct PyThresholdQueryResult {
    threshold_photons_cm2_ns_sr: f64,
    periods: Vec<(DateTime<Utc>, DateTime<Utc>)>,
}

impl PyThresholdQueryResult {
    pub(super) fn try_from_inner(value: ThresholdQueryResult) -> PyResult<Self> {
        Ok(Self {
            threshold_photons_cm2_ns_sr: value.threshold.value(),
            periods: value
                .periods
                .into_iter()
                .map(period_to_datetimes)
                .collect::<PyResult<_>>()?,
        })
    }
}
