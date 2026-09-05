use nsb_public_api_gate::{run_check, CheckOptions, GateStatus, HistoricalMode};
use std::fs;
use std::time::{SystemTime, UNIX_EPOCH};

fn temporary_repo() -> std::path::PathBuf {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock")
        .as_nanos();
    std::env::temp_dir().join(format!(
        "nsb-public-api-gate-prefreeze-{}-{nonce}",
        std::process::id()
    ))
}

#[test]
fn pre_freeze_check_does_not_require_snapshot_or_semver_base() {
    let repo = temporary_repo();
    fs::create_dir_all(repo.join("crates/nsb/src")).expect("create temporary repo");
    fs::write(repo.join("crates/nsb/src/lib.rs"), "pub fn api_can_change() {}\n")
        .expect("write temporary source");

    let outcome = run_check(&CheckOptions {
        repo: repo.clone(),
        write: false,
        base: None,
        base_explicit: false,
    })
    .expect("pre-freeze policy must pass without a snapshot");

    assert_eq!(outcome.status, GateStatus::Pass);
    assert!(outcome.message.contains("pre-freeze"));
    assert!(matches!(
        outcome.historical,
        Some(HistoricalMode::Bootstrap { .. })
    ));
    assert!(!repo.join("crates/nsb/api/public-api.txt").exists());

    fs::remove_dir_all(repo).expect("remove temporary repo");
}
