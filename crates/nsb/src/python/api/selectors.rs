use pyo3::prelude::*;

use crate::site::{AtmosphericConditions, SiteProfile, SiteProfileTag};
use crate::{
    AirglowModel, ComponentMask, GenericClearSky, MoonlightModel, NsbModelConfig,
    ZodiacalExtinction,
};
use siderust::qtty::{Hectopascals, Kilometers};

/// Python-layer selector for the application presets exposed by the binding.
///
/// CTAO variants intentionally live at this dynamic binding boundary rather
/// than in the generic Rust public API. Each value constructs a typed profile
/// before the configuration erases its type internally.
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

    pub(crate) fn apply(self, config: NsbModelConfig) -> NsbModelConfig {
        match self {
            Self::GenericClearSky => {
                config.with_site_profile(SiteProfile::<GenericClearSky>::generic_clear_sky())
            }
            Self::CtaNorth => config.with_site_profile(ctao_north_planning()),
            Self::CtaSouth => config.with_site_profile(ctao_south_planning()),
        }
    }

    pub(crate) fn from_name(name: &str) -> Option<Self> {
        match name {
            "generic-clear-sky" => Some(Self::GenericClearSky),
            "ctao-north-planning" => Some(Self::CtaNorth),
            "ctao-south-planning" => Some(Self::CtaSouth),
            _ => None,
        }
    }
}

struct PythonCtaNorth;

impl SiteProfileTag for PythonCtaNorth {
    const NAME: &'static str = "ctao-north-planning";
}

struct PythonCtaSouth;

impl SiteProfileTag for PythonCtaSouth {
    const NAME: &'static str = "ctao-south-planning";
}

fn ctao_north_planning() -> SiteProfile<PythonCtaNorth> {
    SiteProfile::<PythonCtaNorth>::planning(
        Kilometers::new(2.2),
        AtmosphericConditions::clear_sky_with_pressure(Hectopascals::new(770.0))
            .expect("CTAO-North pressure is physical"),
        concat!(
            "CTAO-North planning preset: representative ORM/La Palma ",
            "altitude, fixed planning pressure, Siderust default Rayleigh scale height, ",
            "and bundled Paranal-like clear-sky Mie parameterization. This is not ",
            "yet a validated CTA-N aerosol calibration and does not identify the ",
            "observer as ORM."
        ),
    )
    .expect("Python CTAO-North profile is valid")
}

fn ctao_south_planning() -> SiteProfile<PythonCtaSouth> {
    SiteProfile::<PythonCtaSouth>::planning(
        Kilometers::new(2.1),
        AtmosphericConditions::paranal_average(),
        concat!(
            "CTAO-South planning preset: Paranal-like atmosphere from Siderust ",
            "AtmosphereProfile::EL_PARANAL used as a planning assumption. This is ",
            "not yet a dedicated CTA-S aerosol calibration and does not identify ",
            "the observer as Paranal."
        ),
    )
    .expect("Python CTAO-South profile is valid")
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
            let config = selector.apply(NsbModelConfig::generic_clear_sky());
            assert_eq!(config.site_profile_name(), selector.as_str());
            assert_eq!(
                config.airglow_calibration_status(),
                CalibrationStatus::PlanningPreset
            );
            assert!(!config.is_airglow_site_calibrated());
            assert_eq!(
                PySiteProfile::from_name(config.site_profile_name()),
                Some(selector)
            );
        }
    }
}
