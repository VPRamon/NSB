//! Public API snapshot integrity and historical SemVer gates for crate `nsb`.
//!
//! CI invokes the binary with an explicit `--base` revision:
//! - `pull_request`: PR base SHA
//! - `push`: `github.event.before` (revision before the push)
//!
//! Updating `crates/nsb/api/public-api.txt` in the same commit as a breaking
//! removal cannot hide the break when `$BASE` already contains a snapshot.

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
};
pub use snapshot::{DEFAULT_NIGHTLY, DEFAULT_PUBLIC_API_VERSION, DEFAULT_SNAPSHOT_PATH};
