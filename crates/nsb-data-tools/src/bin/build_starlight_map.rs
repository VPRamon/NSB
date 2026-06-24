use anyhow::{bail, Context, Result};
use clap::{Parser, ValueEnum};
use csv::{ReaderBuilder, StringRecord};
use serde::Serialize;
use siderust::checksum::{sha256, to_hex};
use siderust::coordinates::cartesian::Direction;
use siderust::coordinates::frames::EquatorialMeanJ2000;
use siderust::healpix::{HealpixGrid, HealpixMap, HealpixOrdering, Nside};
use siderust::starlight::{
    csv as starlight_csv, flux_10mag_units, validate_flux_conservation,
    validate_no_longitude_wrap_artifact, validate_plane_pole_contrast, ApparentMagnitude,
    StellarCatalogueRecord, StellarMapError, StellarMapProvenance, StellarSurfaceBrightness,
    StellarSurfaceBrightnessMap, StellarSurfaceBrightnessMapBuilder,
};
use std::ffi::OsStr;
use std::fs::File;
use std::io::{self, BufWriter, Write};
use std::path::PathBuf;

const S10_V_TO_INTEGRATED_PH_CM2_NS_SR: f64 = 1.242e-3;
const HEALPIX_CSV_HEADER: &str = "healpix_index,integrated_ph_cm2_ns_sr,b_s10,v_s10";

/// Build a Galactic HEALPix starlight map from a local catalogue CSV.
///
/// The executable is intentionally an orchestration layer: HEALPix binning,
/// EquatorialMeanJ2000 -> Galactic transforms, stellar map construction, and
/// validators are provided by Siderust.
#[derive(Debug, Parser)]
#[command(name = "build_starlight_map")]
#[command(about = "Generate an NSB starlight HEALPix CSV from a local stellar catalogue")]
struct Args {
    /// Input stellar catalogue CSV: ra_deg,dec_deg,b_mag,v_mag,weight,source_id.
    #[arg(long)]
    input: PathBuf,

    /// Output NSB starlight map CSV. Use '-' for stdout.
    #[arg(long)]
    output: PathBuf,

    /// HEALPix nside.
    #[arg(long, default_value_t = 64)]
    nside: u32,

    /// HEALPix ordering.
    #[arg(long, value_enum, default_value_t = OrderingArg::Ring)]
    ordering: OrderingArg,

    /// Optional inclusive bright-end V magnitude cut.
    #[arg(long)]
    min_v_mag: Option<f64>,

    /// Optional inclusive faint-end V magnitude cut.
    #[arg(long)]
    max_v_mag: Option<f64>,

    /// Source catalogue name recorded in output comments.
    #[arg(long)]
    catalog_name: String,

    /// Source catalogue release recorded in output comments.
    #[arg(long)]
    catalog_release: Option<String>,

    /// Source catalogue license recorded in output comments.
    #[arg(long)]
    catalog_license: Option<String>,

    /// Source catalogue checksum recorded in output comments.
    #[arg(long)]
    catalog_checksum: Option<String>,

    /// Conversion from V-band S10 to integrated 300-650 nm photon radiance.
    #[arg(long, default_value_t = S10_V_TO_INTEGRATED_PH_CM2_NS_SR)]
    integrated_per_v_s10: f64,

    /// UTC generation timestamp written to provenance metadata.
    #[arg(long)]
    generation_date_utc: String,

    /// Optional JSON diagnostics report written beside the map.
    #[arg(long)]
    diagnostics_output: Option<PathBuf>,

    /// Require all full-sky scientific diagnostics to pass.
    ///
    /// Small deterministic fixtures may not satisfy all regional diagnostics,
    /// but production catalogue maps should enable this flag in CI.
    #[arg(long)]
    require_science_diagnostics: bool,

