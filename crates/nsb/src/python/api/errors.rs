use pyo3::create_exception;
use pyo3::exceptions::PyException;
use pyo3::prelude::*;

use crate::NsbError as RustNsbError;

create_exception!(
    nsb,
    NsbError,
    PyException,
    "Base exception for NSB Python bindings."
);
create_exception!(
    nsb,
    DataParseError,
    NsbError,
    "A scientific data source could not be parsed."
);
create_exception!(
    nsb,
    DataMissingError,
    NsbError,
    "Required scientific data are unavailable."
);
create_exception!(
    nsb,
    InvalidMapError,
    NsbError,
    "A starlight map failed validation."
);
create_exception!(
    nsb,
    OutOfRangeError,
    NsbError,
    "An input lies outside the supported range."
);
create_exception!(
    nsb,
    UnsupportedError,
    NsbError,
    "The selected NSB configuration is unsupported."
);
create_exception!(
    nsb,
    InterpolationError,
    NsbError,
    "A scientific interpolation failed."
);
create_exception!(
    nsb,
    IoError,
    NsbError,
    "An NSB filesystem operation failed."
);

pub(super) fn add_exceptions(module: &Bound<'_, PyModule>) -> PyResult<()> {
    let py = module.py();
    module.add("NsbError", py.get_type::<NsbError>())?;
    module.add("DataParseError", py.get_type::<DataParseError>())?;
    module.add("DataMissingError", py.get_type::<DataMissingError>())?;
    module.add("InvalidMapError", py.get_type::<InvalidMapError>())?;
    module.add("OutOfRangeError", py.get_type::<OutOfRangeError>())?;
    module.add("UnsupportedError", py.get_type::<UnsupportedError>())?;
    module.add("InterpolationError", py.get_type::<InterpolationError>())?;
    module.add("IoError", py.get_type::<IoError>())?;
    Ok(())
}

pub(super) fn to_py_err(error: RustNsbError) -> PyErr {
    let message = error.to_string();
    match error {
        RustNsbError::DataParse { .. } => DataParseError::new_err(message),
        RustNsbError::DataMissing { .. } => DataMissingError::new_err(message),
        RustNsbError::InvalidMap { .. } => InvalidMapError::new_err(message),
        RustNsbError::OutOfRange(_) => OutOfRangeError::new_err(message),
        RustNsbError::Unsupported(_) => UnsupportedError::new_err(message),
        RustNsbError::Interpolation(_) => InterpolationError::new_err(message),
        RustNsbError::Io(_) => IoError::new_err(message),
    }
}

pub(super) fn invalid_input(message: impl Into<String>) -> PyErr {
    OutOfRangeError::new_err(message.into())
}
