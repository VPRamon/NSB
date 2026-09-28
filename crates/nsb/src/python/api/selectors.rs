use pyo3::prelude::*;

use crate::site::{AtmosphericConditions, SiteProfileSpec};
use crate::{AirglowModel, ComponentMask, MoonlightModel, ZodiacalExtinction};
use siderust::qtty::{Hectopascals, Kilometers};

/// Python-layer selector for the application presets exposed by the binding.
///
/// CTAO variants intentionally live here rather than in the generic Rust
/// `SiteProfileId` contract. Each value resolves to an ordinary
/// `SiteProfileSpec`.
#[pyclass(
    name = "SiteProfile",
    eq,
    frozen,
    rename_all = "SCREAMING_SNAKE_CASE",
    module = "nsb",
    from_py_object
)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::python) enum PySiteProfile {
    GenericClearSky,
    CtaNorth,
    CtaSouth,
}

impl PySiteProfile {
    pub(crate) const fn as_str(self) -> &'static str {
        match self {
            Self::GenericClearSky => "generic-clear-sky",
            Self::CtaNorth => "ctao-north-planning",
            Self::CtaSouth => "ctao-south-planning",
        }
    }

    pub(crate) fn to_spec(self) -> SiteProfileSpec {
        match self {
            Self::GenericClearSky => SiteProfileSpec::generic_clear_sky(),
            Self::CtaNorth => SiteProfileSpec::planning(
                "ctao-north-planning",
                "ctao-north-planning",
                Kilometers::new(2.2),
                AtmosphericConditions::clear_sky_with_pressure(Hectopascals::new(770.0)),
                concat!(
                    "CTAO-North planning preset: representative ORM/La Palma ",
                    "altitude, fixed planning pressure, Siderust default ",
                    "Rayleigh scale height, and bundled Paranal-like clear-sky ",
                    "Mie parameterization. This is not yet a validated CTA-N ",
                    "aerosol calibration and does not identify the observer as ORM."
                ),
            ),
            Self::CtaSouth => SiteProfileSpec::planning(
                "ctao-south-planning",
                "ctao-south-planning",
                Kilometers::new(2.1),
                AtmosphericConditions::paranal_average(),
                concat!(
                    "CTAO-South planning preset: Paranal-like atmosphere from ",
                    "Siderust AtmosphereProfile::EL_PARANAL used as a planning ",
                    "assumption. This is not yet a dedicated CTA-S aerosol ",
                    "calibration and does not identify the observer as Paranal."
                ),
            ),
        }
    }

    pub(crate) fn from_spec(spec: &SiteProfileSpec) -> Option<Self> {
        match spec.id().as_str() {
            "generic-clear-sky" => Some(Self::GenericClearSky),
            "ctao-north-planning" => Some(Self::CtaNorth),
            "ctao-south-planning" => Some(Self::CtaSouth),
            _ => None,
        }
    }
}

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

selector_methods!(PySiteProfile, "SiteProfile");
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::site::CalibrationStatus;

    #[test]
    fn python_ctao_selectors_resolve_to_planning_specs() {
        for selector in [PySiteProfile::CtaNorth, PySiteProfile::CtaSouth] {
            let spec = selector.to_spec();
            assert_eq!(spec.id().as_str(), selector.as_str());
            assert_eq!(spec.calibration_status(), CalibrationStatus::PlanningPreset);
            assert!(!spec.is_site_calibrated());
            assert_eq!(PySiteProfile::from_spec(&spec), Some(selector));
        }
    }
}
