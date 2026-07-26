use anyhow::Result;
use nsb_data_tools::platform::checksum_io::{Checksum, ChecksumAlgorithm};
use nsb_data_tools::platform::pipeline::{
    read_partition_state, write_partition_state, CacheInputState, PartitionCompletion,
    PartitionManifest, PartitionState, ProcessingMode, ReconciliationManifest, ResumeAction,
    RowSelection, TransitionEvidence, PIPELINE_SCHEMA_VERSION,
};
use std::fs;

fn md5(byte: char) -> Result<Checksum> {
    Checksum::new(ChecksumAlgorithm::Md5, byte.to_string().repeat(32))
}

fn sha256(byte: char) -> Result<Checksum> {
    Checksum::new(ChecksumAlgorithm::Sha256, byte.to_string().repeat(64))
}

fn manifest(id: &str, rows_valid: u64, rows_excluded: u64) -> Result<PartitionManifest> {
    let rows_scanned = rows_valid + rows_excluded;
    Ok(PartitionManifest {
        schema_version: PIPELINE_SCHEMA_VERSION,
        partition_id: id.to_string(),
        input_checksum: md5('a')?,
        output_checksum: sha256('b')?,
        healpix_checksum: sha256('c')?,
        processing_mode: ProcessingMode::Production,
        row_selection: RowSelection::FullPartition,
        completion: PartitionCompletion::Complete {
            rows_processed: rows_scanned,
        },
        rows_scanned,
        rows_valid,
        rows_excluded,
        rows_failed: 0,
    })
}

fn persist_and_assert(
    path: &std::path::Path,
    state: &PartitionState,
    expected_action: ResumeAction,
) -> Result<()> {
    write_partition_state(path, state)?;
    let restored = read_partition_state(path)?;
    assert_eq!(&restored, state);
    assert_eq!(restored.resume_action(), expected_action);
    Ok(())
}

#[test]
fn reconciliation_is_deterministic_across_processing_order() -> Result<()> {
    let first = manifest("partition-b", 7, 1)?;
    let second = manifest("partition-a", 5, 2)?;
    let left = ReconciliationManifest::from_partitions(vec![first.clone(), second.clone()])?;
    let right = ReconciliationManifest::from_partitions(vec![second, first])?;
    assert_eq!(left, right);
    assert_eq!(left.canonical_json()?, right.canonical_json()?);
    assert_eq!(left.rows_scanned, 15);
    assert_eq!(left.rows_valid, 12);
    assert_eq!(left.rows_excluded, 3);
    Ok(())
}

#[test]
fn reconciliation_rejects_duplicate_partial_and_inconsistent_partitions() -> Result<()> {
    let duplicate = manifest("partition-a", 2, 0)?;
    assert!(ReconciliationManifest::from_partitions(vec![duplicate.clone(), duplicate]).is_err());

    let mut partial = manifest("partition-b", 2, 0)?;
    partial.completion = PartitionCompletion::Partial { rows_processed: 2 };
    assert!(ReconciliationManifest::from_partitions(vec![partial]).is_err());

    let mut inconsistent = manifest("partition-c", 2, 0)?;
    inconsistent.rows_scanned = 3;
    assert!(ReconciliationManifest::from_partitions(vec![inconsistent]).is_err());
    Ok(())
}

#[test]
fn reconciliation_round_trip_is_strict_and_corruption_fails_closed() -> Result<()> {
    let dir = tempfile::tempdir()?;
    let path = dir.path().join("reconciliation.json");
    let manifest = ReconciliationManifest::from_partitions(vec![
        manifest("partition-a", 3, 1)?,
        manifest("partition-b", 4, 0)?,
    ])?;
    manifest.write(&path)?;
    assert_eq!(ReconciliationManifest::read(&path)?, manifest);

    let mut value: serde_json::Value = serde_json::from_slice(&fs::read(&path)?)?;
    value["rows_valid"] = serde_json::json!(999);
    fs::write(&path, serde_json::to_vec_pretty(&value)?)?;
    assert!(ReconciliationManifest::read(&path).is_err());

    value["rows_valid"] = serde_json::json!(7);
    value["unexpected"] = serde_json::json!(true);
    fs::write(&path, serde_json::to_vec_pretty(&value)?)?;
    assert!(ReconciliationManifest::read(&path).is_err());
    Ok(())
}

#[test]
fn every_durable_boundary_round_trips_with_the_correct_resume_action() -> Result<()> {
    let dir = tempfile::tempdir()?;
    let path = dir.path().join("partition-state.json");
    let mut state = PartitionState::planned("partition-a")?;
    persist_and_assert(&path, &state, ResumeAction::Acquire)?;

    state.transition(
        CacheInputState::Downloading,
        TransitionEvidence::DownloadStarted,
    )?;
    persist_and_assert(&path, &state, ResumeAction::Acquire)?;

    state.transition(
        CacheInputState::Downloaded,
        TransitionEvidence::DownloadCompleted { bytes: 4096 },
    )?;
    persist_and_assert(&path, &state, ResumeAction::VerifyInputChecksum)?;

    state.transition(
        CacheInputState::ChecksumVerified,
        TransitionEvidence::ChecksumMatched {
            expected: md5('a')?,
            actual: md5('a')?,
        },
    )?;
    persist_and_assert(&path, &state, ResumeAction::Process)?;

    state.transition(
        CacheInputState::Processing,
        TransitionEvidence::ProcessingStarted {
            mode: ProcessingMode::Production,
            row_selection: RowSelection::FullPartition,
        },
    )?;
    persist_and_assert(&path, &state, ResumeAction::Process)?;

    state.transition(
        CacheInputState::Processed,
        TransitionEvidence::ProcessingCompleted {
            completion: PartitionCompletion::Complete { rows_processed: 8 },
        },
    )?;
    persist_and_assert(&path, &state, ResumeAction::VerifyOutput)?;

    state.transition(
        CacheInputState::OutputVerified,
        TransitionEvidence::OutputVerified {
            checksum: sha256('b')?,
        },
    )?;
    persist_and_assert(&path, &state, ResumeAction::Reconcile)?;

    state.transition(
        CacheInputState::Reconciled,
        TransitionEvidence::Reconciled {
            manifest_checksum: sha256('c')?,
        },
    )?;
    persist_and_assert(&path, &state, ResumeAction::AuthorizeRelease)?;

    state.transition(
        CacheInputState::Releasable,
        TransitionEvidence::ReleaseAuthorized,
    )?;
    persist_and_assert(&path, &state, ResumeAction::CleanupOptional)?;

    state.transition(CacheInputState::Deleted, TransitionEvidence::Deleted)?;
    persist_and_assert(&path, &state, ResumeAction::Complete)?;
    Ok(())
}

#[test]
fn corrupted_or_unknown_partition_state_fails_closed() -> Result<()> {
    let dir = tempfile::tempdir()?;
    let path = dir.path().join("partition-state.json");
    let state = PartitionState::planned("partition-a")?;
    write_partition_state(&path, &state)?;

    let mut value: serde_json::Value = serde_json::from_slice(&fs::read(&path)?)?;
    value["schema_version"] = serde_json::json!(999);
    fs::write(&path, serde_json::to_vec_pretty(&value)?)?;
    assert!(read_partition_state(&path).is_err());

    value["schema_version"] = serde_json::json!(PIPELINE_SCHEMA_VERSION);
    value["unexpected"] = serde_json::json!(true);
    fs::write(&path, serde_json::to_vec_pretty(&value)?)?;
    assert!(read_partition_state(&path).is_err());
    Ok(())
}
