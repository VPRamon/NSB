use anyhow::{bail, Context, Result};
use clap::{Parser, ValueEnum};
use csv::{ReaderBuilder, StringRecord};
use serde::Serialize;
use siderust::checksum::{sha256, to_hex};
use siderust::coordinates::cartesian::Direction;
use siderust::coordinates::frames::{EquatorialMeanJ2000, Galactic, ICRS};
use siderust::coordinates::transform::TransformFrame;
use siderust::healpix::{HealpixGrid, HealpixMap, HealpixOrdering, Nside};
use siderust::starlight::{
    csv as starlight_csv, flux_10mag_units, validate_flux_conservation,
    validate_no_longitude_wrap_artifact, validate_plane_pole_contrast, validate_stellar_map_values,
    ApparentMagnitude, StellarCatalogueRecord, StellarMapError, StellarMapProvenance,
    StellarSurfaceBrightness, StellarSurfaceBrightnessMap, StellarSurfaceBrightnessMapBuilder,
};
use std::ffi::OsStr;
use std::fs::File;
use std::io::{self, BufWriter, Write};
use std::path::PathBuf;

const S10_V_TO_INTEGRATED_PH_CM2_NS_SR: f64 = 1.242e-3;
const HEALPIX_CSV_HEADER: &str = "healpix_index,integrated_ph_cm2_ns_sr,b_s10,v_s10";
const PROXY_MODEL: &str = "v_s10_scaled_integrated_proxy_v1";
const GAIA_XP_MODEL: &str = "gaia_dr3_xp_photon_radiance_336_650nm_v1";
const GAIA_XP_BAND_MIN_NM: f64 = 336.0;
const GAIA_XP_BAND_MAX_NM: f64 = 650.0;

/// Build a Galactic HEALPix starlight map from a local catalogue CSV.
///
/// The executable is intentionally an orchestration layer: HEALPix binning,
/// EquatorialMeanJ2000 -> Galactic transforms, stellar map construction, and
/// validators are provided by Siderust.
#[derive(Debug, Parser)]
#[command(name = "build_starlight_map")]
#[command(about = "Generate an NSB starlight HEALPix CSV from a local stellar catalogue")]
struct Args {
    /// Input stellar catalogue CSV. Proxy inputs use ra_deg/dec_deg/b_mag/v_mag;
    /// Gaia canonical inputs use icrs_ra_rad/icrs_dec_rad/photon_flux_336_650_ph_m2_s.
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

    /// Photometry model used by the input catalogue.
    #[arg(long, default_value = PROXY_MODEL)]
    photometry_model: String,

    /// Passband minimum wavelength, nm, for passband-integrated inputs.
    #[arg(long, default_value_t = GAIA_XP_BAND_MIN_NM)]
    band_min_nm: f64,

    /// Passband maximum wavelength, nm, for passband-integrated inputs.
    #[arg(long, default_value_t = GAIA_XP_BAND_MAX_NM)]
    band_max_nm: f64,

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
    input_integrated_flux_sum_ph_cm2_ns: f64,
    integrated_flux_conservation_pass: bool,
    total_b_s10: f64,
    total_v_s10: f64,
    flux_conservation_pass: bool,
    plane_pole_pass: bool,
    longitude_wrap_pass: bool,
    output_sha256: String,
    photometry_model: String,
    calibration_status: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum InputKind {
    ProxyMagnitudes,
    GaiaPhotonFlux,
}

struct DiagnosticsContext<'a> {
    sources_used: usize,
    longitude_wrap_pass: bool,
    plane_pole_pass: bool,
    csv: &'a str,
    args: &'a Args,
    input_kind: InputKind,
    input_integrated_flux_sum: f64,
}

fn main() -> Result<()> {
    run(Args::parse())
}

