use anyhow::{bail, Context, Result};
use clap::Parser;
use csv::{ReaderBuilder, StringRecord, WriterBuilder};
use std::ffi::OsStr;
use std::f64::consts::PI;
use std::fs::File;
use std::io::{self, BufWriter, Write};
use std::path::PathBuf;

const DEG_TO_RAD: f64 = PI / 180.0;
const RAD_TO_DEG: f64 = 180.0 / PI;
const S10_V_TO_INTEGRATED_PH_CM2_NS_SR: f64 = 1.242e-3;

/// Build a rectangular Galactic starlight map from a simple stellar catalogue CSV.
///
/// Input schema, v1:
///
///   ra_deg,dec_deg,b_mag,v_mag[,weight]
///
/// Coordinates are ICRS/J2000 degrees. B and V magnitudes are converted into
/// S10 surface-brightness units using the standard approximation that 1 S10 is
/// the flux of one 10th-magnitude star per square degree. Integrated
/// 300-650 nm photon radiance is currently derived from V-band S10 through the
/// explicit `--integrated-per-v-s10` scale factor.
#[derive(Debug, Parser)]
#[command(name = "build_starlight_map")]
#[command(about = "Generate an NSB starlight Galactic map CSV from a stellar catalogue")]
struct Args {
    /// Input stellar catalogue CSV: ra_deg,dec_deg,b_mag,v_mag[,weight].
    #[arg(long)]
    input: PathBuf,

    /// Output NSB starlight map CSV. Use '-' for stdout.
    #[arg(long)]
    output: PathBuf,

    /// Galactic longitude bin width in degrees. Must divide 360 exactly.
    #[arg(long, default_value_t = 10.0)]
    lon_bin_deg: f64,

    /// Galactic latitude bin width in degrees. Must divide 180 exactly.
    #[arg(long, default_value_t = 10.0)]
    lat_bin_deg: f64,

    /// Optional faint-end V magnitude cut. Rows with v_mag > max_v_mag are skipped.
    #[arg(long)]
    max_v_mag: Option<f64>,

    /// Optional bright-end V magnitude cut. Rows with v_mag < min_v_mag are skipped.
    #[arg(long)]
    min_v_mag: Option<f64>,

    /// Source catalogue name recorded in output comments.
    #[arg(long, default_value = "unknown")]
    catalog_name: String,

    /// Source catalogue release recorded in output comments.
    #[arg(long)]
    catalog_release: Option<String>,

    /// Source catalogue licence recorded in output comments.
    #[arg(long)]
    catalog_license: Option<String>,

    /// Source catalogue checksum recorded in output comments.
    #[arg(long)]
    catalog_checksum: Option<String>,

    /// Conversion from V-band S10 to integrated 300-650 nm photon radiance.
    #[arg(long, default_value_t = S10_V_TO_INTEGRATED_PH_CM2_NS_SR)]
    integrated_per_v_s10: f64,
}

#[derive(Debug, Clone, Copy, Default)]
struct BinAccum {
    b_flux_10mag_star_numerator: f64,
    v_flux_10mag_star_numerator: f64,
}

#[derive(Debug, Clone, Copy)]
struct ColumnIndices {
    ra_deg: usize,
    dec_deg: usize,
    b_mag: usize,
    v_mag: usize,
    weight: Option<usize>,
}

fn main() -> Result<()> {
    let args = Args::parse();
    run(args)
}