    /// Allow generating an all-zero map if no catalogue rows survive filters.
    #[arg(long)]
    allow_empty: bool,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
enum OrderingArg {
    Ring,
    Nested,
}

impl From<OrderingArg> for HealpixOrdering {
    fn from(value: OrderingArg) -> Self {
        match value {
            OrderingArg::Ring => Self::Ring,
            OrderingArg::Nested => Self::Nested,
        }
    }
}

#[derive(Debug, Clone, Copy)]
struct ColumnIndices {
    ra_deg: usize,
    dec_deg: usize,
    b_mag: usize,
    v_mag: usize,
    weight: Option<usize>,
    source_id: Option<usize>,
}

#[derive(Debug, Serialize)]
struct Diagnostics {
    schema_version: u32,
    sources_used: usize,
    nside: u32,
    ordering: &'static str,
    expected_pixels: usize,
    empty_pixels: usize,
    total_integrated_ph_cm2_ns_sr: f64,
    total_b_s10: f64,
    total_v_s10: f64,
    flux_conservation_pass: bool,
    plane_pole_pass: bool,
    longitude_wrap_pass: bool,
    output_sha256: String,
    photometry_model: &'static str,
    calibration_status: &'static str,
}

fn main() -> Result<()> {
    run(Args::parse())
}

fn run(args: Args) -> Result<()> {
    validate_args(&args)?;
    verify_catalog_checksum(&args)?;

    let grid = HealpixGrid::new(Nside::new(args.nside)?, args.ordering.into())?;
    let min_v_mag = args.min_v_mag.map(ApparentMagnitude::new).transpose()?;
    let max_v_mag = args.max_v_mag.map(ApparentMagnitude::new).transpose()?;
    let provenance = provenance(&args);

    let (records, input_b_flux_sum, input_v_flux_sum) =
        read_records(&args.input, min_v_mag, max_v_mag)?;
    let sources_used = records.len();

    let builder = StellarSurfaceBrightnessMapBuilder {
        grid,
        min_v_mag,
        max_v_mag,
        integrated_per_v_s10: args.integrated_per_v_s10,
    };

    let map = match builder.build(records, provenance.clone()) {
        Ok(map) => map,
        Err(StellarMapError::EmptyFilteredCatalogue) if args.allow_empty => {
            empty_map(grid, provenance.clone())?
        }
        Err(err) => return Err(err.into()),
    };

    validate_flux_conservation(
        input_b_flux_sum,
        input_v_flux_sum,
        map.healpix_map(),
        1.0e-9,
    )?;
    let (longitude_wrap_pass, plane_pole_pass) =
        run_science_diagnostics(&map, args.require_science_diagnostics)?;

    let generation_command = std::env::args().collect::<Vec<_>>().join(" ");
    let validation_report = args
        .diagnostics_output
        .as_ref()
        .map(|path| path.display().to_string())
        .unwrap_or_else(|| "not emitted for this non-production run".to_string());
    let csv = stellar_map_to_csv(&map, &generation_command, &validation_report);
    write_output(&args.output, &csv)?;
    if let Some(path) = &args.diagnostics_output {
        let diagnostics = diagnostics(
            &map,
            sources_used,
            longitude_wrap_pass,
            plane_pole_pass,
            &csv,
        );
        let raw = serde_json::to_string_pretty(&diagnostics)?;
        std::fs::write(path, format!("{raw}\n"))
            .with_context(|| format!("failed to write diagnostics {}", path.display()))?;
    }
    Ok(())
}

fn validate_args(args: &Args) -> Result<()> {
    if args.nside == 0 {
        bail!("--nside must be greater than zero");
    }
    if let (Some(min), Some(max)) = (args.min_v_mag, args.max_v_mag) {
        if !min.is_finite() || !max.is_finite() || min > max {
            bail!("magnitude cuts must be finite and satisfy min <= max");
        }
    }
    if !args.integrated_per_v_s10.is_finite() || args.integrated_per_v_s10 < 0.0 {
        bail!("--integrated-per-v-s10 must be finite and non-negative");
    }
    if args.generation_date_utc.trim().is_empty() {
        bail!("--generation-date-utc must not be empty");
    }
    if args.require_science_diagnostics
        && (args.catalog_release.as_deref().is_none_or(str::is_empty)
            || args.catalog_license.as_deref().is_none_or(str::is_empty)
            || args.catalog_checksum.as_deref().is_none_or(str::is_empty)
            || args.diagnostics_output.is_none())
    {
        bail!(
            "--require-science-diagnostics also requires --catalog-release, --catalog-license, --catalog-checksum, and --diagnostics-output"
        );
    }
    Ok(())
}

fn verify_catalog_checksum(args: &Args) -> Result<()> {
    let Some(expected) = args.catalog_checksum.as_deref() else {
        return Ok(());
    };
    let expected = expected.strip_prefix("sha256:").unwrap_or(expected);
    let bytes = std::fs::read(&args.input)
        .with_context(|| format!("failed to checksum {}", args.input.display()))?;
    let actual = to_hex(&sha256(&bytes));
    if expected != actual {
        bail!(
            "catalogue checksum mismatch for {}: expected sha256:{expected}, actual sha256:{actual}",
            args.input.display()
        );
    }
    Ok(())
}

fn read_records(
    input: &PathBuf,
    min_v_mag: Option<ApparentMagnitude>,
    max_v_mag: Option<ApparentMagnitude>,
) -> Result<(Vec<StellarCatalogueRecord>, f64, f64)> {
    let mut reader = ReaderBuilder::new()
        .comment(Some(b'#'))
        .from_path(input)
        .with_context(|| format!("failed to open input catalogue {}", input.display()))?;
    let headers = reader
        .headers()
        .context("failed to read CSV header")?
        .clone();
    let columns = ColumnIndices::from_headers(&headers)?;

    let mut records = Vec::new();
    let mut input_b_flux_sum = 0.0;
    let mut input_v_flux_sum = 0.0;
    for row in reader.records() {
        let row = row.context("failed to read input CSV record")?;
        if let Some(record) = parse_record(&row, columns)? {
            if passes_v_cut(record.v_mag, min_v_mag, max_v_mag) {
                if let Some(mag) = record.b_mag {
                    input_b_flux_sum += flux_10mag_units(mag) * record.weight;
                }
                if let Some(mag) = record.v_mag {
                    input_v_flux_sum += flux_10mag_units(mag) * record.weight;
                }
            }
            records.push(record);
        }
    }
    Ok((records, input_b_flux_sum, input_v_flux_sum))
}

impl ColumnIndices {
    fn from_headers(headers: &StringRecord) -> Result<Self> {
        Ok(Self {
            ra_deg: required_header(headers, "ra_deg")?,
            dec_deg: required_header(headers, "dec_deg")?,
            b_mag: required_header(headers, "b_mag")?,
            v_mag: required_header(headers, "v_mag")?,
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

fn parse_record(
    row: &StringRecord,
    columns: ColumnIndices,
) -> Result<Option<StellarCatalogueRecord>> {
    let ra_deg = parse_required_f64(row, columns.ra_deg, "ra_deg")?;
    let dec_deg = parse_required_f64(row, columns.dec_deg, "dec_deg")?;
    let b_mag = parse_optional_mag(row, columns.b_mag, "b_mag")?;
    let v_mag = parse_optional_mag(row, columns.v_mag, "v_mag")?;
    let weight = match columns.weight {
        Some(idx) => parse_required_f64(row, idx, "weight")?,
        None => 1.0,
    };
    let source_id = columns
        .source_id
        .and_then(|idx| row.get(idx))
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned);

    if !ra_deg.is_finite() || !dec_deg.is_finite() || !weight.is_finite() {
        bail!("catalogue rows must contain finite coordinates and weights");
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

    let direction = equatorial_direction(ra_deg, dec_deg);
    Ok(Some(StellarCatalogueRecord {
        source_id,
        direction,
        b_mag,
        v_mag,
        weight,
    }))
}

fn parse_required_f64(row: &StringRecord, idx: usize, name: &str) -> Result<f64> {
    row.get(idx)
        .ok_or_else(|| anyhow::anyhow!("missing field {name:?}"))?
        .trim()
        .parse::<f64>()
        .with_context(|| format!("invalid numeric field {name:?}"))
}

fn parse_optional_mag(
    row: &StringRecord,
    idx: usize,
    name: &str,
) -> Result<Option<ApparentMagnitude>> {
    let raw = row
        .get(idx)
        .ok_or_else(|| anyhow::anyhow!("missing field {name:?}"))?
        .trim();
    if raw.is_empty() {
        return Ok(None);
    }
    Ok(Some(ApparentMagnitude::new(
        raw.parse::<f64>()
            .with_context(|| format!("invalid numeric field {name:?}"))?,
    )?))
}

fn equatorial_direction(ra_deg: f64, dec_deg: f64) -> Direction<EquatorialMeanJ2000> {
    let ra = ra_deg.to_radians();
    let dec = dec_deg.to_radians();
    Direction::<EquatorialMeanJ2000>::from_array([
        dec.cos() * ra.cos(),
        dec.cos() * ra.sin(),
        dec.sin(),
    ])
}

fn passes_v_cut(
    magnitude: Option<ApparentMagnitude>,
    min_v_mag: Option<ApparentMagnitude>,
    max_v_mag: Option<ApparentMagnitude>,
) -> bool {
    match magnitude {
        Some(value) => {
            min_v_mag.is_none_or(|min| value.value() >= min.value())
                && max_v_mag.is_none_or(|max| value.value() <= max.value())
        }
        None => min_v_mag.is_none() && max_v_mag.is_none(),
    }
}

fn empty_map(
    grid: HealpixGrid,
    provenance: StellarMapProvenance,
) -> Result<StellarSurfaceBrightnessMap> {
    let values = vec![StellarSurfaceBrightness::zero(); usize::try_from(grid.npix())?];
    let map = HealpixMap::new(grid, values)?;
    Ok(StellarSurfaceBrightnessMap::new(map, provenance))
}

fn provenance(args: &Args) -> StellarMapProvenance {
    let magnitude_limit = match (args.min_v_mag, args.max_v_mag) {
        (Some(min), Some(max)) => format!("{min} <= V <= {max}"),
        (Some(min), None) => format!("V >= {min}"),
        (None, Some(max)) => format!("V <= {max}"),
        (None, None) => "none".to_string(),
    };

    StellarMapProvenance {
        dataset_name: "NSB catalogue-derived Galactic starlight map".to_string(),
        version: "v1".to_string(),
        generation_date_utc: args.generation_date_utc.clone(),
        source_catalogue: args.catalog_name.clone(),
        source_catalogue_release: args.catalog_release.clone(),
        source_catalogue_license: args.catalog_license.clone(),
        source_catalogue_checksum: args.catalog_checksum.clone(),
        magnitude_limit: Some(magnitude_limit),
        band_definition: "integrated 300-650 nm photon radiance plus B/V S10 diagnostics"
            .to_string(),
        photometry_model: "v_s10_scaled_integrated_proxy_v1".to_string(),
        smoothing: None,
        generator: "nsb-data-tools build_starlight_map using siderust feature/healpix-stellar-maps"
            .to_string(),
    }
}

fn run_science_diagnostics(
    map: &StellarSurfaceBrightnessMap,
    require: bool,
) -> Result<(bool, bool)> {
    let longitude_wrap = validate_no_longitude_wrap_artifact(map.healpix_map(), 1.0);
    let longitude_wrap_pass = longitude_wrap.is_ok();
    handle_science_diagnostic("longitude-wrap artifact", longitude_wrap, require)?;
    let plane_pole = validate_plane_pole_contrast(map.healpix_map(), 1.0);
    let plane_pole_pass = plane_pole.is_ok();
    handle_science_diagnostic("plane/pole contrast", plane_pole, require)?;
    Ok((longitude_wrap_pass, plane_pole_pass))
}

fn handle_science_diagnostic<E: std::fmt::Display>(
    name: &str,
    result: std::result::Result<(), E>,
    require: bool,
) -> Result<()> {
    match result {
        Ok(()) => Ok(()),
        Err(err) if require => Err(anyhow::anyhow!("starlight diagnostic {name} failed: {err}")),
        Err(err) => {
            eprintln!("warning: starlight diagnostic {name} did not pass: {err}");
            Ok(())
        }
    }
}

fn stellar_map_to_csv(
    map: &StellarSurfaceBrightnessMap,
    generation_command: &str,
    validation_report: &str,
) -> String {
    let mut out = String::new();
    let provenance = map.provenance();
    let grid = map.grid();
    let ordering = ordering_name(grid.ordering());

    push_metadata(&mut out, "map_type", "healpix");
    push_metadata(&mut out, "coordinate_frame", "galactic");
    push_metadata(&mut out, "nside", &grid.nside().get().to_string());
    push_metadata(&mut out, "ordering", ordering);
    push_metadata(
        &mut out,
        "map_resolution",
        &format!("HEALPix nside={} ordering={ordering}", grid.nside().get()),
    );
    push_metadata(&mut out, "dataset_name", &provenance.dataset_name);
    push_metadata(&mut out, "version", &provenance.version);
    push_metadata(&mut out, "calibration_status", "Experimental");
    push_metadata(
        &mut out,
        "generation_date_utc",
        &provenance.generation_date_utc,
    );
    push_metadata(&mut out, "source_catalogue", &provenance.source_catalogue);
    if let Some(value) = &provenance.source_catalogue_release {
        push_metadata(&mut out, "source_catalogue_release", value);
    }
    if let Some(value) = &provenance.source_catalogue_license {
        push_metadata(&mut out, "source_catalogue_license", value);
    }
    if let Some(value) = &provenance.source_catalogue_checksum {
        push_metadata(&mut out, "source_catalogue_checksum", value);
    }
    if let Some(value) = &provenance.magnitude_limit {
        push_metadata(&mut out, "magnitude_limit", value);
    }
    push_metadata(&mut out, "band_definition", &provenance.band_definition);
    push_metadata(&mut out, "photometry_model", &provenance.photometry_model);
    if let Some(value) = &provenance.smoothing {
        push_metadata(&mut out, "smoothing", value);
    }
    push_metadata(&mut out, "generated_by", &provenance.generator);
    push_metadata(&mut out, "generation_command", generation_command);
    push_metadata(&mut out, "validation_report", validation_report);

    let data = starlight_csv::to_csv(map);
    if data
        .lines()
        .next()
        .is_some_and(|line| line.trim() == HEALPIX_CSV_HEADER)
    {
        out.push_str(&data);
    } else {
        out.push_str(HEALPIX_CSV_HEADER);
        out.push('\n');
        out.push_str(&data);
    }
    if !out.ends_with('\n') {
        out.push('\n');
    }
    out
}

fn diagnostics(
    map: &StellarSurfaceBrightnessMap,
    sources_used: usize,
    longitude_wrap_pass: bool,
    plane_pole_pass: bool,
    csv: &str,
) -> Diagnostics {
    let values = map.values();
    Diagnostics {
        schema_version: 1,
        sources_used,
        nside: map.grid().nside().get(),
        ordering: ordering_name(map.grid().ordering()),
        expected_pixels: values.len(),
        empty_pixels: values
            .iter()
            .filter(|value| {
                value.integrated_ph_cm2_ns_sr == 0.0 && value.b_s10 == 0.0 && value.v_s10 == 0.0
            })
            .count(),
        total_integrated_ph_cm2_ns_sr: values
            .iter()
            .map(|value| value.integrated_ph_cm2_ns_sr)
            .sum(),
        total_b_s10: values.iter().map(|value| value.b_s10).sum(),
        total_v_s10: values.iter().map(|value| value.v_s10).sum(),
        flux_conservation_pass: true,
        plane_pole_pass,
        longitude_wrap_pass,
        output_sha256: format!("sha256:{}", to_hex(&sha256(csv.as_bytes()))),
        photometry_model: "v_s10_scaled_integrated_proxy_v1",
        calibration_status: "experimental-until-independent-validation",
    }
}

fn push_metadata(out: &mut String, key: &str, value: &str) {
    let sanitized = value.replace(['\n', '\r'], " ");
    out.push_str("# ");
    out.push_str(key);
    out.push('=');
    out.push_str(&sanitized);
    out.push('\n');
}

fn ordering_name(ordering: HealpixOrdering) -> &'static str {
    match ordering {
        HealpixOrdering::Ring => "ring",
        HealpixOrdering::Nested => "nested",
    }
}

fn write_output(path: &PathBuf, raw: &str) -> Result<()> {
    let mut out: Box<dyn Write> = if path.as_os_str() == OsStr::new("-") {
        Box::new(BufWriter::new(io::stdout()))
    } else {
        Box::new(BufWriter::new(File::create(path).with_context(|| {
            format!("failed to create output map {}", path.display())
        })?))
    };
    out.write_all(raw.as_bytes())?;
    out.flush()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use nsb::StarlightMap;
    use std::fs;

    #[test]
    fn generated_healpix_map_loads_with_nsb_starlight_map() -> Result<()> {
        let dir = tempfile::tempdir()?;
        let input = dir.path().join("catalogue.csv");
        let output = dir.path().join("map.csv");
        let diagnostics = dir.path().join("map.diagnostics.json");
        fs::write(
            &input,
            "ra_deg,dec_deg,b_mag,v_mag,weight,source_id\n266.4051,-28.936175,10.0,10.0,1.0,gc\n",
        )?;

        run(Args {
            input,
            output: output.clone(),
            nside: 1,
            ordering: OrderingArg::Ring,
            min_v_mag: None,
            max_v_mag: None,
            catalog_name: "test".to_string(),
            catalog_release: Some("fixture".to_string()),
            catalog_license: Some("test-only".to_string()),
            catalog_checksum: None,
            integrated_per_v_s10: S10_V_TO_INTEGRATED_PH_CM2_NS_SR,
            generation_date_utc: "2026-06-21T00:00:00Z".to_string(),
            diagnostics_output: Some(diagnostics.clone()),
            require_science_diagnostics: false,
            allow_empty: false,
        })?;

        let raw = fs::read_to_string(output)?;
        let map = StarlightMap::from_csv_str(&raw, nsb::StarlightProvenance::test_fixture())?;
        assert!(raw.contains("# map_type=healpix"));
        assert!(raw.contains("# calibration_status=Experimental"));
        assert!(raw.contains("# generation_command="));
        assert!(raw.contains("# validation_report="));
        assert!(raw.contains("# photometry_model=v_s10_scaled_integrated_proxy_v1"));
        assert!(raw.contains(HEALPIX_CSV_HEADER));
        assert_eq!(raw.matches(HEALPIX_CSV_HEADER).count(), 1);
        assert_eq!(map.provenance().source_catalogue, "test");
        let diagnostics: serde_json::Value =
            serde_json::from_str(&fs::read_to_string(diagnostics)?)?;
        assert_eq!(diagnostics["schema_version"], 1);
        assert_eq!(diagnostics["sources_used"], 1);
        assert_eq!(diagnostics["expected_pixels"], 12);
        assert!(diagnostics["output_sha256"]
            .as_str()
            .is_some_and(|value| value.starts_with("sha256:")));
        Ok(())
    }

    #[test]
    fn refuses_empty_output_without_explicit_override() -> Result<()> {
        let dir = tempfile::tempdir()?;
        let input = dir.path().join("catalogue.csv");
        let output = dir.path().join("map.csv");
        fs::write(
            &input,
            "ra_deg,dec_deg,b_mag,v_mag\n266.4051,-28.936175,10.0,10.0\n",
        )?;

        let err = run(Args {
            input,
            output,
            nside: 1,
            ordering: OrderingArg::Ring,
            min_v_mag: None,
            max_v_mag: Some(0.0),
            catalog_name: "test".to_string(),
            catalog_release: None,
            catalog_license: None,
            catalog_checksum: None,
            integrated_per_v_s10: S10_V_TO_INTEGRATED_PH_CM2_NS_SR,
            generation_date_utc: "2026-06-21T00:00:00Z".to_string(),
            diagnostics_output: None,
            require_science_diagnostics: false,
            allow_empty: false,
        })
        .expect_err("empty filtered catalogue should fail");

        assert!(err
            .to_string()
            .contains("no stellar catalogue records survived filtering"));
        Ok(())
    }

    #[test]
    fn production_diagnostics_require_complete_catalogue_provenance() {
        let args = Args {
            input: PathBuf::from("unused.csv"),
            output: PathBuf::from("unused-map.csv"),
            nside: 64,
            ordering: OrderingArg::Ring,
            min_v_mag: None,
            max_v_mag: Some(11.5),
            catalog_name: "Tycho-2".to_string(),
            catalog_release: None,
            catalog_license: None,
            catalog_checksum: None,
            integrated_per_v_s10: S10_V_TO_INTEGRATED_PH_CM2_NS_SR,
            generation_date_utc: "2026-06-24T00:00:00Z".to_string(),
            diagnostics_output: None,
            require_science_diagnostics: true,
            allow_empty: false,
        };
        let error = validate_args(&args).expect_err("incomplete provenance must fail closed");
        assert!(error.to_string().contains("--catalog-release"));
    }
}
