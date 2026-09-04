//! Historical base selection for the SemVer gate.
//!
//! The gate must never compare `HEAD..HEAD`. On GitHub Actions `push` to
//! `main`, `origin/main` already points at the newly checked-out tip, so CI
//! must pass `github.event.before` explicitly rather than inferring from
//! `origin/main`.

use thiserror::Error;

/// Inputs that describe how a SemVer base was supplied.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BaseInput {
    /// Revision from `--base` / `NSB_PUBLIC_API_BASE` when present.
    pub explicit_base: Option<String>,
    /// Resolved `HEAD` commit id (full SHA).
    pub head_sha: String,
    /// Whether `explicit_base` (when `Some`) was intentionally provided,
    /// including the empty string / all-zero bootstrap sentinel.
    pub explicit: bool,
}

/// Outcome of base resolution before snapshot existence is considered.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BaseDecision {
    /// No usable historical revision (root commit / all-zero push before).
    BootstrapNoBase,
    /// Use this revision for historical comparison (may still bootstrap if
    /// the snapshot file is absent at that revision).
    UseBase { rev: String },
}

/// Final SemVer mode after checking whether `$BASE` contains the snapshot.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HistoricalMode {
    /// Snapshot match only; no `cargo public-api diff`.
    Bootstrap { reason: &'static str },
    /// Run `cargo public-api diff $BASE..HEAD --deny=removed --deny=changed`.
    Diff { base: String },
}

/// Fail-closed base resolution errors.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum BaseError {
    /// Explicit empty `--base` / env value.
    #[error("SemVer gate: explicit empty base rejected (fail closed)")]
    ExplicitEmpty,
    /// Explicit revision does not resolve to a commit.
    #[error("SemVer gate: historical base revision does not exist: {0}")]
    MissingCommit(String),
    /// Base resolves to the same commit as HEAD (empty historical range).
    #[error("SemVer gate: base resolves to HEAD ({0}); refusing empty historical comparison")]
    BaseIsHead(String),
}

/// True for GitHub's all-zero `before` SHA on initial branch pushes.
pub fn is_null_sha(value: &str) -> bool {
    !value.is_empty() && value.chars().all(|c| c == '0')
}

/// Choose a local fallback base when CI did not pass `--base`.
///
/// Prefer merge-base with `origin/main` when it differs from `HEAD`, then
/// `origin/main` if it differs from `HEAD`, then `HEAD~1`. Never returns a
/// revision equal to `head_sha`.
pub fn resolve_local_base_candidate(
    head_sha: &str,
    origin_main_sha: Option<&str>,
    merge_base_with_origin_main: Option<&str>,
    head_parent_sha: Option<&str>,
) -> Option<String> {
    if let Some(mb) = merge_base_with_origin_main {
        if mb != head_sha {
            return Some(mb.to_string());
        }
    }
    if let Some(main) = origin_main_sha {
        if main != head_sha {
            return Some(main.to_string());
        }
    }
    if let Some(parent) = head_parent_sha {
        if parent != head_sha {
            return Some(parent.to_string());
        }
    }
    None
}

/// Resolve the historical base, failing closed on empty `HEAD..HEAD` ranges.
pub fn decide_base(
    input: &BaseInput,
    resolve_commit: impl Fn(&str) -> Option<String>,
) -> Result<BaseDecision, BaseError> {
    if input.explicit {
        let Some(raw) = input.explicit_base.as_deref() else {
            return Err(BaseError::ExplicitEmpty);
        };
        if raw.is_empty() {
            return Err(BaseError::ExplicitEmpty);
        }
        if is_null_sha(raw) {
            return Ok(BaseDecision::BootstrapNoBase);
        }
        let Some(resolved) = resolve_commit(raw) else {
            return Err(BaseError::MissingCommit(raw.to_string()));
        };
        if resolved == input.head_sha {
            return Err(BaseError::BaseIsHead(raw.to_string()));
        }
        return Ok(BaseDecision::UseBase { rev: resolved });
    }

    match input.explicit_base.as_deref() {
        Some(raw) if !raw.is_empty() && !is_null_sha(raw) => {
            let Some(resolved) = resolve_commit(raw) else {
                return Err(BaseError::MissingCommit(raw.to_string()));
            };
            if resolved == input.head_sha {
                return Err(BaseError::BaseIsHead(raw.to_string()));
            }
            Ok(BaseDecision::UseBase { rev: resolved })
        }
        Some(raw) if is_null_sha(raw) => Ok(BaseDecision::BootstrapNoBase),
        _ => Ok(BaseDecision::BootstrapNoBase),
    }
}

