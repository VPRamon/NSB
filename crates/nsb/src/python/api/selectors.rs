use pyo3::prelude::*;

use crate::{AirglowModel, ComponentMask, MoonlightModel, SiteProfileId, ZodiacalExtinction};

macro_rules! python_selector {
    ($py:ident, $python_name:literal, $rust:ty, {$($variant:ident => $rust_variant:path),+ $(,)?}) => {
        #[pyclass(
            name = $python_name,
            eq,
            frozen,
            rename_all = "SCREAMING_SNAKE_CASE",
            module = "nsb",
            from_py_object
        )]
        #[derive(Clone, Copy, PartialEq, Eq)]
        pub(in crate::python) enum $py {
            $($variant),+
        }

        impl From<$py> for $rust {
            fn from(value: $py) -> Self {
                match value {
                    $($py::$variant => $rust_variant),+
                }
            }
        }

        impl From<$rust> for $py {
            fn from(value: $rust) -> Self {
                match value {
                    $($rust_variant => $py::$variant),+
                }
            }
        }

        #[pymethods]
        impl $py {
            fn as_str(&self) -> &'static str {
                let value: $rust = (*self).into();
                value.as_str()
            }

            fn __repr__(&self) -> String {
                format!("{}('{}')", $python_name, self.as_str())
            }
        }
    };
}

python_selector!(
    PySiteProfile,
    "SiteProfile",
    SiteProfileId,
    {
        GenericClearSky => SiteProfileId::GenericClearSky,
        CtaNorth => SiteProfileId::CtaNorth,
        CtaSouth => SiteProfileId::CtaSouth,
    }
);

python_selector!(
    PyMoonlightModel,
    "MoonlightModel",
    MoonlightModel,
    {
        KrisciunasSchaefer1991 => MoonlightModel::KrisciunasSchaefer1991,
        Jones2013Spectral => MoonlightModel::Jones2013Spectral,
    }
);

python_selector!(
    PyAirglowModel,
    "AirglowModel",
    AirglowModel,
    {
        ParanalNollSkyCalcFors1 => AirglowModel::ParanalNollSkyCalcFors1,
    }
);

python_selector!(
    PyZodiacalExtinction,
    "ZodiacalExtinction",
    ZodiacalExtinction,
    {
        None => ZodiacalExtinction::None,
        Noll2012Approx => ZodiacalExtinction::Noll2012Approx,
    }
);

#[pyclass(name = "ComponentMask", frozen, module = "nsb", from_py_object)]
#[derive(Clone, Copy, PartialEq, Eq)]
pub(in crate::python) struct PyComponentMask {
    inner: ComponentMask,
}

impl PyComponentMask {
    pub(in crate::python) const fn inner(&self) -> ComponentMask {
        self.inner
    }

    pub(in crate::python) const fn from_inner(inner: ComponentMask) -> Self {
        Self { inner }
    }
}

#[pymethods]
impl PyComponentMask {
    #[getter]
    fn bits(&self) -> u8 {
        self.inner.bits()
    }

    fn contains(&self, other: Self) -> bool {
        self.inner.contains(other.inner)
    }

    fn __or__(&self, other: Self) -> Self {
        Self::from_inner(self.inner | other.inner)
    }

    fn __and__(&self, other: Self) -> Self {
        Self::from_inner(self.inner & other.inner)
    }

    fn __repr__(&self) -> String {
        format!("ComponentMask(bits={:#06b})", self.inner.bits())
    }
}

pub(in crate::python) fn install_component_mask_constants(
    py: Python<'_>,
    module: &Bound<'_, PyModule>,
) -> PyResult<()> {
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
