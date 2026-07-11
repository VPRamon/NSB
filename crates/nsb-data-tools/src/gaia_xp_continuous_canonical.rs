//! Canonical Gaia DR3 XP continuous coefficient representation.
//!
//! Both official bulk ECSV rows and Gaia DataLink `XP_CONTINUOUS` CSV responses
//! normalize into [`CanonicalXpContinuousRecord`]. GaiaXPy 2.1.4 receives a
//! DataLink-compatible parenthesis-array CSV produced by
//! [`write_gaiaxpy_datalink_csv`].

use anyhow::{bail, Context, Result};
use csv::{ReaderBuilder, StringRecord, WriterBuilder};
use flate2::read::GzDecoder;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs::File;
use std::io::BufReader;
use std::path::{Path, PathBuf};

use crate::gaia_xp::parse_gaia_tuple_array;

/// Current canonical XP continuous schema version.
pub const CANONICAL_XP_CONTINUOUS_SCHEMA: u32 = 2;

/// Gaia packs the upper triangle (excluding diagonal) column-major; length `n(n-1)/2`.
pub const CORRELATION_PACKING: &str = "gaia_dr3_upper_triangle_column_major_excluding_diagonal";

/// Fields required by GaiaXPy 2.1.4 `calibrate` with correlation inputs.
pub const GAIA_XPY_CALIBRATE_COLUMNS: [&str; 11] = [
    "source_id",
    "bp_n_parameters",
    "bp_standard_deviation",
    "rp_n_parameters",
    "rp_standard_deviation",
    "bp_coefficients",
    "bp_coefficient_errors",
    "bp_coefficient_correlations",
    "rp_coefficients",
    "rp_coefficient_errors",
    "rp_coefficient_correlations",
];

/// Origin-specific provenance for one normalized record.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum XpContinuousSourceFormat {
    DataLink,
    BulkEcsv,
}

/// Versioned, origin-independent XP continuous coefficient record.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CanonicalXpContinuousRecord {
    pub schema_version: u32,
    pub source_id: String,
    pub bp_n_parameters: usize,
    pub rp_n_parameters: usize,
    pub bp_n_relevant_bases: Option<u16>,
    pub rp_n_relevant_bases: Option<u16>,
    pub bp_standard_deviation: f64,
    pub rp_standard_deviation: f64,
    pub bp_coefficients: Vec<f64>,
    pub rp_coefficients: Vec<f64>,
    pub bp_coefficient_errors: Vec<f64>,
    pub rp_coefficient_errors: Vec<f64>,
    pub bp_coefficient_correlations: Vec<f64>,
    pub rp_coefficient_correlations: Vec<f64>,
    pub source_format: XpContinuousSourceFormat,
    pub source_checksum: Option<String>,
    pub quality_flags: Vec<String>,
}

impl CanonicalXpContinuousRecord {
    pub fn validate(&self) -> Result<()> {
        if self.schema_version != CANONICAL_XP_CONTINUOUS_SCHEMA {
            bail!(
                "unsupported canonical XP continuous schema {}; expected {CANONICAL_XP_CONTINUOUS_SCHEMA}",
                self.schema_version
            );
        }
        if self.source_id.trim().is_empty() {
            bail!("canonical XP continuous source_id must not be empty");
        }
        self.source_id
            .parse::<u64>()
            .context("source_id must be u64")?;
        validate_band(
            "bp",
            self.bp_n_parameters,
            &self.bp_coefficients,
            &self.bp_coefficient_errors,
            &self.bp_coefficient_correlations,
            self.bp_standard_deviation,
        )?;
        validate_band(
            "rp",
            self.rp_n_parameters,
            &self.rp_coefficients,
            &self.rp_coefficient_errors,
            &self.rp_coefficient_correlations,
            self.rp_standard_deviation,
        )?;
        Ok(())
    }

