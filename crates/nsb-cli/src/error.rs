use thiserror::Error;

#[derive(Debug, Error)]
pub enum CliError {
    #[error("provide either --site or --lon --lat --height")]
    InvalidObserver,
    #[error("unknown site alias {0:?}")]
    UnknownSite(String),
    #[error("unknown component {0:?}")]
    UnknownComponent(String),
    #[error("invalid component selection: {0}")]
    InvalidComponentSelection(String),
    #[error("--max-nsb must be finite and non-negative")]
    InvalidMaxNsb,
    #[error("--min-nsb must be finite and non-negative")]
    InvalidMinNsb,
    #[error("--min-nsb must be less than or equal to --max-nsb")]
    InvalidNsbRange,
}
