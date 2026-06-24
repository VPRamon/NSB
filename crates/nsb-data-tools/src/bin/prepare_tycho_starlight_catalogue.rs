use anyhow::{bail, Context, Result};
use clap::Parser;
use csv::{ReaderBuilder, StringRecord, WriterBuilder};
use serde::Serialize;
use siderust::checksum::{sha256, to_hex};
use std::ffi::OsStr;
use std::fs::File;
use std::io::{self, BufWriter, Write};
use std::path::PathBuf;

/// Prepare canonical starlight catalogue rows from a local Tycho-like CSV.
///
/// Input columns: ra_deg,dec_deg,bt_mag,vt_mag plus optional weight,source_id.
/// Output columns: ra_deg,dec_deg,b_mag,v_mag,weight,source_id.
///
/// This is an offline preparation step. It does not download catalogues and the
/// BT/VT to Johnson-like B/V conversion is an experimental proxy, not production
/// passband calibration.
#[derive(Debug, Parser)]
struct Args {
    #[arg(long)]
    input: PathBuf,
    #[arg(long)]
    output: PathBuf,
    #[arg(long)]
    diagnostics_output: Option<PathBuf>,
    #[arg(long)]
    catalog_name: String,
    #[arg(long)]
    catalog_release: Option<String>,
    #[arg(long)]
    catalog_license: Option<String>,
    #[arg(long)]
    input_checksum: Option<String>,
    #[arg(long)]
    min_v_mag: Option<f64>,
    #[arg(long)]
    max_v_mag: Option<f64>,
}

#[derive(Debug, Clone, Copy)]
struct Columns {
    ra_deg: usize,
    dec_deg: usize,
    bt_mag: usize,
    vt_mag: usize,
    weight: Option<usize>,
    source_id: Option<usize>,
}

#[derive(Debug, Serialize)]
struct Diagnostics<'a> {
    schema_version: u32,
    catalogue_name: &'a str,
    catalogue_release: Option<&'a str>,
    catalogue_license: Option<&'a str>,
    input_checksum: String,
    photometry_model: &'static str,
    calibration_status: &'static str,
    min_v_mag: Option<f64>,
    max_v_mag: Option<f64>,
    rows_read: usize,
    rows_used: usize,
}

fn main() -> Result<()> {
    run(Args::parse())
}

fn run(args: Args) -> Result<()> {
    if args.catalog_name.trim().is_empty() {
        bail!("--catalog-name must not be empty");
    }
    if let (Some(min), Some(max)) = (args.min_v_mag, args.max_v_mag) {
        if !min.is_finite() || !max.is_finite() || min > max {
            bail!("magnitude cuts must be finite and satisfy min <= max");
        }
    }
    let input_bytes = std::fs::read(&args.input)
        .with_context(|| format!("failed to checksum {}", args.input.display()))?;
    let input_checksum = format!("sha256:{}", to_hex(&sha256(&input_bytes)));
    if let Some(expected) = args.input_checksum.as_deref() {
        let expected = expected.strip_prefix("sha256:").unwrap_or(expected);
        let actual = input_checksum.trim_start_matches("sha256:");
        if expected != actual {
            bail!(
                "input checksum mismatch for {}: expected sha256:{expected}, actual {input_checksum}",
                args.input.display()
            );
        }
    }

    let mut reader = ReaderBuilder::new()
        .comment(Some(b'#'))
        .trim(csv::Trim::All)
        .from_path(&args.input)
        .with_context(|| format!("failed to open input catalogue {}", args.input.display()))?;
    let headers = reader
        .headers()
        .context("failed to read input CSV header")?
        .clone();
    let columns = Columns::from_headers(&headers)?;

    let mut rows_read = 0usize;
    let mut rows_used = 0usize;
    let mut writer = WriterBuilder::new().from_writer(output_writer(&args.output)?);
    writer.write_record(["ra_deg", "dec_deg", "b_mag", "v_mag", "weight", "source_id"])?;

    for row in reader.records() {
        let row = row.context("failed to read input CSV record")?;
        rows_read += 1;
        if let Some(output) = convert_row(&row, columns, &args)? {
            rows_used += 1;
            writer.write_record(output)?;
        }
    }
    writer.flush()?;

    if let Some(path) = &args.diagnostics_output {
        let diagnostics = Diagnostics {
            schema_version: 1,
            catalogue_name: &args.catalog_name,
            catalogue_release: args.catalog_release.as_deref(),
            catalogue_license: args.catalog_license.as_deref(),
            input_checksum,
            photometry_model: "tycho_bt_vt_to_johnson_bv_proxy_v1",
            calibration_status: "experimental-proxy",
            min_v_mag: args.min_v_mag,
            max_v_mag: args.max_v_mag,
            rows_read,
            rows_used,
        };
        let raw = serde_json::to_string_pretty(&diagnostics)?;
        std::fs::write(path, format!("{raw}\n"))
            .with_context(|| format!("failed to write diagnostics {}", path.display()))?;
    }

    Ok(())
}