    pub fn max_abs_diff(&self, other: &Self) -> FieldDiffSummary {
        FieldDiffSummary {
            max_abs_bp_coefficient_diff: max_abs_slice_diff(
                &self.bp_coefficients,
                &other.bp_coefficients,
            ),
            max_abs_rp_coefficient_diff: max_abs_slice_diff(
                &self.rp_coefficients,
                &other.rp_coefficients,
            ),
            max_abs_bp_error_diff: max_abs_slice_diff(
                &self.bp_coefficient_errors,
                &other.bp_coefficient_errors,
            ),
            max_abs_rp_error_diff: max_abs_slice_diff(
                &self.rp_coefficient_errors,
                &other.rp_coefficient_errors,
            ),
            max_abs_bp_correlation_diff: max_abs_slice_diff(
                &self.bp_coefficient_correlations,
                &other.bp_coefficient_correlations,
            ),
            max_abs_rp_correlation_diff: max_abs_slice_diff(
                &self.rp_coefficient_correlations,
                &other.rp_coefficient_correlations,
            ),
            bp_standard_deviation_diff: (self.bp_standard_deviation - other.bp_standard_deviation)
                .abs(),
            rp_standard_deviation_diff: (self.rp_standard_deviation - other.rp_standard_deviation)
                .abs(),
        }
    }
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
pub struct FieldDiffSummary {
    pub max_abs_bp_coefficient_diff: f64,
    pub max_abs_rp_coefficient_diff: f64,
    pub max_abs_bp_error_diff: f64,
    pub max_abs_rp_error_diff: f64,
    pub max_abs_bp_correlation_diff: f64,
    pub max_abs_rp_correlation_diff: f64,
    pub bp_standard_deviation_diff: f64,
    pub rp_standard_deviation_diff: f64,
}

impl FieldDiffSummary {
    pub fn passes_equivalence_gates(&self) -> bool {
        self.max_abs_bp_coefficient_diff <= 1.0e-12
            && self.max_abs_rp_coefficient_diff <= 1.0e-12
            && self.max_abs_bp_error_diff <= 1.0e-12
            && self.max_abs_rp_error_diff <= 1.0e-12
            && self.max_abs_bp_correlation_diff <= 1.0e-10
            && self.max_abs_rp_correlation_diff <= 1.0e-10
            && self.bp_standard_deviation_diff <= 1.0e-12
            && self.rp_standard_deviation_diff <= 1.0e-12
    }
}

pub fn packed_correlation_len(n_parameters: usize) -> usize {
    n_parameters.saturating_mul(n_parameters.saturating_sub(1)) / 2
}

pub fn parse_datalink_gaiaxpy_csv(
    bytes: &[u8],
    expected_source_id: &str,
) -> Result<CanonicalXpContinuousRecord> {
    if bytes.is_empty() {
        bail!("empty XP continuous DataLink response");
    }
    if crate::gaia_xp::contains_service_error(bytes) {
        bail!("XP continuous DataLink response contains SERVICE ERROR");
    }
    let text = String::from_utf8_lossy(bytes);
    if text.trim_start().starts_with('<') {
        bail!("XP continuous DataLink response looks like HTML/XML, not CSV");
    }
    let mut reader = ReaderBuilder::new()
        .comment(Some(b'#'))
        .trim(csv::Trim::All)
        .from_reader(bytes);
    let headers = reader.headers()?.clone();
    let header_map = header_index(&headers);
    let mut rows = reader.records();
    let record = rows
        .next()
        .transpose()
        .context("failed to read XP continuous DataLink row")?
        .ok_or_else(|| anyhow::anyhow!("XP continuous DataLink CSV has no data rows"))?;
    if rows.next().transpose()?.is_some() {
        bail!("XP continuous DataLink CSV must contain exactly one row");
    }
    parse_csv_record(
        &record,
        &header_map,
        expected_source_id,
        XpContinuousSourceFormat::DataLink,
        None,
    )
}

pub fn parse_bulk_ecsv_record(
    record: &StringRecord,
    header_map: &HashMap<String, usize>,
    origin_file: Option<&Path>,
) -> Result<CanonicalXpContinuousRecord> {
    let source_id = field(record, header_map, "source_id")?;
    parse_csv_record(
        record,
        header_map,
        source_id,
        XpContinuousSourceFormat::BulkEcsv,
        origin_file,
    )
}

fn parse_csv_record(
    record: &StringRecord,
    header_map: &HashMap<String, usize>,
    expected_source_id: &str,
    source_format: XpContinuousSourceFormat,
    origin_file: Option<&Path>,
) -> Result<CanonicalXpContinuousRecord> {
    let source_id = field(record, header_map, "source_id")?;
    if source_id != expected_source_id {
        bail!("XP continuous source_id mismatch: expected {expected_source_id}, found {source_id}");
    }
    let sid = source_id.parse::<u64>().ok();
    let bp_n_parameters = parse_usize_field(record, header_map, "bp_n_parameters")?;
    let rp_n_parameters = parse_usize_field(record, header_map, "rp_n_parameters")?;
    let bp_standard_deviation = parse_f64_field(record, header_map, "bp_standard_deviation")?;
    let rp_standard_deviation = parse_f64_field(record, header_map, "rp_standard_deviation")?;
    let bp_coefficients =
        parse_array_field(record, header_map, "bp_coefficients", sid, origin_file)?;
    let rp_coefficients =
        parse_array_field(record, header_map, "rp_coefficients", sid, origin_file)?;
    let bp_coefficient_errors = parse_array_field(
        record,
        header_map,
        "bp_coefficient_errors",
        sid,
        origin_file,
    )?;
    let rp_coefficient_errors = parse_array_field(
        record,
        header_map,
        "rp_coefficient_errors",
        sid,
        origin_file,
    )?;
    let bp_coefficient_correlations = parse_array_field(
        record,
        header_map,
        "bp_coefficient_correlations",
        sid,
        origin_file,
    )?;
    let rp_coefficient_correlations = parse_array_field(
        record,
        header_map,
        "rp_coefficient_correlations",
        sid,
        origin_file,
    )?;
    let bp_n_relevant_bases = optional_u16_field(record, header_map, "bp_n_relevant_bases")?;
    let rp_n_relevant_bases = optional_u16_field(record, header_map, "rp_n_relevant_bases")?;
    let canonical = CanonicalXpContinuousRecord {
        schema_version: CANONICAL_XP_CONTINUOUS_SCHEMA,
        source_id: source_id.to_string(),
        bp_n_parameters,
        rp_n_parameters,
        bp_n_relevant_bases,
        rp_n_relevant_bases,
        bp_standard_deviation,
        rp_standard_deviation,
        bp_coefficients,
        rp_coefficients,
        bp_coefficient_errors,
        rp_coefficient_errors,
        bp_coefficient_correlations,
        rp_coefficient_correlations,
        source_format,
        source_checksum: None,
        quality_flags: Vec::new(),
    };
    canonical.validate().with_context(|| {
        format!("canonical XP continuous validation failed for source {source_id}")
    })?;
    Ok(canonical)
}

pub fn write_gaiaxpy_datalink_csv(path: &Path, record: &CanonicalXpContinuousRecord) -> Result<()> {
    record.validate()?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let part = path.with_extension("csv.part");
    let mut writer = WriterBuilder::new().from_path(&part)?;
    writer.write_record([
        "source_id",
        "bp_n_parameters",
        "bp_standard_deviation",
        "rp_n_parameters",
        "rp_standard_deviation",
        "bp_coefficients",
        "bp_coefficient_errors",
        "bp_coefficient_correlations",
        "rp_coefficients",
        "rp_coefficient_errors",
        "rp_coefficient_correlations",
        "bp_n_relevant_bases",
        "rp_n_relevant_bases",
    ])?;
    writer.write_record([
        record.source_id.clone(),
        record.bp_n_parameters.to_string(),
        format_scalar(record.bp_standard_deviation),
        record.rp_n_parameters.to_string(),
        format_scalar(record.rp_standard_deviation),
        format_parenthesis_array(&record.bp_coefficients),
        format_parenthesis_array(&record.bp_coefficient_errors),
        format_parenthesis_array(&record.bp_coefficient_correlations),
        format_parenthesis_array(&record.rp_coefficients),
        format_parenthesis_array(&record.rp_coefficient_errors),
        format_parenthesis_array(&record.rp_coefficient_correlations),
        record
            .bp_n_relevant_bases
            .map(|v| v.to_string())
            .unwrap_or_default(),
        record
            .rp_n_relevant_bases
            .map(|v| v.to_string())
            .unwrap_or_default(),
    ])?;
    writer.flush()?;
    drop(writer);
    std::fs::rename(part, path)?;
    Ok(())
}

pub fn write_gaiaxpy_datalink_csv_batch(
    path: &Path,
    records: &[CanonicalXpContinuousRecord],
) -> Result<()> {
    if records.is_empty() {
        bail!("cannot write empty GaiaXPy CSV batch");
    }
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let part = path.with_extension("csv.part");
    let mut writer = WriterBuilder::new().from_path(&part)?;
    writer.write_record([
        "source_id",
        "bp_n_parameters",
        "bp_standard_deviation",
        "rp_n_parameters",
        "rp_standard_deviation",
        "bp_coefficients",
        "bp_coefficient_errors",
        "bp_coefficient_correlations",
        "rp_coefficients",
        "rp_coefficient_errors",
        "rp_coefficient_correlations",
        "bp_n_relevant_bases",
        "rp_n_relevant_bases",
    ])?;
    for record in records {
        record.validate()?;
        writer.write_record([
            record.source_id.clone(),
            record.bp_n_parameters.to_string(),
            format_scalar(record.bp_standard_deviation),
            record.rp_n_parameters.to_string(),
            format_scalar(record.rp_standard_deviation),
            format_parenthesis_array(&record.bp_coefficients),
            format_parenthesis_array(&record.bp_coefficient_errors),
            format_parenthesis_array(&record.bp_coefficient_correlations),
            format_parenthesis_array(&record.rp_coefficients),
            format_parenthesis_array(&record.rp_coefficient_errors),
            format_parenthesis_array(&record.rp_coefficient_correlations),
            record
                .bp_n_relevant_bases
                .map(|v| v.to_string())
                .unwrap_or_default(),
            record
                .rp_n_relevant_bases
                .map(|v| v.to_string())
                .unwrap_or_default(),
        ])?;
    }
    writer.flush()?;
    drop(writer);
    std::fs::rename(part, path)?;
    Ok(())
}

/// Default `csv` reader buffer (8 KiB) is too small for Gaia bulk rows (~40 KiB).
const BULK_CSV_BUFFER_CAPACITY: usize = 512 * 1024;

type BulkEcsvCsvReader = csv::Reader<BufReader<GzDecoder<File>>>;

pub fn stream_bulk_ecsv_gz(path: &Path) -> Result<BulkEcsvStream> {
    let file = File::open(path).with_context(|| format!("open bulk ECSV {}", path.display()))?;
    let decoder = GzDecoder::new(file);
    let reader = BufReader::new(decoder);
    let mut csv_reader = ReaderBuilder::new()
        .comment(Some(b'#'))
        .buffer_capacity(BULK_CSV_BUFFER_CAPACITY)
        .from_reader(reader);
    let headers = csv_reader.headers()?.clone();
    let header_map = header_index(&headers);
    Ok(BulkEcsvStream {
        path: path.to_path_buf(),
        header_map,
        csv_reader,
        row_buffer: StringRecord::new(),
        finished: false,
    })
}

pub struct BulkEcsvStream {
    path: PathBuf,
    header_map: HashMap<String, usize>,
    csv_reader: BulkEcsvCsvReader,
    row_buffer: StringRecord,
    finished: bool,
}

impl BulkEcsvStream {
    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn header_map(&self) -> &HashMap<String, usize> {
        &self.header_map
    }

