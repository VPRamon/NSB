//! Error type for the NSB crate.

use thiserror::Error;

#[derive(Debug, Error)]
pub enum NsbError {
    #[error("data parse error in {file}: {message}")]
    DataParse { file: &'static str, message: String },

    #[error("input out of range: {0}")]
    OutOfRange(String),

    #[error("unsupported configuration: {0}")]
    Unsupported(String),

    #[error("ephemeris error: {0}")]
    Ephemeris(String),

    #[error("interpolation error: {0}")]
    Interpolation(String),

    #[error("unknown source: {0}")]
    UnknownSource(String),

    #[error("unknown site: {0}")]
    UnknownSite(String),

    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
}

pub type Result<T> = std::result::Result<T, NsbError>;
