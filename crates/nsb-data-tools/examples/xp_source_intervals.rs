// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Vallés Puig, Ramon
//! Evaluate a small checksum-pinned DataLink source set with the existing XP integrator.
//! Source files and outputs remain private; this example contains no catalogue records.
use anyhow::{bail, Context, Result};
use nsb_data_tools::platform::checksum_io;
use nsb_data_tools::starlight::xp::canonical::parse_datalink_gaiaxpy_csv;
use nsb_data_tools::starlight::xp::GaiaXpContinuousCalibrator;
use serde::Deserialize;
use std::io::Write;
use std::path::Path;

#[derive(Deserialize)]
struct InputFile {
    source_id: String,
    sha256: String,
}

#[derive(Deserialize)]
struct Manifest {
    files: Vec<InputFile>,
}

fn main() -> Result<()> {
    run(&std::env::args().collect::<Vec<_>>())
}

fn run(args: &[String]) -> Result<()> {
    if args.len() != 7 {
        bail!(
            "usage: xp_source_intervals DIRECTORY MANIFEST MANIFEST_SHA DESIGN DESIGN_SHA OUTPUT"
        );
    }
    for (path, expected) in [(&args[2], &args[3]), (&args[4], &args[5])] {
        if checksum_io::sha256_file(Path::new(path))? != *expected {
            bail!("pinned input checksum mismatch");
        }
    }
    let manifest: Manifest = serde_json::from_slice(&std::fs::read(&args[2])?)?;
    if manifest.files.is_empty() {
        bail!("empty source manifest");
    }
    let calibrator = GaiaXpContinuousCalibrator::from_design_fixture(Path::new(&args[4]))?;
    let intervals = [
        [360.0, 570.0],
        [360.0, 390.0],
        [390.0, 420.0],
        [420.0, 450.0],
        [450.0, 480.0],
        [480.0, 510.0],
        [510.0, 540.0],
        [540.0, 570.0],
    ];
    let integrator = calibrator.bp_integrator(&intervals)?;
    let mut rows = Vec::new();
    let mut seen = std::collections::HashSet::new();
    let mut files = manifest.files;
    // Canonical decimal IDs also prevent distinct spellings of the same source.
    for entry in &files {
        let id: u64 = entry
            .source_id
            .parse()
            .context("source ID must be numeric")?;
        if id == 0 || id.to_string() != entry.source_id {
            bail!("source ID must be a positive canonical decimal u64");
        }
        if !seen.insert(id) {
            bail!("duplicate source ID");
        }
    }
    files.sort_by(|a, b| {
        a.source_id
            .len()
            .cmp(&b.source_id.len())
            .then(a.source_id.cmp(&b.source_id))
    });
    for entry in files {
        let path = Path::new(&args[1]).join(format!("{}.csv", entry.source_id));
        if checksum_io::sha256_file(&path)? != entry.sha256 {
            bail!("source checksum mismatch");
        }
        let record = parse_datalink_gaiaxpy_csv(&std::fs::read(path)?, &entry.source_id)?;
        let (flux, covariance) = integrator.integrate(&record)?;
        rows.push(serde_json::json!({"source_id":entry.source_id,
            "input_sha256":entry.sha256,"flux_ph_m2_s":flux,
            "statistical_covariance":covariance,
            "positive_all_intervals":flux.iter().all(|f| *f>0.0)}));
    }
    let output = Path::new(&args[6]);
    let report = serde_json::json!({"schema":"nsb-xp-source-intervals-v1",
        "intervals_nm":intervals,"manifest_sha256":args[3],"design_sha256":args[5],
        "sources":rows,"units":"photon/(m2 s)",
        "covariance_units":"photon^2/(m4 s2)",
        "population_selection_applied":false,
        "systematic_uncertainty":null,
        "covariance":"full within-source BP coefficient covariance; no between-source covariance"});
    publish(output, &serde_json::to_vec_pretty(&report)?)?;
    Ok(())
}