    pub fn next_record(&mut self) -> Result<Option<CanonicalXpContinuousRecord>> {
        if self.finished {
            return Ok(None);
        }
        loop {
            self.row_buffer.clear();
            if !self
                .csv_reader
                .read_record(&mut self.row_buffer)
                .with_context(|| format!("read bulk row from {}", self.path.display()))?
            {
                self.finished = true;
                return Ok(None);
            }
            if self.row_buffer.is_empty() {
                continue;
            }
            let mut parsed =
                parse_bulk_ecsv_record(&self.row_buffer, &self.header_map, Some(&self.path))?;
            parsed.source_checksum = None;
            return Ok(Some(parsed));
        }
    }
}

pub fn find_bulk_source(
    bulk_gz: &Path,
    source_id: &str,
) -> Result<Option<CanonicalXpContinuousRecord>> {
    let wanted = [source_id.to_string()].into_iter().collect();
    Ok(find_bulk_sources(bulk_gz, &wanted)?.remove(source_id))
}

pub fn find_bulk_sources(
    bulk_gz: &Path,
    wanted: &std::collections::HashSet<String>,
) -> Result<HashMap<String, CanonicalXpContinuousRecord>> {
    if wanted.is_empty() {
        return Ok(HashMap::new());
    }
    let mut found = HashMap::with_capacity(wanted.len());
    let mut stream = stream_bulk_ecsv_gz(bulk_gz)?;
    while found.len() < wanted.len() {
        let Some(record) = stream.next_record()? else {
            break;
        };
        if wanted.contains(&record.source_id) {
            found.insert(record.source_id.clone(), record);
        }
    }
    Ok(found)
}

fn header_index(headers: &StringRecord) -> HashMap<String, usize> {
    headers
        .iter()
        .enumerate()
        .map(|(index, name)| (name.to_string(), index))
        .collect()
}

fn field<'a>(
    record: &'a StringRecord,
    header_map: &HashMap<String, usize>,
    name: &str,
) -> Result<&'a str> {
    let index = *header_map
        .get(name)
        .with_context(|| format!("missing column {name}"))?;
    record
        .get(index)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .with_context(|| format!("empty field {name}"))
}

