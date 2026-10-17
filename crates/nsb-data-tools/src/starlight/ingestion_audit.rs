// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Vallés Puig, Ramon
//! Optional raw-to-canonical XP audit using the normal partition executor.
use super::{config::StarlightConfig, sources::acquisition, xp};
use crate::dataset::{Artifact, ValidationGate};
use crate::platform::{artifact_store::atomic_write, checksum_io};
use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, fs, path::Path};

pub const NAME: &str = "gaia_xp_ingestion_audit.json";
#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Shard {
    schema: String,
    partition: String,
    input_sha256: String,
    raw_rows: u64,
    canonical_rows: u64,
    rejected: Vec<xp::canonical::RejectedXpRecord>,
}
fn artifact(path: &Path, name: &str) -> Result<Artifact> {
    Ok(Artifact {
        name: name.into(),
        path: path.into(),
        sha256: checksum_io::sha256_file(path)?,
        bytes: path.metadata()?.len(),
    })
}
pub(crate) fn build(
    root: &Path,
    config: &StarlightConfig,
    partitions: &[String],
) -> Result<Vec<Artifact>> {
    let worker = root
        .parent()
        .and_then(Path::file_name)
        .is_some_and(|p| p == "workers");
    let shared = if worker {
        root.parent()
            .and_then(Path::parent)
            .context("audit worker root")?
    } else {
        root
    };
    let mut artifacts = Vec::new();
    for partition in partitions {
        let input = acquisition::verified_object_for_partition(
            shared,
            &config.gaia_products,
            "xp-continuous",
            partition,
        )?;
        let mut stream = xp::stream_bulk_ecsv_gz(&input)?;
        let mut canonical_rows = 0;
        while stream.next_record()?.is_some() {
            canonical_rows += 1;
        }
        let shard = Shard {
            schema: "nsb-xp-ingestion-shard-v1".into(),
            partition: partition.clone(),
            input_sha256: checksum_io::sha256_file(&input)?,
            raw_rows: stream.raw_rows(),
            canonical_rows,
            rejected: stream.rejected_records().to_vec(),
        };
        if shard.raw_rows != shard.canonical_rows + shard.rejected.len() as u64 {
            bail!("XP row audit accounting failed");
        }
        let path = if worker {
            root.join("shard.json")
        } else {
            root.join("workers").join(partition).join("shard.json")
        };
        atomic_write(&path, &serde_json::to_vec_pretty(&shard)?)?;
        artifacts.push(artifact(&path, &format!("shards/{partition}.json"))?);
    }
    super::worker::write_artifact_index(root, &artifacts)?;
    Ok(artifacts)
}
pub(crate) fn finalize(
    root: &Path,
    config: &StarlightConfig,
    expected: &[String],
) -> Result<Vec<Artifact>> {
    let expected_rows = config
        .ingestion_audit
        .as_ref()
        .context("XP audit configuration")?
        .expected_rows;
    let mut raw = 0;
    let mut canonical = 0;
    let mut rejected = Vec::new();
    let mut shards = BTreeMap::new();
    for partition in expected {
        let path = root
            .join("outputs/shards")
            .join(format!("{partition}.json"));
        let shard: Shard = serde_json::from_slice(&fs::read(&path)?)?;
        if shard.schema != "nsb-xp-ingestion-shard-v1"
            || shard.partition != *partition
            || shard.raw_rows != shard.canonical_rows + shard.rejected.len() as u64
        {
            bail!("XP ingestion shard identity/accounting mismatch");
        }
        raw += shard.raw_rows;
        canonical += shard.canonical_rows;
        for rejection in shard.rejected {
            rejected.push(serde_json::json!({"partition":partition,"source_id":rejection.source_id,"reason":rejection.reason}));
        }
        shards.insert(partition,serde_json::json!({"shard_sha256":checksum_io::sha256_file(&path)?,"input_sha256":shard.input_sha256}));
    }
    let report = serde_json::json!({"schema":"nsb-xp-ingestion-audit-v1","expected_rows":expected_rows,
        "raw_rows":raw,"canonical_rows":canonical,"rejected_rows":rejected.len(),"rejections":rejected,
        "reference_count_matches":raw==expected_rows,"row_accounting":raw==canonical+rejected.len() as u64,
        "partitions":shards,"processed_partitions":expected.len()});
    let path = root.join("outputs").join(NAME);
    atomic_write(&path, &serde_json::to_vec_pretty(&report)?)?;
    let artifacts = vec![artifact(&path, NAME)?];
    super::worker::write_artifact_index(root, &artifacts)?;
    Ok(artifacts)
}
pub(crate) fn validate(path: &Path) -> Result<()> {
    let report: serde_json::Value = serde_json::from_slice(&fs::read(path)?)?;
    if report["schema"] != "nsb-xp-ingestion-audit-v1" || report["row_accounting"] != true {
        bail!("invalid XP ingestion audit");
    }
    Ok(())
}
pub(crate) fn gates(root: &Path) -> Result<Vec<ValidationGate>> {
    let path = root.join("outputs").join(NAME);
    validate(&path)?;
    let report: serde_json::Value = serde_json::from_slice(&fs::read(path)?)?;
    Ok(vec![ValidationGate {
        name: "xp-ingestion-reference-count".into(),
        passed: report["reference_count_matches"] == true,
        detail: format!(
            "raw={}; canonical={}; rejected={}; reference={}",
            report["raw_rows"],
            report["canonical_rows"],
            report["rejected_rows"],
            report["expected_rows"]
        ),
    }])
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dataset::DatasetPipeline;

    #[test]
    fn audit_pipeline_keeps_reference_count_distinct_from_row_accounting() {
        let root = tempfile::tempdir().unwrap();
        let mut config: StarlightConfig =
            toml::from_str("[ingestion_audit]\nexpected_rows=2\n").unwrap();
        let dir = root.path().join("outputs/shards");
        fs::create_dir_all(&dir).unwrap();
        let shard = Shard {
            schema: "nsb-xp-ingestion-shard-v1".into(),
            partition: "a".into(),
            input_sha256: "a".repeat(64),
            raw_rows: 2,
            canonical_rows: 2,
            rejected: vec![],
        };
        fs::write(dir.join("a.json"), serde_json::to_vec(&shard).unwrap()).unwrap();
        let artifacts = finalize(root.path(), &config, &["a".into()]).unwrap();
        assert!(gates(root.path()).unwrap()[0].passed);
        let mut run: crate::dataset::RunConfig = toml::from_str(
            "schema_version=1\ndataset='starlight'\n[workspace]\nroot='/tmp/unused'\n",
        )
        .unwrap();
        run.workspace.root = root.path().to_owned();
        run.starlight = Some(config.clone());
        let pipeline = &super::super::pipeline::PIPELINE;
        assert_eq!(pipeline.expected_outputs_for(&run), vec![NAME]);
        pipeline
            .validate_artifact(NAME, &artifacts[0].path)
            .unwrap();
        assert!(pipeline.validation_gates(&run, &artifacts).unwrap()[0].passed);
        assert!(pipeline.build(&run, &[]).unwrap().unwrap().is_empty());
        config.ingestion_audit.as_mut().unwrap().expected_rows = 3;
        finalize(root.path(), &config, &["a".into()]).unwrap();
        // Complete accounting does not establish equality to a preregistered population.
        validate(&artifacts[0].path).unwrap();
        assert!(!gates(root.path()).unwrap()[0].passed);
        fs::write(&artifacts[0].path, b"{}").unwrap();
        assert!(validate(&artifacts[0].path).is_err());
        let mut wrong = shard;
        wrong.partition = "wrong".into();
        fs::write(dir.join("a.json"), serde_json::to_vec(&wrong).unwrap()).unwrap();
        assert!(finalize(root.path(), &config, &["a".into()]).is_err());
    }
}