impl Columns {
    fn from_headers(headers: &StringRecord) -> Result<Self> {
        Ok(Self {
            ra_deg: required_header(headers, "ra_deg")?,
            dec_deg: required_header(headers, "dec_deg")?,
            bt_mag: required_header(headers, "bt_mag")?,
            vt_mag: required_header(headers, "vt_mag")?,
            weight: optional_header(headers, "weight"),
            source_id: optional_header(headers, "source_id"),
        })
    }
}

fn required_header(headers: &StringRecord, name: &str) -> Result<usize> {
    optional_header(headers, name)
        .ok_or_else(|| anyhow::anyhow!("missing required column {name:?}"))
}

fn optional_header(headers: &StringRecord, name: &str) -> Option<usize> {
    headers.iter().position(|h| h.trim() == name)
}

fn convert_row(row: &StringRecord, columns: Columns, args: &Args) -> Result<Option<[String; 6]>> {
    let ra_deg = parse_f64(row, columns.ra_deg, "ra_deg")?;
    let dec_deg = parse_f64(row, columns.dec_deg, "dec_deg")?;
    let bt_mag = parse_f64(row, columns.bt_mag, "bt_mag")?;
    let vt_mag = parse_f64(row, columns.vt_mag, "vt_mag")?;
    let weight = match columns.weight {
        Some(idx) => parse_f64(row, idx, "weight")?,
        None => 1.0,
    };
    if !ra_deg.is_finite() || !dec_deg.is_finite() || !bt_mag.is_finite() || !vt_mag.is_finite() {
        bail!("input rows must contain finite coordinates and magnitudes");
    }
    if !(-90.0..=90.0).contains(&dec_deg) {
        bail!("dec_deg={dec_deg} is outside [-90, 90]");
    }
    if !weight.is_finite() || weight <= 0.0 {
        return Ok(None);
    }

    let (b_mag, v_mag) = tycho_to_johnson_bv(bt_mag, vt_mag);
    if args.min_v_mag.is_some_and(|min| v_mag < min)
        || args.max_v_mag.is_some_and(|max| v_mag > max)
    {
        return Ok(None);
    }
    let source_id = columns
        .source_id
        .and_then(|idx| row.get(idx))
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("unknown")
        .to_string();
    Ok(Some([
        format!("{ra_deg:.10}"),
        format!("{dec_deg:.10}"),
        format!("{b_mag:.10}"),
        format!("{v_mag:.10}"),
        format!("{weight:.10}"),
        source_id,
    ]))
}

fn parse_f64(row: &StringRecord, idx: usize, name: &str) -> Result<f64> {
    row.get(idx)
        .ok_or_else(|| anyhow::anyhow!("missing field {name:?}"))?
        .trim()
        .parse::<f64>()
        .with_context(|| format!("invalid numeric field {name:?}"))
}

fn tycho_to_johnson_bv(bt_mag: f64, vt_mag: f64) -> (f64, f64) {
    let color = bt_mag - vt_mag;
    let v_mag = vt_mag - 0.090 * color;
    let b_mag = v_mag + 0.850 * color;
    (b_mag, v_mag)
}

fn output_writer(path: &PathBuf) -> Result<Box<dyn Write>> {
    if path.as_os_str() == OsStr::new("-") {
        Ok(Box::new(BufWriter::new(io::stdout())))
    } else {
        Ok(Box::new(BufWriter::new(File::create(path).with_context(
            || format!("failed to create output catalogue {}", path.display()),
        )?)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preparation_writes_canonical_rows_and_json_diagnostics() -> Result<()> {
        let dir = tempfile::tempdir()?;
        let input = dir.path().join("tycho.csv");
        let output = dir.path().join("canonical.csv");
        let diagnostics = dir.path().join("diagnostics.json");
        std::fs::write(
            &input,
            "ra_deg,dec_deg,bt_mag,vt_mag,source_id\n10.0,20.0,6.0,5.5,T1\n",
        )?;

        run(Args {
            input,
            output: output.clone(),
            diagnostics_output: Some(diagnostics.clone()),
            catalog_name: "Tycho fixture".to_string(),
            catalog_release: Some("test".to_string()),
            catalog_license: Some("test-only".to_string()),
            input_checksum: None,
            min_v_mag: None,
            max_v_mag: Some(11.5),
        })?;

        let canonical = std::fs::read_to_string(output)?;
        assert!(canonical.starts_with("ra_deg,dec_deg,b_mag,v_mag,weight,source_id"));
        let report: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(diagnostics)?)?;
        assert_eq!(report["schema_version"], 1);
        assert_eq!(report["rows_read"], 1);
        assert_eq!(report["rows_used"], 1);
        assert_eq!(report["calibration_status"], "experimental-proxy");
        assert!(report["input_checksum"]
            .as_str()
            .is_some_and(|value| value.starts_with("sha256:")));
        Ok(())
    }
}