fn parse_usize_field(
    record: &StringRecord,
    header_map: &HashMap<String, usize>,
    name: &str,
) -> Result<usize> {
    field(record, header_map, name)?
        .parse::<usize>()
        .with_context(|| format!("invalid integer field {name}"))
}

fn parse_f64_field(
    record: &StringRecord,
    header_map: &HashMap<String, usize>,
    name: &str,
) -> Result<f64> {
    let value = field(record, header_map, name)?
        .parse::<f64>()
        .with_context(|| format!("invalid float field {name}"))?;
    if !value.is_finite() || value <= 0.0 {
        bail!("field {name} must be finite and positive");
    }
    Ok(value)
}

fn optional_u16_field(
    record: &StringRecord,
    header_map: &HashMap<String, usize>,
    name: &str,
) -> Result<Option<u16>> {
    match header_map.get(name) {
        Some(index) => record
            .get(*index)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(|value| {
                value
                    .parse::<u16>()
                    .with_context(|| format!("invalid optional field {name}"))
            })
            .transpose(),
        None => Ok(None),
    }
}

fn parse_array_field(
    record: &StringRecord,
    header_map: &HashMap<String, usize>,
    name: &str,
    source_id: Option<u64>,
    origin_file: Option<&Path>,
) -> Result<Vec<f64>> {
    parse_gaia_tuple_array(
        field(record, header_map, name)?,
        name,
        source_id,
        origin_file,
    )
    .with_context(|| format!("parse array field {name}"))
}

