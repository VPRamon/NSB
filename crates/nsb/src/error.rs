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
/// Failures returned by NSB model construction and evaluation.
pub enum NsbError {
    /// A bundled or caller-provided data file could not be parsed.
    #[error("data parse error in {file}: {message}")]
    DataParse {
        /// Logical data-file name.
        file: &'static str,
        /// Parse failure detail.
        message: String,
    },

    /// Required scientific data were unavailable.
    #[error("required data missing: {file}: {message}")]
    DataMissing {
        /// Logical data-file name.
        file: &'static str,
        /// Missing-data detail.
        message: String,
    },

    /// A starlight map failed schema or value validation.
    #[error("invalid starlight map: {message}")]
    InvalidMap {
        /// Validation failure detail.
        message: String,
    },

    /// An input lies outside the supported numeric/domain range.
    #[error("input out of range: {0}")]
    OutOfRange(String),

    /// The selected model configuration is not evaluable.
    #[error("unsupported configuration: {0}")]
    Unsupported(String),

    /// An upstream ephemeris computation failed.
    #[error("ephemeris error: {0}")]
    Ephemeris(String),

    /// A table interpolation failed.
    #[error("interpolation error: {0}")]
    Interpolation(String),

    /// A named site identifier was unknown.
    #[error("unknown site: {0}")]
    UnknownSite(String),

    /// Filesystem input/output failure.
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
}

/// Crate result alias.
pub type Result<T> = std::result::Result<T, NsbError>;
