//! Public API policy gate for crate `nsb`.
//!
//! The crate is currently pre-freeze: API signatures may still change while
//! the first release surface is being corrected. In that state the gate keeps
//! compatibility-only symbols from reappearing but does not require a public
//! API snapshot or historical SemVer compatibility.
//!
//! Freezing is explicit: add `crates/nsb/api/API_FROZEN` together with a
//! generated `crates/nsb/api/public-api.txt`. The freeze commit bootstraps that
//! baseline; later commits enforce snapshot equality and historical
//! `cargo-public-api` diffs against frozen bases.

mod base;
mod compat;
mod git;
mod run;
mod snapshot;

pub use base::{
    decide_base, decide_historical_mode, is_null_sha, resolve_local_base_candidate, BaseDecision,
    BaseError, BaseInput, HistoricalMode,
};
pub use run::{
    discover_repo, run_check, run_write, CheckOptions, GateError, GateOutcome, GateStatus,
    PUBLIC_API_FREEZE_MARKER,
};
pub use snapshot::{DEFAULT_NIGHTLY, DEFAULT_PUBLIC_API_VERSION, DEFAULT_SNAPSHOT_PATH};
