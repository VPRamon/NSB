use pyo3::prelude::*;

use crate::{AirglowSelection, NsbModelConfig};

use super::selectors::{PyAirglowModel, PyMoonlightModel, PySiteProfile, PyZodiacalExtinction};

#[pyclass(name = "NsbModelConfig", frozen, module = "nsb", skip_from_py_object)]
#[derive(Clone)]
pub(in crate::python) struct PyNsbModelConfig {
    inner: NsbModelConfig,
}

impl PyNsbModelConfig {
    pub(in crate::python) fn inner(&self) -> NsbModelConfig {
        self.inner.clone()
    }

    pub(in crate::python) fn from_inner(inner: NsbModelConfig) -> Self {
        Self { inner }
    }
}

#[pymethods]
impl PyNsbModelConfig {
    #[staticmethod]
    fn generic_clear_sky() -> Self {
        Self::from_inner(NsbModelConfig::generic_clear_sky())
    }

    fn with_site_profile(&self, profile: PySiteProfile) -> Self {
        Self::from_inner(self.inner.clone().with_site_profile(profile.into()))
    }

    fn with_moonlight_model(&self, model: PyMoonlightModel) -> Self {
        Self::from_inner(self.inner.clone().with_moonlight_model(model.into()))
    }

    fn with_airglow_model(&self, model: PyAirglowModel) -> Self {
        Self::from_inner(self.inner.clone().with_airglow_model(model.into()))
    }

    fn with_automatic_airglow(&self) -> Self {
        Self::from_inner(
            self.inner
                .clone()
                .with_airglow_selection(AirglowSelection::Automatic),
        )
    }

    fn with_zodiacal_extinction(&self, extinction: PyZodiacalExtinction) -> Self {
        Self::from_inner(
            self.inner
                .clone()
                .with_zodiacal_extinction(extinction.into()),
        )
    }

    #[getter]
    fn site_profile(&self) -> PySiteProfile {
        self.inner.site_profile().into()
    }

    #[getter]
    fn moonlight_model(&self) -> PyMoonlightModel {
        self.inner.moonlight_model().into()
    }

    #[getter]
    fn airglow_selection(&self) -> &'static str {
        self.inner.airglow_selection().as_str()
    }

    #[getter]
    fn airglow_model(&self) -> Option<PyAirglowModel> {
        self.inner.airglow_model().map(Into::into)
    }

    #[getter]
    fn zodiacal_extinction(&self) -> PyZodiacalExtinction {
        self.inner.zodiacal_extinction().into()
    }

    #[getter]
    fn is_airglow_site_calibrated(&self) -> bool {
        self.inner.is_airglow_site_calibrated()
    }

    fn __repr__(&self) -> String {
        format!(
            "NsbModelConfig(site_profile='{}', moonlight_model='{}', airglow_selection='{}', zodiacal_extinction='{}')",
            self.inner.site_profile().as_str(),
            self.inner.moonlight_model().as_str(),
            self.inner.airglow_selection().as_str(),
            self.inner.zodiacal_extinction().as_str(),
        )
    }
}
