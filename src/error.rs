//! Error type for the NSB crate.
//!
//! Scientific role:
//! scientific code is only as trustworthy as its failure modes. This module
//! defines the explicit ways the NSB calculation can fail: malformed bundled
//! reference data, invalid geometry/ranges, unsupported model requests, or
//! upstream ephemeris/interpolation issues.
//!
//! Contribution to the science:
//! by separating parse, range, interpolation, and ephemeris failures, this
//! file helps users distinguish between "the sky model says the answer is X"
//! and "the model could not be evaluated reliably for this input or dataset."

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

    #[error("unknown site: {0}")]
    UnknownSite(String),

    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
}

pub type Result<T> = std::result::Result<T, NsbError>;