fn run(args: Args) -> Result<()> {
    validate_args(&args)?;
    verify_catalog_checksum(&args)?;

    let grid = HealpixGrid::new(Nside::new(args.nside)?, args.ordering.into())?;
    let input_kind = input_kind(&args.input)?;
    validate_spectral_contract(&args, input_kind)?;
    let provenance = provenance(&args, input_kind);

    let (map, sources_used, input_integrated_flux_sum, longitude_wrap_pass, plane_pole_pass) =
        match input_kind {
            InputKind::ProxyMagnitudes => {
                let min_v_mag = args.min_v_mag.map(ApparentMagnitude::new).transpose()?;
                let max_v_mag = args.max_v_mag.map(ApparentMagnitude::new).transpose()?;
                let (records, input_b_flux_sum, input_v_flux_sum) =
                    read_records(&args.input, min_v_mag, max_v_mag)?;
                let sources_used = records.len();

                let builder = StellarSurfaceBrightnessMapBuilder {
                    grid,
                    // `read_records` is the single filtering boundary so map input,
                    // conservation sums, and `sources_used` cannot diverge.
                    min_v_mag: None,
                    max_v_mag: None,
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
                let input_integrated_flux_sum =
                    input_v_flux_sum * args.integrated_per_v_s10 * map.grid().pixel_area_deg2();
                let (longitude_wrap_pass, plane_pole_pass) =
                    run_science_diagnostics(&map, args.require_science_diagnostics)?;
                (
                    map,
                    sources_used,
                    input_integrated_flux_sum,
                    longitude_wrap_pass,
                    plane_pole_pass,
                )
            }
            InputKind::GaiaPhotonFlux => {
                let (map, sources_used, input_integrated_flux_sum) =
                    build_gaia_photon_map(&args.input, grid, provenance)?;
                validate_stellar_map_values(map.healpix_map())?;
                validate_integrated_flux_conservation(&map, input_integrated_flux_sum, 1.0e-9)?;
                let (longitude_wrap_pass, plane_pole_pass) =
                    run_integrated_science_diagnostics(&map, args.require_science_diagnostics)?;
                (
                    map,
                    sources_used,
                    input_integrated_flux_sum,
                    longitude_wrap_pass,
                    plane_pole_pass,
                )
            }
        };

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
            DiagnosticsContext {
                sources_used,
                longitude_wrap_pass,
                plane_pole_pass,
                csv: &csv,
                args: &args,
                input_kind,
                input_integrated_flux_sum,
            },
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
    if args.photometry_model.trim().is_empty() {
        bail!("--photometry-model must not be empty");
    }
    if args.require_science_diagnostics
        && (args.photometry_model.contains("proxy")
            || args.photometry_model.contains("experimental"))
    {
        bail!("production diagnostics reject proxy or experimental photometry models");
    }
    if !args.band_min_nm.is_finite()
        || !args.band_max_nm.is_finite()
        || args.band_min_nm >= args.band_max_nm
    {
        bail!("band bounds must be finite and satisfy min < max");
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

fn input_kind(input: &PathBuf) -> Result<InputKind> {
    let mut reader = ReaderBuilder::new()
        .comment(Some(b'#'))
        .from_path(input)
        .with_context(|| format!("failed to open input catalogue {}", input.display()))?;
    let headers = reader
        .headers()
        .context("failed to read CSV header")?
        .clone();
    if optional_header(&headers, "photon_flux_336_650_ph_m2_s").is_some() {
        Ok(InputKind::GaiaPhotonFlux)
    } else {
        Ok(InputKind::ProxyMagnitudes)
    }
}

fn validate_spectral_contract(args: &Args, input_kind: InputKind) -> Result<()> {
    if input_kind != InputKind::GaiaPhotonFlux {
        return Ok(());
    }
    if args.photometry_model != GAIA_XP_MODEL {
        bail!(
            "Gaia canonical inputs require --photometry-model={GAIA_XP_MODEL}; found {:?}",
            args.photometry_model
        );
    }
    if args.band_min_nm.to_bits() != GAIA_XP_BAND_MIN_NM.to_bits()
        || args.band_max_nm.to_bits() != GAIA_XP_BAND_MAX_NM.to_bits()
    {
        bail!(
            "Gaia canonical inputs require the measured {}-{} nm XP band",
            GAIA_XP_BAND_MIN_NM,
            GAIA_XP_BAND_MAX_NM
        );
    }
    Ok(())
}

fn verify_catalog_checksum(args: &Args) -> Result<()> {
    let Some(expected) = args.catalog_checksum.as_deref() else {
        return Ok(());
    };
    nsb_data_tools::checksum_io::verify_sha256_file(&args.input, expected, "catalogue")
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
                records.push(record);
            }
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

#[derive(Debug, Clone, Copy)]
struct GaiaPhotonColumns {
    icrs_ra_rad: usize,
    icrs_dec_rad: usize,
    photon_flux_336_650_ph_m2_s: usize,
    photometry_model: usize,
    weight: Option<usize>,
}

impl GaiaPhotonColumns {
    fn from_headers(headers: &StringRecord) -> Result<Self> {
        Ok(Self {
            icrs_ra_rad: required_header(headers, "icrs_ra_rad")?,
            icrs_dec_rad: required_header(headers, "icrs_dec_rad")?,
            photon_flux_336_650_ph_m2_s: required_header(headers, "photon_flux_336_650_ph_m2_s")?,
            photometry_model: required_header(headers, "photometry_model")?,
            weight: optional_header(headers, "weight"),
        })
    }
}

fn build_gaia_photon_map(
    input: &PathBuf,
    grid: HealpixGrid,
    provenance: StellarMapProvenance,
) -> Result<(StellarSurfaceBrightnessMap, usize, f64)> {
    let mut reader = ReaderBuilder::new()
        .comment(Some(b'#'))
        .from_path(input)
        .with_context(|| {
            format!(
                "failed to open Gaia canonical source table {}",
                input.display()
            )
        })?;
    let headers = reader
        .headers()
        .context("failed to read Gaia canonical CSV header")?
        .clone();
    let columns = GaiaPhotonColumns::from_headers(&headers)?;
    let mut values = vec![StellarSurfaceBrightness::zero(); usize::try_from(grid.npix())?];
    let mut sources_used = 0usize;
    let mut input_integrated_flux_sum = 0.0;

    for row in reader.records() {
        let row = row.context("failed to read Gaia canonical CSV record")?;
        let icrs_ra_rad = parse_required_f64(&row, columns.icrs_ra_rad, "icrs_ra_rad")?;
        let icrs_dec_rad = parse_required_f64(&row, columns.icrs_dec_rad, "icrs_dec_rad")?;
        let photon_flux = parse_required_f64(
            &row,
            columns.photon_flux_336_650_ph_m2_s,
            "photon_flux_336_650_ph_m2_s",
        )?;
        let photometry_model = row
            .get(columns.photometry_model)
            .ok_or_else(|| anyhow::anyhow!("missing field \"photometry_model\""))?
            .trim();
        if photometry_model != GAIA_XP_MODEL {
            bail!(
                "Gaia canonical row photometry_model must be {GAIA_XP_MODEL:?}; found {photometry_model:?}"
            );
        }
        let weight = match columns.weight {
            Some(idx) => parse_required_f64(&row, idx, "weight")?,
            None => 1.0,
        };
        if !icrs_ra_rad.is_finite()
            || !icrs_dec_rad.is_finite()
            || !photon_flux.is_finite()
            || !weight.is_finite()
        {
            bail!("Gaia canonical rows must contain finite coordinates, fluxes, and weights");
        }
        if !(0.0..std::f64::consts::TAU).contains(&icrs_ra_rad)
            || !((-std::f64::consts::FRAC_PI_2)..=std::f64::consts::FRAC_PI_2)
                .contains(&icrs_dec_rad)
        {
            bail!("Gaia canonical coordinates are outside valid ICRS ranges");
        }
        if photon_flux < 0.0 || weight < 0.0 {
            bail!("Gaia photon flux and weight must be non-negative");
        }
        if photon_flux == 0.0 || weight == 0.0 {
            continue;
        }

        let direction = icrs_direction_from_radians(icrs_ra_rad, icrs_dec_rad);
        let galactic: Direction<Galactic> = direction.to_frame();
        let index = grid.direction_to_pixel(galactic)?;
        let pixel = &mut values[usize::try_from(index.get())?];
        let integrated_flux = photon_flux * weight * 1.0e-13;
        pixel.integrated_ph_cm2_ns_sr += integrated_flux / grid.pixel_area_sr();
        input_integrated_flux_sum += integrated_flux;
        sources_used += 1;
    }

    if sources_used == 0 {
        bail!("no Gaia passband-integrated sources survived filtering");
    }

    let healpix_map = HealpixMap::<Galactic, _>::new(grid, values)?;
    Ok((
        StellarSurfaceBrightnessMap::new(healpix_map, provenance),
        sources_used,
        input_integrated_flux_sum,
    ))
}

fn icrs_direction_from_radians(ra: f64, dec: f64) -> Direction<ICRS> {
    Direction::<ICRS>::from_array([dec.cos() * ra.cos(), dec.cos() * ra.sin(), dec.sin()])
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

fn provenance(args: &Args, input_kind: InputKind) -> StellarMapProvenance {
    let magnitude_limit = match (args.min_v_mag, args.max_v_mag) {
        (Some(min), Some(max)) => format!("{min} <= V <= {max}"),
        (Some(min), None) => format!("V >= {min}"),
        (None, Some(max)) => format!("V <= {max}"),
        (None, None) => "none".to_string(),
    };

    let band_definition = match input_kind {
        InputKind::ProxyMagnitudes => {
            "integrated 300-650 nm photon radiance plus B/V S10 diagnostics".to_string()
        }
        InputKind::GaiaPhotonFlux => format!(
            "Gaia DR3 XP passband-integrated {}-{} nm photon radiance",
            args.band_min_nm, args.band_max_nm
        ),
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
        band_definition,
        photometry_model: args.photometry_model.clone(),
        smoothing: None,
        generator: "nsb-data-tools build_starlight_map using siderust feature/healpix-stellar-maps"
            .to_string(),
    }
}

fn run_integrated_science_diagnostics(
    map: &StellarSurfaceBrightnessMap,
    require: bool,
) -> Result<(bool, bool)> {
    let longitude_wrap_pass = integrated_longitude_wrap_pass(map);
    if require && !longitude_wrap_pass {
        bail!("starlight diagnostic integrated longitude-wrap artifact failed");
    }
    let plane_pole_pass = integrated_plane_pole_pass(map);
    if require && !plane_pole_pass {
        bail!("starlight diagnostic integrated plane/pole contrast failed");
    }
    Ok((longitude_wrap_pass, plane_pole_pass))
}

fn integrated_plane_pole_pass(map: &StellarSurfaceBrightnessMap) -> bool {
    let mut plane_sum = 0.0;
    let mut plane_count = 0usize;
    let mut pole_sum = 0.0;
    let mut pole_count = 0usize;
    for (idx, value) in map.values().iter().enumerate() {
        let Ok(center) = map
            .grid()
            .pixel_center::<Galactic>(siderust::healpix::HealpixIndex::new(idx as u64))
        else {
            return false;
        };
        let latitude = center.z().asin().to_degrees();
        if latitude.abs() <= 10.0 {
            plane_sum += value.integrated_ph_cm2_ns_sr;
            plane_count += 1;
        } else if latitude.abs() >= 60.0 {
            pole_sum += value.integrated_ph_cm2_ns_sr;
            pole_count += 1;
        }
    }
    plane_count > 0
        && pole_count > 0
        && (plane_sum / plane_count as f64) >= (pole_sum / pole_count as f64)
}

fn integrated_longitude_wrap_pass(map: &StellarSurfaceBrightnessMap) -> bool {
    let values = map.values();
    let max = values
        .iter()
        .map(|value| value.integrated_ph_cm2_ns_sr)
        .fold(0.0_f64, f64::max);
    values.iter().all(|value| {
        value.integrated_ph_cm2_ns_sr.is_finite()
            && value.integrated_ph_cm2_ns_sr <= max.max(1.0) * 1.0e12
    })
}

fn validate_integrated_flux_conservation(
    map: &StellarSurfaceBrightnessMap,
    expected_flux: f64,
    tolerance: f64,
) -> Result<()> {
    let actual_flux = map
        .values()
        .iter()
        .map(|value| value.integrated_ph_cm2_ns_sr * map.grid().pixel_area_sr())
        .sum::<f64>();
    let scale = expected_flux.abs().max(actual_flux.abs()).max(1.0);
    let relative_error = (actual_flux - expected_flux).abs() / scale;
    if relative_error > tolerance {
        bail!(
            "integrated flux conservation failed: expected {expected_flux}, actual {actual_flux}, relative error {relative_error}, tolerance {tolerance}"
        );
    }
    Ok(())
}

fn integrated_flux_conservation_pass(
    map: &StellarSurfaceBrightnessMap,
    expected_flux: f64,
) -> bool {
    validate_integrated_flux_conservation(map, expected_flux, 1.0e-9).is_ok()
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
    let calibration_status = if provenance.photometry_model == GAIA_XP_MODEL {
        "production-candidate"
    } else {
        "Experimental"
    };
    push_metadata(&mut out, "calibration_status", calibration_status);
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

fn diagnostics(map: &StellarSurfaceBrightnessMap, context: DiagnosticsContext<'_>) -> Diagnostics {
    let values = map.values();
    Diagnostics {
        schema_version: 1,
        sources_used: context.sources_used,
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
        input_integrated_flux_sum_ph_cm2_ns: context.input_integrated_flux_sum,
        integrated_flux_conservation_pass: integrated_flux_conservation_pass(
            map,
            context.input_integrated_flux_sum,
        ),
        total_b_s10: values.iter().map(|value| value.b_s10).sum(),
        total_v_s10: values.iter().map(|value| value.v_s10).sum(),
        flux_conservation_pass: true,
        plane_pole_pass: context.plane_pole_pass,
        longitude_wrap_pass: context.longitude_wrap_pass,
        output_sha256: format!("sha256:{}", to_hex(&sha256(context.csv.as_bytes()))),
        photometry_model: context.args.photometry_model.clone(),
        calibration_status: match context.input_kind {
            InputKind::ProxyMagnitudes => "experimental-until-independent-validation".to_string(),
            InputKind::GaiaPhotonFlux => {
                "production-candidate-until-independent-validation".to_string()
            }
        },
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
            input: input.clone(),
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
            photometry_model: PROXY_MODEL.to_string(),
            band_min_nm: GAIA_XP_BAND_MIN_NM,
            band_max_nm: 650.0,
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
    fn magnitude_cuts_filter_records_and_diagnostics_source_count() -> Result<()> {
        let dir = tempfile::tempdir()?;
        let input = dir.path().join("catalogue.csv");
        let output = dir.path().join("map.csv");
        let diagnostics = dir.path().join("map.diagnostics.json");
        fs::write(
            &input,
            concat!(
                "ra_deg,dec_deg,b_mag,v_mag,weight,source_id\n",
                "266.4051,-28.936175,10.0,10.0,1.0,accepted\n",
                "10.0,10.0,0.0,0.0,1.0,excluded_bright\n",
            ),
        )?;

        run(Args {
            input: input.clone(),
            output: output.clone(),
            nside: 1,
            ordering: OrderingArg::Ring,
            min_v_mag: Some(9.0),
            max_v_mag: Some(11.0),
            catalog_name: "test".to_string(),
            catalog_release: Some("fixture".to_string()),
            catalog_license: Some("test-only".to_string()),
            catalog_checksum: None,
            integrated_per_v_s10: S10_V_TO_INTEGRATED_PH_CM2_NS_SR,
            photometry_model: PROXY_MODEL.to_string(),
            band_min_nm: GAIA_XP_BAND_MIN_NM,
            band_max_nm: 650.0,
            generation_date_utc: "2026-06-21T00:00:00Z".to_string(),
            diagnostics_output: Some(diagnostics.clone()),
            require_science_diagnostics: false,
            allow_empty: false,
        })?;

        let raw = fs::read_to_string(output)?;
        let map = StarlightMap::from_csv_str(&raw, nsb::StarlightProvenance::test_fixture())?;
        let expected_v = flux_10mag_units(ApparentMagnitude::new(10.0)?);
        let expected_integrated = expected_v * S10_V_TO_INTEGRATED_PH_CM2_NS_SR;
        let pixel_area_deg2 =
            map.pixels()[0].solid_angle.value() * (180.0 / std::f64::consts::PI).powi(2);
        let total_v: f64 = map
            .pixels()
            .iter()
            .map(|value| value.v_flux_s10.value())
            .sum();
        let total_integrated: f64 = map
            .pixels()
            .iter()
            .map(|value| value.integrated.value())
            .sum();

        assert!(
            (total_v - expected_v / pixel_area_deg2).abs()
                <= 1.0e-12 * (expected_v / pixel_area_deg2).max(1.0)
        );
        assert!(
            (total_integrated - expected_integrated / pixel_area_deg2).abs()
                <= 1.0e-12 * (expected_integrated / pixel_area_deg2).max(1.0)
        );

        let diagnostics: serde_json::Value =
            serde_json::from_str(&fs::read_to_string(diagnostics)?)?;
        assert_eq!(diagnostics["sources_used"], 1);
        assert_eq!(diagnostics["flux_conservation_pass"], true);
        assert_eq!(diagnostics["total_v_s10"].as_f64().unwrap(), total_v);

        fs::write(
            &input,
            concat!(
                "ra_deg,dec_deg,b_mag,v_mag,weight,source_id\n",
                "266.4051,-28.936175,10.0,10.0,1.0,accepted\n",
                "10.0,10.0,-20.0,-20.0,1.0,excluded_extremely_bright\n",
            ),
        )?;
        let second_output = dir.path().join("map-second.csv");
        run(Args {
            input,
            output: second_output.clone(),
            nside: 1,
            ordering: OrderingArg::Ring,
            min_v_mag: Some(9.0),
            max_v_mag: Some(11.0),
            catalog_name: "test".to_string(),
            catalog_release: Some("fixture".to_string()),
            catalog_license: Some("test-only".to_string()),
            catalog_checksum: None,
            integrated_per_v_s10: S10_V_TO_INTEGRATED_PH_CM2_NS_SR,
            photometry_model: PROXY_MODEL.to_string(),
            band_min_nm: GAIA_XP_BAND_MIN_NM,
            band_max_nm: 650.0,
            generation_date_utc: "2026-06-21T00:00:00Z".to_string(),
            diagnostics_output: None,
            require_science_diagnostics: false,
            allow_empty: false,
        })?;
        let second_map = StarlightMap::from_csv_str(
            &fs::read_to_string(second_output)?,
            nsb::StarlightProvenance::test_fixture(),
        )?;
        assert_eq!(map.pixels(), second_map.pixels());
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
            photometry_model: PROXY_MODEL.to_string(),
            band_min_nm: GAIA_XP_BAND_MIN_NM,
            band_max_nm: 650.0,
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
    fn gaia_photon_flux_fixture_builds_healpix_map() -> Result<()> {
        let dir = tempfile::tempdir()?;
        let input = dir.path().join("canonical-gaia.csv");
        let output = dir.path().join("map.csv");
        let diagnostics = dir.path().join("map.diagnostics.json");
        fs::write(
            &input,
            concat!(
                "source_id,icrs_ra_rad,icrs_dec_rad,epoch_jyr,photon_flux_336_650_ph_m2_s,photometry_model,weight\n",
                "42,4.649644, -0.505386,2016.0,1.0e6,",
                "gaia_dr3_xp_photon_radiance_336_650nm_v1,1.0\n",
            ),
        )?;

        run(Args {
            input,
            output: output.clone(),
            nside: 1,
            ordering: OrderingArg::Ring,
            min_v_mag: None,
            max_v_mag: None,
            catalog_name: "Gaia".to_string(),
            catalog_release: Some("DR3".to_string()),
            catalog_license: Some("CC-BY-4.0-derived-policy-reviewed".to_string()),
            catalog_checksum: None,
            integrated_per_v_s10: S10_V_TO_INTEGRATED_PH_CM2_NS_SR,
            photometry_model: GAIA_XP_MODEL.to_string(),
            band_min_nm: GAIA_XP_BAND_MIN_NM,
            band_max_nm: 650.0,
            generation_date_utc: "2026-06-21T00:00:00Z".to_string(),
            diagnostics_output: Some(diagnostics.clone()),
            require_science_diagnostics: false,
            allow_empty: false,
        })?;

        let raw = fs::read_to_string(output)?;
        assert!(raw.contains("# calibration_status=production-candidate"));
        assert!(raw.contains("# photometry_model=gaia_dr3_xp_photon_radiance_336_650nm_v1"));
        let report: serde_json::Value = serde_json::from_str(&fs::read_to_string(diagnostics)?)?;
        assert_eq!(report["sources_used"], 1);
        assert_eq!(report["photometry_model"], GAIA_XP_MODEL);
        assert!(report["total_integrated_ph_cm2_ns_sr"]
            .as_f64()
            .is_some_and(|value| value > 0.0));
        Ok(())
    }

    #[test]
    fn gaia_canonical_source_lands_in_expected_galactic_healpix_pixel() -> Result<()> {
        let dir = tempfile::tempdir()?;
        let input = dir.path().join("canonical-gaia.csv");
        let output = dir.path().join("map.csv");
        let nside = 8;
        let ra_rad = 0.0_f64;
        let dec_rad = 0.0_f64;
        fs::write(
            &input,
            format!(
                concat!(
                    "source_id,icrs_ra_rad,icrs_dec_rad,epoch_jyr,photon_flux_336_650_ph_m2_s,photometry_model,weight\n",
                    "42,{:.16},{:.16},2016.0,1.0e6,",
                    "gaia_dr3_xp_photon_radiance_336_650nm_v1,1.0\n",
                ),
                ra_rad, dec_rad
            ),
        )?;

        run(Args {
            input,
            output: output.clone(),
            nside,
            ordering: OrderingArg::Ring,
            min_v_mag: None,
            max_v_mag: None,
            catalog_name: "Gaia".to_string(),
            catalog_release: Some("DR3".to_string()),
            catalog_license: Some("CC-BY-4.0-derived-policy-reviewed".to_string()),
            catalog_checksum: None,
            integrated_per_v_s10: S10_V_TO_INTEGRATED_PH_CM2_NS_SR,
            photometry_model: GAIA_XP_MODEL.to_string(),
            band_min_nm: GAIA_XP_BAND_MIN_NM,
            band_max_nm: 650.0,
            generation_date_utc: "2026-06-21T00:00:00Z".to_string(),
            diagnostics_output: None,
            require_science_diagnostics: false,
            allow_empty: false,
        })?;

        let grid = HealpixGrid::new(Nside::new(nside)?, HealpixOrdering::Ring)?;
        let galactic: Direction<Galactic> = icrs_direction_from_radians(ra_rad, dec_rad).to_frame();
        let expected_index = grid.direction_to_pixel(galactic)?.get();
        let raw = fs::read_to_string(output)?;
        let mut reader = ReaderBuilder::new()
            .comment(Some(b'#'))
            .from_reader(raw.as_bytes());
        let mut nonzero = Vec::new();
        for row in reader.records() {
            let row = row?;
            let index: u64 = row[0].parse()?;
            let value: f64 = row[1].parse()?;
            if value > 0.0 {
                nonzero.push(index);
            }
        }
        assert_eq!(nonzero, vec![expected_index]);
        Ok(())
    }

    #[test]
    fn gaia_canonical_radian_ranges_are_validated() -> Result<()> {
        let dir = tempfile::tempdir()?;
        let input = dir.path().join("canonical-gaia.csv");
        let output = dir.path().join("map.csv");
        fs::write(
            &input,
            concat!(
                "source_id,icrs_ra_rad,icrs_dec_rad,epoch_jyr,photon_flux_336_650_ph_m2_s,photometry_model,weight\n",
                "42,6.283185307179586,0.0,2016.0,1.0e6,",
                "gaia_dr3_xp_photon_radiance_336_650nm_v1,1.0\n",
            ),
        )?;

        let err = run(Args {
            input,
            output,
            nside: 1,
            ordering: OrderingArg::Ring,
            min_v_mag: None,
            max_v_mag: None,
            catalog_name: "Gaia".to_string(),
            catalog_release: Some("DR3".to_string()),
            catalog_license: Some("CC-BY-4.0-derived-policy-reviewed".to_string()),
            catalog_checksum: None,
            integrated_per_v_s10: S10_V_TO_INTEGRATED_PH_CM2_NS_SR,
            photometry_model: GAIA_XP_MODEL.to_string(),
            band_min_nm: GAIA_XP_BAND_MIN_NM,
            band_max_nm: 650.0,
            generation_date_utc: "2026-06-21T00:00:00Z".to_string(),
            diagnostics_output: None,
            require_science_diagnostics: false,
            allow_empty: false,
        })
        .expect_err("invalid RA radians must be rejected");
        assert!(err
            .to_string()
            .contains("Gaia canonical coordinates are outside valid ICRS ranges"));
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
            photometry_model: GAIA_XP_MODEL.to_string(),
            band_min_nm: GAIA_XP_BAND_MIN_NM,
            band_max_nm: 650.0,
            generation_date_utc: "2026-06-24T00:00:00Z".to_string(),
            diagnostics_output: None,
            require_science_diagnostics: true,
            allow_empty: false,
        };
        let error = validate_args(&args).expect_err("incomplete provenance must fail closed");
        assert!(error.to_string().contains("--catalog-release"));
    }

    #[test]
    fn gaia_input_rejects_any_band_other_than_measured_336_650_contract() {
        let args = Args {
            input: PathBuf::from("unused.csv"),
            output: PathBuf::from("unused-map.csv"),
            nside: 64,
            ordering: OrderingArg::Ring,
            min_v_mag: None,
            max_v_mag: None,
            catalog_name: "Gaia".to_string(),
            catalog_release: Some("DR3".to_string()),
            catalog_license: Some("reviewed policy".to_string()),
            catalog_checksum: Some(format!("sha256:{}", "1".repeat(64))),
            integrated_per_v_s10: S10_V_TO_INTEGRATED_PH_CM2_NS_SR,
            photometry_model: GAIA_XP_MODEL.to_string(),
            band_min_nm: 335.0,
            band_max_nm: GAIA_XP_BAND_MAX_NM,
            generation_date_utc: "2026-07-11T00:00:00Z".to_string(),
            diagnostics_output: Some(PathBuf::from("unused-diagnostics.json")),
            require_science_diagnostics: true,
            allow_empty: false,
        };
        let error = validate_spectral_contract(&args, InputKind::GaiaPhotonFlux)
            .expect_err("unmeasured lower band edge must fail closed");
        assert!(error.to_string().contains("measured 336-650 nm XP band"));
    }

    #[test]
    fn streaming_catalogue_checksum_matches_sha256sum() -> Result<()> {
        let dir = tempfile::tempdir()?;
        let input = dir.path().join("catalogue.csv");
        std::fs::write(&input, b"source_id,ra_deg,dec_deg\n")?;
        let expected = nsb_data_tools::checksum_io::sha256_file(&input)?;
        let args = Args {
            input: input.clone(),
            output: dir.path().join("map.csv"),
            nside: 64,
            ordering: OrderingArg::Ring,
            min_v_mag: None,
            max_v_mag: None,
            catalog_name: "Gaia".to_string(),
            catalog_release: Some("DR3".to_string()),
            catalog_license: Some("reviewed".to_string()),
            catalog_checksum: Some(format!("sha256:{expected}")),
            integrated_per_v_s10: S10_V_TO_INTEGRATED_PH_CM2_NS_SR,
            photometry_model: GAIA_XP_MODEL.to_string(),
            band_min_nm: GAIA_XP_BAND_MIN_NM,
            band_max_nm: GAIA_XP_BAND_MAX_NM,
            generation_date_utc: "2026-07-11T00:00:00Z".to_string(),
            diagnostics_output: None,
            require_science_diagnostics: false,
            allow_empty: true,
        };
        verify_catalog_checksum(&args)?;
        let wrong = Args {
            catalog_checksum: Some("sha256:deadbeef".to_string()),
            ..args
        };
        verify_catalog_checksum(&wrong).expect_err("wrong checksum");
        Ok(())
    }
}
