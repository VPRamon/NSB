use pyo3::prelude::*;

use crate::{AirglowModel, ComponentMask, MoonlightModel, SiteProfileId, ZodiacalExtinction};

macro_rules! selector_methods {
    ($type:ty, $python_name:literal) => {
        #[pymethods]
        impl $type {
            #[pyo3(name = "as_str")]
            fn py_as_str(&self) -> &'static str {
                <$type>::as_str(*self)
            }

            fn __repr__(&self) -> String {
                format!("{}('{}')", $python_name, <$type>::as_str(*self))
            }
        }
    };
}

selector_methods!(SiteProfileId, "SiteProfile");
selector_methods!(MoonlightModel, "MoonlightModel");
selector_methods!(AirglowModel, "AirglowModel");
selector_methods!(ZodiacalExtinction, "ZodiacalExtinction");

#[pymethods]
impl ComponentMask {
    #[getter(bits)]
    fn py_bits(&self) -> u8 {
        self.bits()
    }

    #[pyo3(name = "contains")]
    fn py_contains(&self, other: Self) -> bool {
        self.contains(other)
    }

    fn __or__(&self, other: Self) -> Self {
        *self | other
    }

    fn __and__(&self, other: Self) -> Self {
        *self & other
    }

    fn __repr__(&self) -> String {
        format!("ComponentMask(bits={:#06b})", self.bits())
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
        ty.setattr(name, Py::new(py, value)?)?;
    }
    Ok(())
}
