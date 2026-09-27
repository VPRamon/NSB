use pyo3::prelude::*;

use crate::{
    AirglowModel, ComponentMask, MoonlightModel, SiteProfileId, StarlightProduct,
    ZodiacalExtinction, ZodiacalModel,
};

#[pyclass(
    name = "SiteProfile",
    eq,
    rename_all = "SCREAMING_SNAKE_CASE",
    module = "nsb"
)]
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum PySiteProfile {
    GenericClearSky,
    CtaNorth,
    CtaSouth,
}

impl From<PySiteProfile> for SiteProfileId {
    fn from(value: PySiteProfile) -> Self {
        match value {
            PySiteProfile::GenericClearSky => Self::GenericClearSky,
            PySiteProfile::CtaNorth => Self::CtaNorth,
            PySiteProfile::CtaSouth => Self::CtaSouth,
        }
    }
}

impl From<SiteProfileId> for PySiteProfile {
    fn from(value: SiteProfileId) -> Self {
        match value {
            SiteProfileId::GenericClearSky => Self::GenericClearSky,
            SiteProfileId::CtaNorth => Self::CtaNorth,
            SiteProfileId::CtaSouth => Self::CtaSouth,
        }
    }
}

#[pymethods]
impl PySiteProfile {
    fn as_str(&self) -> &'static str {
        SiteProfileId::from(*self).as_str()
    }
}

#[pyclass(
    name = "MoonlightModel",
    eq,
    rename_all = "SCREAMING_SNAKE_CASE",
    module = "nsb"
)]
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum PyMoonlightModel {
    KrisciunasSchaefer1991,
    Jones2013Spectral,
}

impl From<PyMoonlightModel> for MoonlightModel {
    fn from(value: PyMoonlightModel) -> Self {
        match value {
            PyMoonlightModel::KrisciunasSchaefer1991 => Self::KrisciunasSchaefer1991,
            PyMoonlightModel::Jones2013Spectral => Self::Jones2013Spectral,
        }
    }
}

impl From<MoonlightModel> for PyMoonlightModel {
    fn from(value: MoonlightModel) -> Self {
        match value {
            MoonlightModel::KrisciunasSchaefer1991 => Self::KrisciunasSchaefer1991,
            MoonlightModel::Jones2013Spectral => Self::Jones2013Spectral,
        }
    }
}

#[pymethods]
impl PyMoonlightModel {
    fn as_str(&self) -> &'static str {
        MoonlightModel::from(*self).as_str()
    }
}

#[pyclass(
    name = "AirglowModel",
    eq,
    rename_all = "SCREAMING_SNAKE_CASE",
    module = "nsb"
)]
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum PyAirglowModel {
    ParanalNollSkyCalcFors1,
}

impl From<PyAirglowModel> for AirglowModel {
    fn from(value: PyAirglowModel) -> Self {
        match value {
            PyAirglowModel::ParanalNollSkyCalcFors1 => Self::ParanalNollSkyCalcFors1,
        }
    }
}

impl From<AirglowModel> for PyAirglowModel {
    fn from(value: AirglowModel) -> Self {
        match value {
            AirglowModel::ParanalNollSkyCalcFors1 => Self::ParanalNollSkyCalcFors1,
        }
    }
}

#[pymethods]
impl PyAirglowModel {
    fn as_str(&self) -> &'static str {
        AirglowModel::from(*self).as_str()
    }
}

#[pyclass(
    name = "ZodiacalModel",
    eq,
    rename_all = "SCREAMING_SNAKE_CASE",
    module = "nsb"
)]
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum PyZodiacalModel {
    Leinert1998,
}

impl From<PyZodiacalModel> for ZodiacalModel {
    fn from(value: PyZodiacalModel) -> Self {
        match value {
            PyZodiacalModel::Leinert1998 => Self::Leinert1998,
        }
    }
}

impl From<ZodiacalModel> for PyZodiacalModel {
    fn from(value: ZodiacalModel) -> Self {
        match value {
            ZodiacalModel::Leinert1998 => Self::Leinert1998,
        }
    }
}