/// Map a base decision + snapshot presence onto the historical SemVer mode.
pub fn decide_historical_mode(
    decision: BaseDecision,
    snapshot_exists_at_base: bool,
) -> HistoricalMode {
    match decision {
        BaseDecision::BootstrapNoBase => HistoricalMode::Bootstrap {
            reason: "no usable historical revision; bootstrap mode (snapshot match only)",
        },
        BaseDecision::UseBase { rev } if snapshot_exists_at_base => {
            HistoricalMode::Diff { base: rev }
        }
        BaseDecision::UseBase { .. } => HistoricalMode::Bootstrap {
            reason: "no snapshot at historical base; bootstrap mode (snapshot match only)",
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn head() -> String {
        "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".into()
    }

    fn parent() -> String {
        "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb".into()
    }

    #[test]
    fn null_sha_detection() {
        assert!(is_null_sha("0000000000000000000000000000000000000000"));
        assert!(is_null_sha("0"));
        assert!(!is_null_sha(""));
        assert!(!is_null_sha(&head()));
    }

    #[test]
    fn push_before_sha_selects_previous_main_revision() {
        let before = parent();
        let input = BaseInput {
            explicit_base: Some(before.clone()),
            head_sha: head(),
            explicit: true,
        };
        let decision = decide_base(&input, |rev| {
            if rev == before {
                Some(before.clone())
            } else {
                None
            }
        })
        .unwrap();
        assert_eq!(
            decision,
            BaseDecision::UseBase {
                rev: before.clone()
            }
        );
        assert_eq!(
            decide_historical_mode(decision, true),
            HistoricalMode::Diff { base: before }
        );
    }

    #[test]
    fn push_before_equal_to_head_is_rejected() {
        let input = BaseInput {
            explicit_base: Some(head()),
            head_sha: head(),
            explicit: true,
        };
        let err = decide_base(&input, |rev| Some(rev.to_string())).unwrap_err();
        assert_eq!(err, BaseError::BaseIsHead(head()));
    }

    #[test]
    fn origin_main_equal_to_head_must_not_be_used_as_base() {
        // Simulates a direct push checkout where origin/main already == HEAD.
        let candidate =
            resolve_local_base_candidate(&head(), Some(&head()), Some(&head()), Some(&parent()));
        assert_eq!(candidate.as_deref(), Some(parent().as_str()));
    }

    #[test]
    fn pr_base_sha_is_used_when_explicit() {
        let base = parent();
        let input = BaseInput {
            explicit_base: Some("origin/main".into()),
            head_sha: head(),
            explicit: true,
        };
        let decision = decide_base(&input, |rev| {
            if rev == "origin/main" {
                Some(base.clone())
            } else {
                None
            }
        })
        .unwrap();
        assert_eq!(decision, BaseDecision::UseBase { rev: base });
    }

    #[test]
    fn explicit_empty_base_fails_closed() {
        let input = BaseInput {
            explicit_base: Some(String::new()),
            head_sha: head(),
            explicit: true,
        };
        assert_eq!(
            decide_base(&input, |_| None).unwrap_err(),
            BaseError::ExplicitEmpty
        );
    }

    #[test]
    fn missing_explicit_commit_fails_closed() {
        let input = BaseInput {
            explicit_base: Some("deadbeef".into()),
            head_sha: head(),
            explicit: true,
        };
        assert_eq!(
            decide_base(&input, |_| None).unwrap_err(),
            BaseError::MissingCommit("deadbeef".into())
        );
    }

    #[test]
    fn null_before_sha_bootstraps() {
        let input = BaseInput {
            explicit_base: Some("0000000000000000000000000000000000000000".into()),
            head_sha: head(),
            explicit: true,
        };
        let decision = decide_base(&input, |_| None).unwrap();
        assert_eq!(decision, BaseDecision::BootstrapNoBase);
        assert!(matches!(
            decide_historical_mode(decision, false),
            HistoricalMode::Bootstrap { .. }
        ));
    }

    #[test]
    fn historical_commit_without_snapshot_bootstraps() {
        let decision = BaseDecision::UseBase { rev: parent() };
        assert!(matches!(
            decide_historical_mode(decision, false),
            HistoricalMode::Bootstrap { .. }
        ));
    }

    #[test]
    fn historical_commit_with_snapshot_runs_diff() {
        let decision = BaseDecision::UseBase { rev: parent() };
        assert_eq!(
            decide_historical_mode(decision, true),
            HistoricalMode::Diff { base: parent() }
        );
    }

    #[test]
    fn local_fallback_prefers_merge_base_over_origin_main_tip() {
        let mb = "cccccccccccccccccccccccccccccccccccccccc";
        let main = "dddddddddddddddddddddddddddddddddddddddd";
        let candidate =
            resolve_local_base_candidate(&head(), Some(main), Some(mb), Some(&parent()));
        assert_eq!(candidate.as_deref(), Some(mb));
    }
}