fn run(args: Args) -> Result<()> {
    validate_args(&args)?;

    let n_lon = checked_axis_count(360.0, args.lon_bin_deg, "lon-bin-deg")?;
    let n_lat = checked_axis_count(180.0, args.lat_bin_deg, "lat-bin-deg")?;
    let mut bins = vec![BinAccum::default(); n_lon * n_lat];

    let mut reader = ReaderBuilder::new()
        .comment(Some(b'#'))
        .from_path(&args.input)
        .with_context(|| format!("failed to open input catalogue {}", args.input.display()))?;
    let headers = reader.headers().context("failed to read CSV header")?.clone();
    let columns = ColumnIndices::from_headers(&headers)?;

    let mut read_rows = 0_u64;
    let mut used_rows = 0_u64;
    for record in reader.records() {
        let record = record.context("failed to read input CSV record")?;
        read_rows += 1;
        let Some(star) = parse_star(&record, columns)? else {
            continue;
        };
        if let Some(max_v_mag) = args.max_v_mag {
            if star.v_mag > max_v_mag {
                continue;
            }
        }
        if let Some(min_v_mag) = args.min_v_mag {
            if star.v_mag < min_v_mag {
                continue;
            }
        }

        let (lon, lat) = equatorial_to_galactic(star.ra_deg, star.dec_deg);
        let lon_idx = lon_bin_index(lon, args.lon_bin_deg, n_lon);
        let lat_idx = lat_bin_index(lat, args.lat_bin_deg, n_lat);
        let idx = grid_index_for(n_lat, lon_idx, lat_idx);

        bins[idx].b_flux_10mag_star_numerator += star.weight * mag_to_10mag_flux(star.b_mag);
        bins[idx].v_flux_10mag_star_numerator += star.weight * mag_to_10mag_flux(star.v_mag);
        used_rows += 1;
    }

    write_map(&args, n_lon, n_lat, &bins, read_rows, used_rows)
}

fn validate_args(args: &Args) -> Result<()> {
    if !args.lon_bin_deg.is_finite() || args.lon_bin_deg <= 0.0 || args.lon_bin_deg > 360.0 {
        bail!("--lon-bin-deg must be finite and in (0, 360]");
    }
    if !args.lat_bin_deg.is_finite() || args.lat_bin_deg <= 0.0 || args.lat_bin_deg > 180.0 {
        bail!("--lat-bin-deg must be finite and in (0, 180]");
    }
    if !args.integrated_per_v_s10.is_finite() || args.integrated_per_v_s10 < 0.0 {
        bail!("--integrated-per-v-s10 must be finite and non-negative");
    }
    if let (Some(min), Some(max)) = (args.min_v_mag, args.max_v_mag) {
        if !min.is_finite() || !max.is_finite() || min > max {
            bail!("magnitude cuts must be finite and satisfy min <= max");
        }
    }
    Ok(())
}

fn checked_axis_count(span_deg: f64, bin_deg: f64, name: &str) -> Result<usize> {
    let raw = span_deg / bin_deg;
    let rounded = raw.round();
    if (raw - rounded).abs() > 1.0e-10 {
        bail!("--{name}={bin_deg} must divide {span_deg} degrees exactly");
    }
    let n = rounded as usize;
    if n == 0 {
        bail!("--{name} creates an empty axis");
    }
    Ok(n)
}

#[derive(Debug, Clone, Copy)]
struct StarRow {
    ra_deg: f64,
    dec_deg: f64,
    b_mag: f64,
    v_mag: f64,
    weight: f64,
}

impl ColumnIndices {
    fn from_headers(headers: &StringRecord) -> Result<Self> {
        Ok(Self {
            ra_deg: required_header(headers, "ra_deg")?,
            dec_deg: required_header(headers, "dec_deg")?,
            b_mag: required_header(headers, "b_mag")?,
            v_mag: required_header(headers, "v_mag")?,
            weight: optional_header(headers, "weight"),
        })
    }
}

fn required_header(headers: &StringRecord, name: &str) -> Result<usize> {
    optional_header(headers, name).ok_or_else(|| anyhow::anyhow!("missing required column {name:?}"))
}

fn optional_header(headers: &StringRecord, name: &str) -> Option<usize> {
    headers.iter().position(|h| h.trim() == name)
}

