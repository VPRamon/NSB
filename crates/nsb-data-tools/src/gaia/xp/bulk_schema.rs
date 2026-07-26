//! Bulk ECSV schema inspection and cross-prefix consistency checks.

use crate::gaia::xp::canonical::{
    packed_correlation_len, stream_bulk_ecsv_gz, CanonicalXpContinuousRecord,
};
use anyhow::{bail, Result};
use md5::{Digest, Md5};
use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct BulkEcsvSchemaReport {
    pub file_name: String,
    pub header_columns: Vec<String>,
    pub header_column_count: usize,
    pub sample_rows_inspected: usize,
    pub bp_n_parameters: Option<usize>,
    pub rp_n_parameters: Option<usize>,
    pub bp_correlation_length: Option<usize>,
    pub rp_correlation_length: Option<usize>,
    pub sample_parse_errors: Vec<String>,
    pub schema_fingerprint: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PrefixSchemaComparison {
    pub left_file: String,
    pub right_file: String,
    pub headers_identical: bool,
    pub bp_n_parameters_match: bool,
    pub rp_n_parameters_match: bool,
    pub correlation_lengths_match: bool,
    pub fingerprints_match: bool,
    pub compatible: bool,
    pub incompatibilities: Vec<String>,
}

pub fn inspect_bulk_ecsv_schema(path: &Path, sample_rows: usize) -> Result<BulkEcsvSchemaReport> {
    let mut stream = stream_bulk_ecsv_gz(path)?;
    let header_columns = stream.header_map().keys().cloned().collect::<Vec<_>>();
    let mut sorted_headers = header_columns.clone();
    sorted_headers.sort();

    let mut bp_n_parameters = None;
    let mut rp_n_parameters = None;
    let mut bp_correlation_length = None;
    let mut rp_correlation_length = None;
    let mut sample_parse_errors = Vec::new();
    let mut inspected = 0_usize;

    while inspected < sample_rows {
        match stream.next_record() {
            Ok(Some(record)) => {
                update_sample_stats(
                    &record,
                    &mut bp_n_parameters,
                    &mut rp_n_parameters,
                    &mut bp_correlation_length,
                    &mut rp_correlation_length,
                );
                inspected += 1;
            }
            Ok(None) => break,
            Err(error) => {
                sample_parse_errors.push(error.to_string());
                break;
            }
        }
    }

    let schema_fingerprint = schema_fingerprint(&sorted_headers, bp_n_parameters, rp_n_parameters);
    Ok(BulkEcsvSchemaReport {
        file_name: path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("unknown")
            .to_string(),
        header_columns: sorted_headers,
        header_column_count: header_columns.len(),
        sample_rows_inspected: inspected,
        bp_n_parameters,
        rp_n_parameters,
        bp_correlation_length,
        rp_correlation_length,
        sample_parse_errors,
        schema_fingerprint,
    })
}

fn update_sample_stats(
    record: &CanonicalXpContinuousRecord,
    bp_n_parameters: &mut Option<usize>,
    rp_n_parameters: &mut Option<usize>,
    bp_correlation_length: &mut Option<usize>,
    rp_correlation_length: &mut Option<usize>,
) {
    *bp_n_parameters = Some(record.bp_n_parameters);
    *rp_n_parameters = Some(record.rp_n_parameters);
    *bp_correlation_length = Some(record.bp_coefficient_correlations.len());
    *rp_correlation_length = Some(record.rp_coefficient_correlations.len());
}

fn schema_fingerprint(
    sorted_headers: &[String],
    bp_n_parameters: Option<usize>,
    rp_n_parameters: Option<usize>,
) -> String {
    let mut hasher = Md5::new();
    for header in sorted_headers {
        hasher.update(header.as_bytes());
        hasher.update([0]);
    }
    if let Some(bp) = bp_n_parameters {
        hasher.update(bp.to_le_bytes());
    }
    if let Some(rp) = rp_n_parameters {
        hasher.update(rp.to_le_bytes());
    }
    format!("{:x}", hasher.finalize())
}

pub fn compare_prefix_schemas(
    left: &BulkEcsvSchemaReport,
    right: &BulkEcsvSchemaReport,
) -> PrefixSchemaComparison {
    let mut incompatibilities = Vec::new();
    let headers_identical = left.header_columns == right.header_columns;
    if !headers_identical {
        incompatibilities.push("header column sets differ".to_string());
    }
    let bp_n_parameters_match = left.bp_n_parameters == right.bp_n_parameters;
    if !bp_n_parameters_match {
        incompatibilities.push(format!(
            "bp_n_parameters differ: {:?} vs {:?}",
            left.bp_n_parameters, right.bp_n_parameters
        ));
    }
    let rp_n_parameters_match = left.rp_n_parameters == right.rp_n_parameters;
    if !rp_n_parameters_match {
        incompatibilities.push(format!(
            "rp_n_parameters differ: {:?} vs {:?}",
            left.rp_n_parameters, right.rp_n_parameters
        ));
    }
    let correlation_lengths_match = left.bp_correlation_length == right.bp_correlation_length
        && left.rp_correlation_length == right.rp_correlation_length;
    if !correlation_lengths_match {
        incompatibilities.push("packed correlation lengths differ".to_string());
    }
    let fingerprints_match = left.schema_fingerprint == right.schema_fingerprint;
    if !fingerprints_match && headers_identical {
        incompatibilities.push("schema fingerprint differs despite matching headers".to_string());
    }
    let compatible = incompatibilities.is_empty()
        && left.sample_parse_errors.is_empty()
        && right.sample_parse_errors.is_empty();
    PrefixSchemaComparison {
        left_file: left.file_name.clone(),
        right_file: right.file_name.clone(),
        headers_identical,
        bp_n_parameters_match,
        rp_n_parameters_match,
        correlation_lengths_match,
        fingerprints_match,
        compatible,
        incompatibilities,
    }
}

pub fn assert_prefix_compatible(left: &Path, right: &Path, sample_rows: usize) -> Result<()> {
    let left_report = inspect_bulk_ecsv_schema(left, sample_rows)?;
    let right_report = inspect_bulk_ecsv_schema(right, sample_rows)?;
    let comparison = compare_prefix_schemas(&left_report, &right_report);
    if !comparison.compatible {
        bail!(
            "bulk prefix schema incompatibility between {} and {}: {}",
            left.display(),
            right.display(),
            comparison.incompatibilities.join("; ")
        );
    }
    Ok(())
}

pub fn expected_correlation_length(n_parameters: usize) -> usize {
    packed_correlation_len(n_parameters)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn expected_correlation_length_matches_gaia_55() {
        assert_eq!(expected_correlation_length(55), 1485);
    }

    #[test]
    fn compare_identical_reports_is_compatible() {
        let report = BulkEcsvSchemaReport {
            file_name: "a.csv.gz".to_string(),
            header_columns: vec!["source_id".to_string()],
            header_column_count: 1,
            sample_rows_inspected: 1,
            bp_n_parameters: Some(55),
            rp_n_parameters: Some(55),
            bp_correlation_length: Some(1485),
            rp_correlation_length: Some(1485),
            sample_parse_errors: vec![],
            schema_fingerprint: "abc".to_string(),
        };
        let comparison = compare_prefix_schemas(&report, &report);
        assert!(comparison.compatible);
    }

    #[test]
    fn inspect_pilot_prefix_when_available() -> Result<()> {
        let path = PathBuf::from(
            "/path/to/nsb-data/starlight-gaia-release/pilot-xp-continuous-bulk/bulk/XpContinuousMeanSpectrum_000000-003111.csv.gz",
        );
        if !path.is_file() {
            return Ok(());
        }
        let report = inspect_bulk_ecsv_schema(&path, 3)?;
        assert_eq!(report.bp_n_parameters, Some(55));
        assert_eq!(report.bp_correlation_length, Some(1485));
        Ok(())
    }

    #[test]
    fn two_pilot_prefixes_are_compatible_when_available() -> Result<()> {
        let left = PathBuf::from(
            "/path/to/nsb-data/starlight-gaia-release/pilot-xp-continuous-bulk/bulk/XpContinuousMeanSpectrum_000000-003111.csv.gz",
        );
        let right = PathBuf::from(
            "/path/to/nsb-data/starlight-gaia-release/pilot-xp-continuous-bulk/bulk/XpContinuousMeanSpectrum_003112-005263.csv.gz",
        );
        if !left.is_file() || !right.is_file() {
            return Ok(());
        }
        let left_report = inspect_bulk_ecsv_schema(&left, 8)?;
        let right_report = inspect_bulk_ecsv_schema(&right, 8)?;
        let comparison = compare_prefix_schemas(&left_report, &right_report);
        assert!(comparison.compatible);
        Ok(())
    }
}
