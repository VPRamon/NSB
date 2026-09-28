use pyo3::prelude::*;

use crate::{
    AirglowModel, AirglowSelection, MoonlightModel, NsbModelConfig, SiteProfileId,
    ZodiacalExtinction,
};

#[pymethods]
impl NsbModelConfig {
    #[staticmethod]
    #[pyo3(name = "generic_clear_sky")]
    fn py_generic_clear_sky() -> Self {
        Self::generic_clear_sky()
    }

    #[pyo3(name = "with_site_profile")]
    fn py_with_site_profile(&self, profile: SiteProfileId) -> Self {
        self.clone().with_site_profile(profile)
    }

    #[pyo3(name = "with_moonlight_model")]
    fn py_with_moonlight_model(&self, model: MoonlightModel) -> Self {
        self.clone().with_moonlight_model(model)
    }

    #[pyo3(name = "with_airglow_model")]
    fn py_with_airglow_model(&self, model: AirglowModel) -> Self {
        self.clone().with_airglow_model(model)
    }

    #[pyo3(name = "with_automatic_airglow")]
    fn py_with_automatic_airglow(&self) -> Self {
        self.clone()
            .with_airglow_selection(AirglowSelection::Automatic)
    }

    #[pyo3(name = "with_zodiacal_extinction")]
    fn py_with_zodiacal_extinction(&self, extinction: ZodiacalExtinction) -> Self {
        self.clone().with_zodiacal_extinction(extinction)
    }

    #[getter(site_profile)]
    fn py_site_profile(&self) -> SiteProfileId {
        self.site_profile()
    }

    #[getter(moonlight_model)]
    fn py_moonlight_model(&self) -> MoonlightModel {
        self.moonlight_model()
    }

    #[getter(airglow_selection)]
    fn py_airglow_selection(&self) -> &'static str {
        self.airglow_selection().as_str()
    }

    #[getter(airglow_model)]
    fn py_airglow_model(&self) -> Option<AirglowModel> {
        self.airglow_model()
    }

    #[getter(zodiacal_extinction)]
    fn py_zodiacal_extinction(&self) -> ZodiacalExtinction {
        self.zodiacal_extinction()
    }

    #[getter(is_airglow_site_calibrated)]
    fn py_is_airglow_site_calibrated(&self) -> bool {
        self.is_airglow_site_calibrated()
    }

    fn __repr__(&self) -> String {
        format!(
            "NsbModelConfig(site_profile='{}', moonlight_model='{}', airglow_selection='{}', zodiacal_extinction='{}')",
            self.site_profile().as_str(),
            self.moonlight_model().as_str(),
            self.airglow_selection().as_str(),
            self.zodiacal_extinction().as_str(),
        )
    }
}