fn parse_star(record: &StringRecord, columns: ColumnIndices) -> Result<Option<StarRow>> {
    let ra_deg = parse_f64(record, columns.ra_deg, "ra_deg")?;
    let dec_deg = parse_f64(record, columns.dec_deg, "dec_deg")?;
    let b_mag = parse_f64(record, columns.b_mag, "b_mag")?;
    let v_mag = parse_f64(record, columns.v_mag, "v_mag")?;
    let weight = match columns.weight {
        Some(idx) => parse_f64(record, idx, "weight")?,
        None => 1.0,
    };

    if !ra_deg.is_finite()
        || !dec_deg.is_finite()
        || !b_mag.is_finite()
        || !v_mag.is_finite()
        || !weight.is_finite()
    {
        bail!("catalogue rows must contain finite ra/dec/magnitude/weight values");
    }
    if !(-90.0..=90.0).contains(&dec_deg) {
        bail!("dec_deg={dec_deg} is outside [-90, 90]");
    }
    if weight < 0.0 {
        bail!("weight must be non-negative");
    }
    if weight == 0.0 {
        return Ok(None);
    }

    Ok(Some(StarRow {
        ra_deg: normalize_degrees_360(ra_deg),
        dec_deg,
        b_mag,
        v_mag,
        weight,
    }))
}

fn parse_f64(record: &StringRecord, idx: usize, name: &str) -> Result<f64> {
    record
        .get(idx)
        .ok_or_else(|| anyhow::anyhow!("missing field {name:?}"))?
        .trim()
        .parse::<f64>()
        .with_context(|| format!("invalid numeric field {name:?}"))
}

fn write_map(
    args: &Args,
    n_lon: usize,
    n_lat: usize,
    bins: &[BinAccum],
    read_rows: u64,
    used_rows: u64,
) -> Result<()> {
    let mut out: Box<dyn Write> = if args.output.as_os_str() == OsStr::new("-") {
        Box::new(BufWriter::new(io::stdout()))
    } else {
        Box::new(BufWriter::new(File::create(&args.output).with_context(|| {
            format!("failed to create output map {}", args.output.display())
        })?))
    };

    writeln!(out, "# generated_by=nsb-data-tools build_starlight_map")?;
    writeln!(out, "# source_catalog_name={}", args.catalog_name)?;
    if let Some(value) = &args.catalog_release {
        writeln!(out, "# source_catalog_release={value}")?;
    }
    if let Some(value) = &args.catalog_license {
        writeln!(out, "# source_catalog_license={value}")?;
    }
    if let Some(value) = &args.catalog_checksum {
        writeln!(out, "# source_catalog_checksum={value}")?;
    }
    writeln!(out, "# input_rows={read_rows}")?;
    writeln!(out, "# used_rows={used_rows}")?;
    writeln!(out, "# lon_bin_deg={}", args.lon_bin_deg)?;
    writeln!(out, "# lat_bin_deg={}", args.lat_bin_deg)?;
    writeln!(out, "# integrated_per_v_s10={}", args.integrated_per_v_s10)?;
    writeln!(out, "# input_schema=ra_deg,dec_deg,b_mag,v_mag[,weight]")?;
    writeln!(out, "# calibration_note=1 S10 is treated as one 10th-magnitude star per square degree; integrated radiance is V-S10 scaled")?;

    let mut writer = WriterBuilder::new().has_headers(false).from_writer(out);
    writer.write_record([
        "galactic_lon_deg",
        "galactic_lat_deg",
        "solid_angle_sr",
        "integrated_ph_cm2_ns_sr",
        "b_s10",
        "v_s10",
    ])?;

    for lon_idx in 0..n_lon {
        for lat_idx in 0..n_lat {
            let idx = grid_index_for(n_lat, lon_idx, lat_idx);
            let lon_center = (lon_idx as f64 + 0.5) * args.lon_bin_deg;
            let lat_min = -90.0 + lat_idx as f64 * args.lat_bin_deg;
            let lat_max = lat_min + args.lat_bin_deg;
            let lat_center = 0.5 * (lat_min + lat_max);
            let solid_angle = cell_solid_angle_sr(args.lon_bin_deg, lat_min, lat_max);
            let solid_angle_deg2 = solid_angle * RAD_TO_DEG * RAD_TO_DEG;
            let b_s10 = bins[idx].b_flux_10mag_star_numerator / solid_angle_deg2;
            let v_s10 = bins[idx].v_flux_10mag_star_numerator / solid_angle_deg2;
            let integrated = v_s10 * args.integrated_per_v_s10;

            writer.write_record([
                format_float(normalize_degrees_360(lon_center)),
                format_float(lat_center),
                format_float(solid_angle),
                format_float(integrated),
                format_float(b_s10),
                format_float(v_s10),
            ])?;
        }
    }
    writer.flush()?;
    Ok(())
}