#[pymethods]
impl PyZodiacalModel {
    fn as_str(&self) -> &'static str {
        ZodiacalModel::from(*self).as_str()
    }
}

#[pyclass(
    name = "ZodiacalExtinction",
    eq,
    rename_all = "SCREAMING_SNAKE_CASE",
    module = "nsb"
)]
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum PyZodiacalExtinction {
    None,
    Noll2012Approx,
}

impl From<PyZodiacalExtinction> for ZodiacalExtinction {
    fn from(value: PyZodiacalExtinction) -> Self {
        match value {
            PyZodiacalExtinction::None => Self::None,
            PyZodiacalExtinction::Noll2012Approx => Self::Noll2012Approx,
        }
    }
}

impl From<ZodiacalExtinction> for PyZodiacalExtinction {
    fn from(value: ZodiacalExtinction) -> Self {
        match value {
            ZodiacalExtinction::None => Self::None,
            ZodiacalExtinction::Noll2012Approx => Self::Noll2012Approx,
        }
    }
}

#[pymethods]
impl PyZodiacalExtinction {
    fn as_str(&self) -> &'static str {
        ZodiacalExtinction::from(*self).as_str()
    }
}

#[pyclass(
    name = "StarlightProduct",
    eq,
    rename_all = "SCREAMING_SNAKE_CASE",
    module = "nsb"
)]
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum PyStarlightProduct {
    BundledProductionGaiaDr3,
}

impl From<PyStarlightProduct> for StarlightProduct {
    fn from(value: PyStarlightProduct) -> Self {
        match value {
            PyStarlightProduct::BundledProductionGaiaDr3 => {
                StarlightProduct::bundled_production_gaia_dr3()
            }
        }
    }
}

#[pymethods]
impl PyStarlightProduct {
    fn as_str(&self) -> &'static str {
        "bundled-production-gaia-dr3"
    }

    #[staticmethod]
    fn bundled_production_available() -> bool {
        StarlightProduct::bundled_production_available()
    }
}

#[pyclass(name = "ComponentMask", frozen, module = "nsb")]
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) struct PyComponentMask {
    inner: ComponentMask,
}

impl PyComponentMask {
    pub(super) const fn from_inner(inner: ComponentMask) -> Self {
        Self { inner }
    }

    pub(super) const fn inner(&self) -> ComponentMask {
        self.inner
    }
}

#[pymethods]
impl PyComponentMask {
    #[new]
    fn new(bits: u8) -> PyResult<Self> {
        ComponentMask::from_bits(bits)
            .map(Self::from_inner)
            .ok_or_else(|| {
                pyo3::exceptions::PyValueError::new_err(format!(
                    "unknown ComponentMask bits: 0x{bits:02x}"
                ))
            })
    }

    #[getter]
    fn bits(&self) -> u8 {
        self.inner.bits()
    }

    fn contains(&self, other: &Self) -> bool {
        self.inner.contains(other.inner)
    }

    fn __or__(&self, py: Python<'_>, other: &Self) -> PyResult<Py<Self>> {
        Py::new(py, Self::from_inner(self.inner | other.inner))
    }

    fn __and__(&self, py: Python<'_>, other: &Self) -> PyResult<Py<Self>> {
        Py::new(py, Self::from_inner(self.inner & other.inner))
    }

    fn __repr__(&self) -> String {
        format!("ComponentMask(bits=0x{:02x})", self.inner.bits())
    }
}

pub(super) fn install_component_mask_constants(module: &Bound<'_, PyModule>) -> PyResult<()> {
    let py = module.py();
    let ty = module.getattr("ComponentMask")?;
    for (name, value) in [
        ("ZODIACAL", ComponentMask::ZODIACAL),
        ("STARLIGHT", ComponentMask::STARLIGHT),
        ("AIRGLOW", ComponentMask::AIRGLOW),
        ("MOON", ComponentMask::MOON),
        ("DEFAULT", ComponentMask::DEFAULT),
        ("ALL", ComponentMask::ALL),
    ] {
        ty.setattr(name, Py::new(py, PyComponentMask::from_inner(value))?)?;
    }
    Ok(())
}
