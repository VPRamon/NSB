use anyhow::Result;
use nsb_data_tools::platform::checksum_io::{Checksum, ChecksumAlgorithm};
use nsb_data_tools::platform::pipeline::{
    AdmissionDecision, CacheInputState, DiagnosticSample, Gate, GateStatus, PartitionCheckpoint,
    PartitionCompletion, PartitionState, ProcessingMode, ProductionAdmission, ResumeAction,
    RowSelection, TransitionEvidence, MAX_DIAGNOSTIC_SAMPLES,
};

fn md5(byte: char) -> Result<Checksum> {
    Checksum::new(ChecksumAlgorithm::Md5, byte.to_string().repeat(32))
}

fn sha256(byte: char) -> Result<Checksum> {
    Checksum::new(ChecksumAlgorithm::Sha256, byte.to_string().repeat(64))
}

fn checksum_verified_state() -> Result<PartitionState> {
    let checksum = md5('a')?;
    let mut state = PartitionState::planned("XpContinuousMeanSpectrum_000000-003111.csv.gz")?;
    state.transition(
        CacheInputState::Downloading,
        TransitionEvidence::DownloadStarted,
    )?;
    state.transition(
        CacheInputState::Downloaded,
        TransitionEvidence::DownloadCompleted { bytes: 4096 },
    )?;
    state.transition(
        CacheInputState::ChecksumVerified,
        TransitionEvidence::ChecksumMatched {
            expected: checksum.clone(),
            actual: checksum,
        },
    )?;
    Ok(state)
}

#[test]
fn full_partition_mode_is_explicit_and_zero_is_rejected() {
    assert_eq!(RowSelection::FullPartition.limit(), None);
    assert_eq!(RowSelection::first_rows(12).unwrap().limit(), Some(12));
    assert!(RowSelection::first_rows(0).is_err());
}

#[test]
fn skipped_required_gate_never_counts_as_passed() -> Result<()> {
    let report = ProductionAdmission::new(
        vec![
            Gate::new("storage", true, GateStatus::Passed)?,
            Gate::new(
                "resume_validation",
                true,
                GateStatus::NotRun("explicitly skipped".to_string()),
            )?,
        ],
        Vec::new(),
    );
    let decision = report.evaluate()?;
    assert!(!decision.is_ready());
    assert_eq!(decision.exit_code(), 2);
    assert!(decision
        .blockers()
        .iter()
        .any(|value| value.contains("resume_validation")));
    Ok(())
}

#[test]
fn every_explicit_blocker_produces_a_failing_exit_code() -> Result<()> {
    let report = ProductionAdmission::new(Vec::new(), vec!["inventory unavailable".to_string()]);
    assert!(matches!(report.evaluate()?, AdmissionDecision::Blocked(_)));
    assert_eq!(report.evaluate()?.exit_code(), 2);
    Ok(())
}

#[test]
fn checksum_mismatch_is_rejected_without_state_mutation() -> Result<()> {
    let mut state = PartitionState::planned("partition.csv.gz")?;
    state.transition(
        CacheInputState::Downloading,
        TransitionEvidence::DownloadStarted,
    )?;
    state.transition(
        CacheInputState::Downloaded,
        TransitionEvidence::DownloadCompleted { bytes: 10 },
    )?;
    let before = state.clone();
    assert!(state
        .transition(
            CacheInputState::ChecksumVerified,
            TransitionEvidence::ChecksumMatched {
                expected: md5('a')?,
                actual: md5('b')?,
            },
        )
        .is_err());
    assert_eq!(state, before);
    Ok(())
}

#[test]
fn pilot_or_partial_processing_can_never_become_releasable() -> Result<()> {
    let mut state = checksum_verified_state()?;
    state.transition(
        CacheInputState::Processing,
        TransitionEvidence::ProcessingStarted {
            mode: ProcessingMode::Pilot,
            row_selection: RowSelection::first_rows(100)?,
        },
    )?;
    state.transition(
        CacheInputState::Processed,
        TransitionEvidence::ProcessingCompleted {
            completion: PartitionCompletion::Partial {
                rows_processed: 100,
            },
        },
    )?;
    state.transition(
        CacheInputState::OutputVerified,
        TransitionEvidence::OutputVerified {
            checksum: sha256('b')?,
        },
    )?;
    state.transition(
        CacheInputState::Reconciled,
        TransitionEvidence::Reconciled {
            manifest_checksum: sha256('c')?,
        },
    )?;
    let before = state.clone();
    assert!(state
        .transition(
            CacheInputState::Releasable,
            TransitionEvidence::ReleaseAuthorized,
        )
        .is_err());
    assert_eq!(state, before);
    Ok(())
}

#[test]
fn complete_production_evidence_can_authorize_release() -> Result<()> {
    let mut state = checksum_verified_state()?;
    state.transition(
        CacheInputState::Processing,
        TransitionEvidence::ProcessingStarted {
            mode: ProcessingMode::Production,
            row_selection: RowSelection::FullPartition,
        },
    )?;
    state.transition(
        CacheInputState::Processed,
        TransitionEvidence::ProcessingCompleted {
            completion: PartitionCompletion::Complete { rows_processed: 42 },
        },
    )?;
    state.transition(
        CacheInputState::OutputVerified,
        TransitionEvidence::OutputVerified {
            checksum: sha256('b')?,
        },
    )?;
    state.transition(
        CacheInputState::Reconciled,
        TransitionEvidence::Reconciled {
            manifest_checksum: sha256('c')?,
        },
    )?;
    state.transition(
        CacheInputState::Releasable,
        TransitionEvidence::ReleaseAuthorized,
    )?;
    assert_eq!(state.state, CacheInputState::Releasable);
    assert_eq!(state.resume_action(), ResumeAction::CleanupOptional);
    Ok(())
}

