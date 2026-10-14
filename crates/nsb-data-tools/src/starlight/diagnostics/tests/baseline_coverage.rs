// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Vallés Puig, Ramon
//! Offline baseline-partition and shard-loading regression tests.

use super::*;
use crate::starlight::map::accumulator::PartitionShard;
use std::fs;

#[test]
fn smoke_partition_reader_trims_comments_and_preserves_partition_order() -> Result<()> {
    let repo = tempfile::tempdir()?;
    let path = repo.path().join(SMOKE_PARTITIONS_PATH);
    fs::create_dir_all(path.parent().unwrap())?;
    fs::write(
        &path,
        " # pinned partitions\n\n 00-19 \r\n# skipped\n20-29\n \n 30-39 \n",
    )?;
    assert_eq!(
        load_smoke_partitions(repo.path())?,
        vec!["00-19", "20-29", "30-39"]
    );
    fs::write(&path, "\n  # comments only\n")?;
    assert!(load_smoke_partitions(repo.path())?.is_empty());
    Ok(())
}

#[test]
fn baseline_shard_loader_fails_closed_on_missing_and_malformed_partitions() -> Result<()> {
    let directory = tempfile::tempdir()?;
    assert!(load_workspace_shards(directory.path(), &[])?.is_empty());
    let partitions = vec!["part-a".to_string()];
    assert!(load_workspace_shards(directory.path(), &partitions)
        .unwrap_err()
        .to_string()
        .contains("read shard"));

    let shard_path = directory.path().join("workers/part-a/shard.json");
    fs::create_dir_all(shard_path.parent().unwrap())?;
    fs::write(&shard_path, b"malformed")?;
    assert!(load_workspace_shards(directory.path(), &partitions).is_err());

    let shard = PartitionShard::new("part-a", 1)?;
    shard.write(&shard_path)?;
    let loaded = load_workspace_shards(directory.path(), &partitions)?;
    assert_eq!(loaded.len(), 1);
    assert_eq!(loaded[0].partition_id, "part-a");
    Ok(())
}

#[test]
fn baseline_report_fails_on_missing_or_invalid_configuration() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let workspace = directory.path().join("workspace");
    let config = directory.path().join("config.toml");
    let output = directory.path().join("baseline.json");
    assert!(write_baseline_report(
        directory.path(),
        &config,
        &workspace,
        "commit",
        &output,
        &[],
    )
    .is_err());
    fs::write(&config, b"not valid = = toml")?;
    assert!(write_baseline_report(
        directory.path(),
        &config,
        &workspace,
        "commit",
        &output,
        &[],
    )
    .is_err());
    assert!(!output.exists());
    Ok(())
}


#[test]
fn complete_baseline_report_roundtrips_a_synthetic_sparse_workspace() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let repo = directory.path().join("repository");
    let partitions_path = repo.join(SMOKE_PARTITIONS_PATH);
    fs::create_dir_all(partitions_path.parent().unwrap())?;
    fs::write(&partitions_path, b"00-19\n20-29\n")?;
    let config = directory.path().join("starlight.toml");
    fs::write(
        &config,
        include_str!("../../../../config/starlight-production.toml"),
    )?;
    let workspace = directory.path().join("workspace");
    for (name, reason, excluded) in [
        ("00-19", "invalid-spectrum", 2_u64),
        ("20-29", "invalid-spectrum", 3_u64),
    ] {
        let mut shard = PartitionShard::new(name, 2)?;
        shard.exclusion_reasons.insert(reason.into(), excluded);
        let shard_path = workspace.join("workers").join(name).join("shard.json");
        shard.write(&shard_path)?;
    }
    let output = directory.path().join("reports/phase0.json");
    let report = write_baseline_report(
        &repo, &config, &workspace, "synthetic-commit", &output, &[13, 24],
    )?;
    assert_eq!(report.phase, "phase0_baseline");
    assert_eq!(report.commit, "synthetic-commit");
    assert_eq!(report.partition_count, 2);
    assert_eq!(report.partitions, ["00-19", "20-29"]);
    assert_eq!(report.full_sky_anomalous_parents_reference, vec![13, 24]);
    assert_eq!(report.flux_metrics.total_flux_ph_m2_s, 0.0);
    assert_eq!(report.flux_metrics.admitted_sources, 0);
    assert_eq!(report.flux_metrics.excluded_sources, 0);
    assert_eq!(report.global_exclusion_reasons["invalid-spectrum"], 5);
    assert_eq!(
        report.config_sha256,
        crate::platform::checksum_io::sha256_file(&config)?,
    );
    assert_eq!(serde_json::from_slice::<BaselineReport>(&fs::read(output)?)?, report);
    Ok(())
}
