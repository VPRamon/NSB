use pyo3::prelude::*;

use crate::{AirglowSelection, NsbModelConfig};

use super::selectors::{
    PyAirglowModel, PyMoonlightModel, PySiteProfile, PyStarlightProduct, PyZodiacalExtinction,
    PyZodiacalModel,
};

#[pyclass(name = "NsbModelConfig", frozen, module = "nsb")]
#[derive(Clone)]
pub(super) struct PyNsbModelConfig {
    inner: NsbModelConfig,
}

impl PyNsbModelConfig {
    pub(super) fn inner(&self) -> NsbModelConfig {
        self.inner.clone()
    }

    pub(super) fn from_inner(inner: NsbModelConfig) -> Self {
        Self { inner }
    }
}

#[pymethods]
impl PyNsbModelConfig {
    #[new]
    fn new() -> Self {
        Self::from_inner(NsbModelConfig::generic_clear_sky())
    }

    #[staticmethod]
    fn generic_clear_sky(py: Python<'_>) -> PyResult<Py<Self>> {
        Py::new(py, Self::from_inner(NsbModelConfig::generic_clear_sky()))
    }

    #[staticmethod]
    fn cta_n_planning(py: Python<'_>) -> PyResult<Py<Self>> {
        Py::new(py, Self::from_inner(NsbModelConfig::cta_n_planning()))
    }

    #[staticmethod]
    fn cta_s_planning(py: Python<'_>) -> PyResult<Py<Self>> {
        Py::new(py, Self::from_inner(NsbModelConfig::cta_s_planning()))
    }

    fn with_site_profile(&self, py: Python<'_>, profile: &PySiteProfile) -> PyResult<Py<Self>> {
        Py::new(
            py,
            Self::from_inner(self.inner.clone().with_site_profile((*profile).into())),
        )
    }

    fn with_moonlight_model(
        &self,
        py: Python<'_>,
        model: &PyMoonlightModel,
    ) -> PyResult<Py<Self>> {
        Py::new(
            py,
            Self::from_inner(self.inner.clone().with_moonlight_model((*model).into())),
        )
    }

    fn with_airglow_model(
        &self,
        py: Python<'_>,
        model: &PyAirglowModel,
    ) -> PyResult<Py<Self>> {
        Py::new(
            py,
            Self::from_inner(self.inner.clone().with_airglow_model((*model).into())),
        )
    }

    fn with_automatic_airglow(&self, py: Python<'_>) -> PyResult<Py<Self>> {
        Py::new(
            py,
            Self::from_inner(
                self.inner
                    .clone()
                    .with_airglow_selection(AirglowSelection::Automatic),
            ),
        )
    }

    fn with_zodiacal_model(
        &self,
        py: Python<'_>,
        model: &PyZodiacalModel,
    ) -> PyResult<Py<Self>> {
        Py::new(
            py,
            Self::from_inner(self.inner.clone().with_zodiacal_model((*model).into())),
        )
    }

    fn with_zodiacal_extinction(
        &self,
        py: Python<'_>,
        extinction: &PyZodiacalExtinction,
    ) -> PyResult<Py<Self>> {
        Py::new(
            py,
            Self::from_inner(
                self.inner
                    .clone()
                    .with_zodiacal_extinction((*extinction).into()),
            ),
        )
    }

    fn with_starlight_product(
        &self,
        py: Python<'_>,
        product: &PyStarlightProduct,
    ) -> PyResult<Py<Self>> {
        Py::new(
            py,
            Self::from_inner(
                self.inner
                    .clone()
                    .with_starlight_product((*product).into()),
            ),
        )
    }

    #[getter]
    fn site_profile(&self, py: Python<'_>) -> PyResult<Py<PySiteProfile>> {
        Py::new(py, self.inner.site_profile().into())
    }

    #[getter]
    fn moonlight_model(&self, py: Python<'_>) -> PyResult<Py<PyMoonlightModel>> {
        Py::new(py, self.inner.moonlight_model().into())
    }

    #[getter]
    fn zodiacal_model(&self, py: Python<'_>) -> PyResult<Py<PyZodiacalModel>> {
        Py::new(py, self.inner.zodiacal_model().into())
    }

    #[getter]
    fn zodiacal_extinction(&self, py: Python<'_>) -> PyResult<Py<PyZodiacalExtinction>> {
        Py::new(py, self.inner.zodiacal_extinction().into())
    }

    #[getter]
    fn airglow_selection(&self) -> &'static str {
        self.inner.airglow_selection().as_str()
    }

    #[getter]
    fn airglow_model(&self, py: Python<'_>) -> PyResult<Option<Py<PyAirglowModel>>> {
        self.inner
            .airglow_model()
            .map(|model| Py::new(py, model.into()))
            .transpose()
    }

    #[getter]
    fn starlight_product(&self) -> Option<&'static str> {
        self.inner.starlight_product().map(|product| product.as_str())
    }

    #[getter]
    fn is_airglow_site_calibrated(&self) -> bool {
        self.inner.is_airglow_site_calibrated()
    }

    fn __repr__(&self) -> String {
        format!(
            "NsbModelConfig(site_profile='{}', moonlight_model='{}', airglow_selection='{}', zodiacal_model='{}', zodiacal_extinction='{}')",
            self.inner.site_profile().as_str(),
            self.inner.moonlight_model().as_str(),
            self.inner.airglow_selection().as_str(),
            self.inner.zodiacal_model().as_str(),
            self.inner.zodiacal_extinction().as_str(),
        )
    }
}