fn format_float(value: f64) -> String {
    format!("{value:.12e}")
}

fn grid_index_for(n_lat: usize, lon_idx: usize, lat_idx: usize) -> usize {
    lon_idx * n_lat + lat_idx
}

fn lon_bin_index(lon_deg: f64, bin_deg: f64, n_lon: usize) -> usize {
    let idx = (normalize_degrees_360(lon_deg) / bin_deg).floor() as usize;
    idx.min(n_lon - 1)
}

fn lat_bin_index(lat_deg: f64, bin_deg: f64, n_lat: usize) -> usize {
    let idx = ((lat_deg + 90.0) / bin_deg).floor() as usize;
    idx.min(n_lat - 1)
}

fn cell_solid_angle_sr(lon_bin_deg: f64, lat_min_deg: f64, lat_max_deg: f64) -> f64 {
    lon_bin_deg * DEG_TO_RAD
        * ((lat_max_deg * DEG_TO_RAD).sin() - (lat_min_deg * DEG_TO_RAD).sin())
}

fn mag_to_10mag_flux(mag: f64) -> f64 {
    10.0_f64.powf(-0.4 * (mag - 10.0))
}

fn equatorial_to_galactic(ra_deg: f64, dec_deg: f64) -> (f64, f64) {
    let ra = ra_deg * DEG_TO_RAD;
    let dec = dec_deg * DEG_TO_RAD;
    let x_eq = dec.cos() * ra.cos();
    let y_eq = dec.cos() * ra.sin();
    let z_eq = dec.sin();

    let x_gal = -0.054_875_560_416_215_4 * x_eq
        - 0.873_437_090_234_885_0 * y_eq
        - 0.483_835_015_548_713_2 * z_eq;
    let y_gal = 0.494_109_427_875_583_7 * x_eq
        - 0.444_829_629_960_011_2 * y_eq
        + 0.746_982_244_497_218_9 * z_eq;
    let z_gal = -0.867_666_149_019_004_7 * x_eq
        - 0.198_076_373_431_201_5 * y_eq
        + 0.455_983_776_175_066_9 * z_eq;

    let lon = normalize_degrees_360(y_gal.atan2(x_gal) * RAD_TO_DEG);
    let lat = z_gal.clamp(-1.0, 1.0).asin() * RAD_TO_DEG;
    (lon, lat)
}

fn normalize_degrees_360(deg: f64) -> f64 {
    let mut x = deg % 360.0;
    if x < 0.0 {
        x += 360.0;
    }
    x
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn galactic_center_is_near_zero_zero() {
        let (lon, lat) = equatorial_to_galactic(266.4051, -28.936175);
        assert!(lon < 0.2 || (lon - 360.0).abs() < 0.2, "l={lon}");
        assert!(lat.abs() < 0.2, "b={lat}");
    }

    #[test]
    fn mag_ten_is_one_s10_numerator() {
        assert!((mag_to_10mag_flux(10.0) - 1.0).abs() < 1.0e-12);
        assert!((mag_to_10mag_flux(15.0) - 0.01).abs() < 1.0e-12);
    }
}
