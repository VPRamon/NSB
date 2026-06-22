use anyhow::{bail, Context, Result};
use clap::Parser;
use csv::{ReaderBuilder, StringRecord, WriterBuilder};
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

    let mut reader = ReaderBuilder::new()
        .comment(Some(b'#'))
        .trim(csv::Trim::All)
        .from_path(&args.input)
        .with_context(|| format!("failed to open input catalogue {}", args.input.display()))?;
    let headers = reader.headers().context("failed to read input CSV header")?.clone();
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
        let diagnostics = format!(
            concat!(
                "catalogue_name={}\n",
                "catalogue_release={}\n",
                "catalogue_license={}\n",
                "input_checksum={}\n",
                "photometry_model=tycho_bt_vt_to_johnson_bv_proxy_v1\n",
                "filters=min_v_mag={:?};max_v_mag={:?}\n",
                "rows_read={}\n",
                "rows_used={}\n"
            ),
            args.catalog_name,
            args.catalog_release.as_deref().unwrap_or("not recorded"),
            args.catalog_license.as_deref().unwrap_or("not recorded"),
            args.input_checksum.as_deref().unwrap_or("not recorded"),
            args.min_v_mag,
            args.max_v_mag,
            rows_read,
            rows_used,
        );
        std::fs::write(path, diagnostics)
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
    optional_header(headers, name).ok_or_else(|| anyhow::anyhow!("missing required column {name:?}"))
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
    if args.min_v_mag.is_some_and(|min| v_mag < min) || args.max_v_mag.is_some_and(|max| v_mag > max) {
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
    if path.as_os_str() == "-" {
        Ok(Box::new(BufWriter::new(io::stdout())))
    } else {
        Ok(Box::new(BufWriter::new(File::create(path).with_context(|| {
            format!("failed to create output catalogue {}", path.display())
        })?)))
    }
}