fn validate_band(
    band: &str,
    n_parameters: usize,
    coefficients: &[f64],
    errors: &[f64],
    correlations: &[f64],
    standard_deviation: f64,
) -> Result<()> {
    if coefficients.len() != n_parameters {
        bail!(
            "{band}: coefficient length {} != n_parameters {n_parameters}",
            coefficients.len()
        );
    }
    if errors.len() != n_parameters {
        bail!(
            "{band}: error length {} != n_parameters {n_parameters}",
            errors.len()
        );
    }
    let expected_corr = packed_correlation_len(n_parameters);
    if correlations.len() != expected_corr {
        bail!(
            "{band}: correlation length {} != expected packed length {expected_corr} for n={n_parameters}",
            correlations.len()
        );
    }
    if !standard_deviation.is_finite() || standard_deviation <= 0.0 {
        bail!("{band}: standard_deviation must be finite and positive");
    }
    for (label, values) in [
        ("coefficients", coefficients),
        ("errors", errors),
        ("correlations", correlations),
    ] {
        if values.iter().any(|value| !value.is_finite()) {
            bail!("{band}: non-finite values in {label}");
        }
    }
    if errors.iter().any(|value| *value < 0.0) {
        bail!("{band}: coefficient errors must be non-negative");
    }
    validate_packed_correlation_range(band, correlations)?;
    Ok(())
}

fn validate_packed_correlation_range(band: &str, correlations: &[f64]) -> Result<()> {
    for (index, value) in correlations.iter().enumerate() {
        if value.abs() > 1.0 + 1.0e-6 {
            bail!("{band}: correlation[{index}]={value} outside [-1,1]");
        }
    }
    Ok(())
}

fn format_parenthesis_array(values: &[f64]) -> String {
    let body = values
        .iter()
        .map(|value| format_scalar(*value))
        .collect::<Vec<_>>()
        .join(", ");
    format!("({body})")
}

fn format_scalar(value: f64) -> String {
    format!("{value:.8e}")
}