fn publish(output: &Path, bytes: &[u8]) -> Result<()> {
    let parent = output
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    let mut staged =
        tempfile::NamedTempFile::new_in(parent).context("cannot stage immutable output")?;
    staged
        .write_all(bytes)
        .context("cannot write staged output")?;
    staged
        .as_file()
        .sync_all()
        .context("cannot sync staged output")?;
    staged
        .persist_noclobber(output)
        .map_err(|e| e.error)
        .context("cannot publish new immutable output")?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use nsb_data_tools::starlight::xp::canonical::{
        write_gaiaxpy_datalink_csv, CanonicalXpContinuousRecord,
    };
    use std::sync::{Arc, Barrier};

    fn fixture(root: &Path, ids: &[&str]) -> Vec<String> {
        let mut record: serde_json::Value = serde_json::from_str(include_str!(
            "../tests/fixtures/gaiaxpy_oracle/record-01.json"
        ))
        .unwrap();
        record.as_object_mut().unwrap().remove("split");
        record.as_object_mut().unwrap().remove("expected");
        record.as_object_mut().unwrap().remove("oracle");
        record.as_object_mut().unwrap().remove("correlations");
        record["bp_coefficient_correlations"] = serde_json::json!(vec![0.0; 1485]);
        record["rp_coefficient_correlations"] = serde_json::json!(vec![0.0; 1485]);
        for arm in ["bp", "rp"] {
            record[format!("{arm}_n_parameters")] = record[format!("{arm}_coefficients")]
                .as_array()
                .unwrap()
                .len()
                .into();
        }
        record["schema_version"] =
            nsb_data_tools::starlight::xp::canonical::CANONICAL_XP_CONTINUOUS_SCHEMA.into();
        record["source_format"] = "data_link".into();
        record["source_checksum"] = serde_json::Value::Null;
        record["quality_flags"] = serde_json::json!([]);
        let mut files = Vec::new();
        for id in ids {
            let mut value = record.clone();
            value["source_id"] = (*id).into();
            let record: CanonicalXpContinuousRecord = serde_json::from_value(value).unwrap();
            let path = root.join(format!("{id}.csv"));
            write_gaiaxpy_datalink_csv(&path, &record).unwrap();
            files.push(serde_json::json!({"source_id":id,"sha256":checksum_io::sha256_file(&path).unwrap()}));
        }
        let manifest = root.join("manifest.json");
        std::fs::write(
            &manifest,
            serde_json::to_vec(&serde_json::json!({"files":files})).unwrap(),
        )
        .unwrap();
        let design = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/gaiaxpy_continuous_design_v375wi_v142r.json");
        vec![
            "xp_source_intervals".into(),
            root.to_str().unwrap().into(),
            manifest.to_str().unwrap().into(),
            checksum_io::sha256_file(&manifest).unwrap(),
            design.to_str().unwrap().into(),
            checksum_io::sha256_file(&design).unwrap(),
            root.join("output.json").to_str().unwrap().into(),
        ]
    }

    #[test]
    fn concurrent_publication_preserves_one_complete_immutable_report() {
        let root = tempfile::tempdir().unwrap();
        let output = Arc::new(root.path().join("report.json"));
        let barrier = Arc::new(Barrier::new(8));
        let writers = (0..8)
            .map(|id| {
                let (output, barrier) = (Arc::clone(&output), Arc::clone(&barrier));
                std::thread::spawn(move || {
                    let bytes = format!("{{\"writer\":{id},\"payload\":\"{}\"}}", "x".repeat(4096));
                    barrier.wait();
                    publish(&output, bytes.as_bytes()).map(|_| bytes)
                })
            })
            .collect::<Vec<_>>();
        let winners = writers
            .into_iter()
            .filter_map(|t| t.join().unwrap().ok())
            .collect::<Vec<_>>();
        assert_eq!(winners.len(), 1);
        assert_eq!(std::fs::read_to_string(&*output).unwrap(), winners[0]);
        assert!(publish(&output, b"replacement").is_err());
        assert_eq!(std::fs::read_to_string(&*output).unwrap(), winners[0]);
        assert_eq!(std::fs::read_dir(root.path()).unwrap().count(), 1);
    }

    #[test]
    fn source_report_uses_canonical_covariance_integrator_and_numeric_order() {
        let root = tempfile::tempdir().unwrap();
        let args = fixture(root.path(), &["10", "2"]);
        run(&args).unwrap();
        let bytes = std::fs::read(&args[6]).unwrap();
        let report: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(report["sources"][0]["source_id"], "2");
        assert_eq!(report["sources"][1]["source_id"], "10");
        assert_eq!(report["units"], "photon/(m2 s)");
        assert!(report["systematic_uncertainty"].is_null());
        let flux = report["sources"][0]["flux_ph_m2_s"].as_array().unwrap();
        assert_eq!(flux.len(), 8);
        let parts: f64 = flux[1..].iter().map(|v| v.as_f64().unwrap()).sum();
        assert!((flux[0].as_f64().unwrap() - parts).abs() < 1e-10 * parts.abs());
        assert_eq!(
            report["sources"][0]["statistical_covariance"]
                .as_array()
                .unwrap()
                .len(),
            8
        );
        assert!(run(&args).is_err());
        assert_eq!(std::fs::read(&args[6]).unwrap(), bytes);
    }

    #[test]
    fn invalid_inputs_and_output_failures_never_publish_a_report() {
        let root = tempfile::tempdir().unwrap();
        let args = fixture(root.path(), &["2"]);
        assert!(run(&args[..1]).is_err());
        for index in [3, 5] {
            let mut bad = args.clone();
            bad[index] = "0".repeat(64);
            assert!(run(&bad).is_err());
            assert!(!Path::new(&args[6]).exists());
        }
        let mut bad = args.clone();
        bad[6] = root
            .path()
            .join("missing/output.json")
            .to_str()
            .unwrap()
            .into();
        assert!(run(&bad).is_err());
        assert!(!Path::new(&bad[6]).exists());
        bad[6] = root.path().to_str().unwrap().into();
        assert!(run(&bad).is_err());
        // Tamper with the source without updating its pin.
        std::fs::write(root.path().join("2.csv"), b"invalid").unwrap();
        assert!(run(&args).is_err());
        assert!(!Path::new(&args[6]).exists());
        assert!(!root.path().read_dir().unwrap().any(|e| e
            .unwrap()
            .file_name()
            .to_string_lossy()
            .starts_with(".tmp")));
    }

    #[test]
    fn invalid_manifest_sources_and_malformed_records_are_rejected() {
        let root = tempfile::tempdir().unwrap();
        let mut args = fixture(root.path(), &["2"]);
        let original: serde_json::Value =
            serde_json::from_slice(&std::fs::read(&args[2]).unwrap()).unwrap();
        let entry = original["files"][0].clone();
        let mut invalid_id = entry.clone();
        invalid_id["source_id"] = "02".into();
        let mut nonnumeric = entry.clone();
        nonnumeric["source_id"] = "../2".into();
        for manifest in [
            serde_json::json!({"files":[]}),
            serde_json::json!({"files":[entry.clone(),entry.clone()]}),
            serde_json::json!({"files":[invalid_id]}),
            serde_json::json!({"files":[nonnumeric]}),
        ] {
            std::fs::write(&args[2], serde_json::to_vec(&manifest).unwrap()).unwrap();
            args[3] = checksum_io::sha256_file(Path::new(&args[2])).unwrap();
            assert!(run(&args).is_err());
            assert!(!Path::new(&args[6]).exists());
        }
        std::fs::write(root.path().join("2.csv"), b"malformed CSV").unwrap();
        let mut malformed = entry;
        malformed["sha256"] = checksum_io::sha256_file(&root.path().join("2.csv"))
            .unwrap()
            .into();
        std::fs::write(
            &args[2],
            serde_json::to_vec(&serde_json::json!({"files":[malformed]})).unwrap(),
        )
        .unwrap();
        args[3] = checksum_io::sha256_file(Path::new(&args[2])).unwrap();
        assert!(run(&args).is_err());
        assert!(!Path::new(&args[6]).exists());
    }
}