#[test]
fn resume_semantics_cover_every_persisted_state() -> Result<()> {
    let planned = PartitionState::planned("partition.csv.gz")?;
    assert_eq!(planned.resume_action(), ResumeAction::Acquire);

    let mut downloading = planned.clone();
    downloading.transition(
        CacheInputState::Downloading,
        TransitionEvidence::DownloadStarted,
    )?;
    assert_eq!(downloading.resume_action(), ResumeAction::Acquire);

    let mut downloaded = downloading.clone();
    downloaded.transition(
        CacheInputState::Downloaded,
        TransitionEvidence::DownloadCompleted { bytes: 1 },
    )?;
    assert_eq!(
        downloaded.resume_action(),
        ResumeAction::VerifyInputChecksum
    );

    let mut verified = downloaded.clone();
    verified.transition(
        CacheInputState::ChecksumVerified,
        TransitionEvidence::ChecksumMatched {
            expected: md5('a')?,
            actual: md5('a')?,
        },
    )?;
    assert_eq!(verified.resume_action(), ResumeAction::Process);

    let mut processing = verified.clone();
    processing.transition(
        CacheInputState::Processing,
        TransitionEvidence::ProcessingStarted {
            mode: ProcessingMode::Production,
            row_selection: RowSelection::FullPartition,
        },
    )?;
    assert_eq!(processing.resume_action(), ResumeAction::Process);

    let mut processed = processing.clone();
    processed.transition(
        CacheInputState::Processed,
        TransitionEvidence::ProcessingCompleted {
            completion: PartitionCompletion::Complete { rows_processed: 1 },
        },
    )?;
    assert_eq!(processed.resume_action(), ResumeAction::VerifyOutput);

    let mut output_verified = processed.clone();
    output_verified.transition(
        CacheInputState::OutputVerified,
        TransitionEvidence::OutputVerified {
            checksum: sha256('b')?,
        },
    )?;
    assert_eq!(output_verified.resume_action(), ResumeAction::Reconcile);

    let mut reconciled = output_verified.clone();
    reconciled.transition(
        CacheInputState::Reconciled,
        TransitionEvidence::Reconciled {
            manifest_checksum: sha256('c')?,
        },
    )?;
    assert_eq!(reconciled.resume_action(), ResumeAction::AuthorizeRelease);

    let mut releasable = reconciled.clone();
    releasable.transition(
        CacheInputState::Releasable,
        TransitionEvidence::ReleaseAuthorized,
    )?;
    assert_eq!(releasable.resume_action(), ResumeAction::CleanupOptional);

    let mut deleted = releasable.clone();
    deleted.transition(CacheInputState::Deleted, TransitionEvidence::Deleted)?;
    assert_eq!(deleted.resume_action(), ResumeAction::Complete);

    let mut failed = processing;
    failed.transition(
        CacheInputState::Failed,
        TransitionEvidence::Failed {
            reason: "interrupted".to_string(),
        },
    )?;
    assert_eq!(failed.resume_action(), ResumeAction::InspectFailure);
    Ok(())
}

#[test]
fn production_checkpoint_growth_is_bounded() -> Result<()> {
    let mut checkpoint = PartitionCheckpoint::new(
        "partition.csv.gz",
        ProcessingMode::Production,
        RowSelection::FullPartition,
    )?;
    let samples = (0..100)
        .map(|index| DiagnosticSample::new(index, "parse", format!("failure {index}")))
        .collect::<Result<Vec<_>>>()?;
    checkpoint.record_batch(100, 0, 0, 100, 100, samples)?;
    assert_eq!(checkpoint.diagnostics.len(), MAX_DIAGNOSTIC_SAMPLES);
    assert!(serde_json::to_vec(&checkpoint)?.len() < 16 * 1024);

    checkpoint.record_batch(184_729_270, 184_729_270, 0, 0, 184_729_370, Vec::new())?;
    assert_eq!(checkpoint.diagnostics.len(), MAX_DIAGNOSTIC_SAMPLES);
    assert!(serde_json::to_vec(&checkpoint)?.len() < 16 * 1024);
    Ok(())
}

#[test]
fn strict_checkpoint_schema_rejects_unknown_fields() {
    let json = r#"{
        "schema_version":1,
        "partition_id":"p",
        "mode":"production",
        "row_selection":{"kind":"full_partition"},
        "next_row_offset":0,
        "rows_scanned":0,
        "rows_valid":0,
        "rows_excluded":0,
        "rows_failed":0,
        "rolling_input_checksum":null,
        "healpix_checksum":null,
        "diagnostics":[],
        "unexpected":true
    }"#;
    assert!(serde_json::from_str::<PartitionCheckpoint>(json).is_err());
}