fn max_abs_slice_diff(left: &[f64], right: &[f64]) -> f64 {
    if left.len() != right.len() {
        return f64::INFINITY;
    }
    left.iter()
        .zip(right.iter())
        .map(|(a, b)| (a - b).abs())
        .fold(0.0, f64::max)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_record() -> CanonicalXpContinuousRecord {
        let n = 3_usize;
        CanonicalXpContinuousRecord {
            schema_version: CANONICAL_XP_CONTINUOUS_SCHEMA,
            source_id: "42".to_string(),
            bp_n_parameters: n,
            rp_n_parameters: n,
            bp_n_relevant_bases: Some(3),
            rp_n_relevant_bases: Some(3),
            bp_standard_deviation: 1.1,
            rp_standard_deviation: 1.2,
            bp_coefficients: vec![1.0, 2.0, 3.0],
            rp_coefficients: vec![4.0, 5.0, 6.0],
            bp_coefficient_errors: vec![0.1, 0.2, 0.3],
            rp_coefficient_errors: vec![0.4, 0.5, 0.6],
            bp_coefficient_correlations: vec![0.1, 0.2, 0.3],
            rp_coefficient_correlations: vec![-0.1, 0.0, 0.2],
            source_format: XpContinuousSourceFormat::DataLink,
            source_checksum: None,
            quality_flags: Vec::new(),
        }
    }

    #[test]
    fn bulk_row_csv_reader_accepts_large_quoted_fields() {
        let path = Path::new("/tmp/bulk_one_row.csv");
        if !path.is_file() {
            return;
        }
        let mut reader = ReaderBuilder::new()
            .buffer_capacity(BULK_CSV_BUFFER_CAPACITY)
            .from_path(path)
            .unwrap();
        let headers = reader.headers().unwrap();
        assert_eq!(headers.len(), 26);
        let record = reader.records().next().unwrap().unwrap();
        assert_eq!(record.len(), 26);
        assert_eq!(record.get(0).unwrap(), "4295806720");
    }

    #[test]
    fn bulk_ecsv_first_row_parses_from_pilot_file() {
        let path = Path::new(
            "/path/to/nsb-data/starlight-gaia-release/pilot-xp-continuous-bulk/bulk/XpContinuousMeanSpectrum_000000-003111.csv.gz",
        );
        if !path.is_file() {
            return;
        }
        let mut stream = stream_bulk_ecsv_gz(path).unwrap();
        assert!(stream.header_map().contains_key("source_id"));
        assert!(stream.header_map().contains_key("bp_coefficients"));
        let record = stream
            .next_record()
            .unwrap()
            .expect("pilot bulk file should contain at least one data row");
        assert_eq!(record.source_id, "4295806720");
        assert_eq!(record.bp_n_parameters, 55);
    }

    #[test]
    fn packed_correlation_length_matches_gaia_convention() {
        assert_eq!(packed_correlation_len(55), 1485);
        assert_eq!(packed_correlation_len(3), 3);
    }

    #[test]
    fn datalink_parenthesis_and_bulk_bracket_arrays_parse_equivalently() {
        let raw = concat!(
            "source_id,bp_n_parameters,bp_standard_deviation,rp_n_parameters,rp_standard_deviation,",
            "bp_coefficients,bp_coefficient_errors,bp_coefficient_correlations,",
            "rp_coefficients,rp_coefficient_errors,rp_coefficient_correlations\n",
            "7,3,1.5,3,1.6,\"[1.0,2.0,3.0]\",\"[0.1,0.2,0.3]\",\"[0.2,0.3,0.4]\",",
            "\"(4.0,5.0,6.0)\",\"(0.4,0.5,0.6)\",\"(0.1,0.2,0.3)\"\n",
        );
        let parsed = parse_datalink_gaiaxpy_csv(raw.as_bytes(), "7").unwrap();
        assert_eq!(parsed.bp_coefficients, vec![1.0, 2.0, 3.0]);
        assert_eq!(parsed.rp_coefficients, vec![4.0, 5.0, 6.0]);
        assert_eq!(parsed.bp_coefficient_correlations, vec![0.2, 0.3, 0.4]);
    }

    #[test]
    fn gaiaxpy_csv_roundtrip_preserves_canonical_fields() {
        let dir = tempfile::tempdir().unwrap();
        let record = sample_record();
        let path = dir.path().join("42.csv");
        write_gaiaxpy_datalink_csv(&path, &record).unwrap();
        let bytes = std::fs::read(&path).unwrap();
        let parsed = parse_datalink_gaiaxpy_csv(&bytes, "42").unwrap();
        assert_eq!(parsed.bp_coefficients, record.bp_coefficients);
        assert_eq!(
            parsed.rp_coefficient_correlations,
            record.rp_coefficient_correlations
        );
    }
}
