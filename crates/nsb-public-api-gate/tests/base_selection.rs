//! Deterministic base-selection and snapshot-policy contract tests.
//!
//! These tests do not invoke `cargo-public-api`; they prove the gate's
//! SemVer base selection and bootstrap rules that protect direct pushes to
//! `main` and PR comparisons.

use nsb_public_api_gate::{
    decide_base, decide_historical_mode, is_null_sha, resolve_local_base_candidate, BaseDecision,
    BaseError, BaseInput, HistoricalMode,
};

fn sha(label: u8) -> String {
    std::iter::repeat_n(char::from(label), 40).collect()
}

#[test]
fn snapshot_mode_diff_when_historical_baseline_exists() {
    let base = sha(b'b');
    let mode = decide_historical_mode(BaseDecision::UseBase { rev: base.clone() }, true);
    assert_eq!(mode, HistoricalMode::Diff { base });
}

#[test]
fn bootstrap_when_historical_commit_lacks_snapshot() {
    let mode = decide_historical_mode(BaseDecision::UseBase { rev: sha(b'b') }, false);
    assert!(matches!(mode, HistoricalMode::Bootstrap { .. }));
}

#[test]
fn direct_main_push_must_use_before_sha_not_head() {
    let head = sha(b'a');
    let before = sha(b'b');
    // After push, origin/main == HEAD; local inference must fall back to parent.
    assert_eq!(
        resolve_local_base_candidate(&head, Some(&head), Some(&head), Some(&before)).as_deref(),
        Some(before.as_str())
    );
    let input = BaseInput {
        explicit_base: Some(before.clone()),
        head_sha: head,
        explicit: true,
    };
    let decision = decide_base(&input, |rev| Some(rev.to_string())).unwrap();
    assert_eq!(decision, BaseDecision::UseBase { rev: before });
}

#[test]
fn pr_comparison_uses_explicit_base_sha() {
    let head = sha(b'a');
    let pr_base = sha(b'c');
    let input = BaseInput {
        explicit_base: Some(pr_base.clone()),
        head_sha: head,
        explicit: true,
    };
    let decision = decide_base(&input, |rev| {
        if rev == pr_base {
            Some(pr_base.clone())
        } else {
            None
        }
    })
    .unwrap();
    assert_eq!(decision, BaseDecision::UseBase { rev: pr_base });
}

#[test]
fn head_equals_base_is_rejected_even_if_snapshot_updated() {
    // Removal + regenerated snapshot at HEAD still needs a distinct historical
    // base; comparing HEAD..HEAD would hide the break.
    let head = sha(b'a');
    let input = BaseInput {
        explicit_base: Some(head.clone()),
        head_sha: head.clone(),
        explicit: true,
    };
    assert_eq!(
        decide_base(&input, |_| Some(head.clone())).unwrap_err(),
        BaseError::BaseIsHead(head)
    );
}

#[test]
fn null_before_sha_bootstraps_without_diff() {
    assert!(is_null_sha("0000000000000000000000000000000000000000"));
    let input = BaseInput {
        explicit_base: Some("0000000000000000000000000000000000000000".into()),
        head_sha: sha(b'a'),
        explicit: true,
    };
    let decision = decide_base(&input, |_| None).unwrap();
    assert_eq!(decision, BaseDecision::BootstrapNoBase);
    assert!(matches!(
        decide_historical_mode(decision, true),
        HistoricalMode::Bootstrap { .. }
    ));
}

#[test]
fn missing_or_empty_explicit_base_fails_closed() {
    assert_eq!(
        decide_base(
            &BaseInput {
                explicit_base: Some(String::new()),
                head_sha: sha(b'a'),
                explicit: true,
            },
            |_| None
        )
        .unwrap_err(),
        BaseError::ExplicitEmpty
    );
    assert_eq!(
        decide_base(
            &BaseInput {
                explicit_base: Some("missing".into()),
                head_sha: sha(b'a'),
                explicit: true,
            },
            |_| None
        )
        .unwrap_err(),
        BaseError::MissingCommit("missing".into())
    );
}
