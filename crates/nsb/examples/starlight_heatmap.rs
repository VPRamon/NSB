// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Vallés Puig, Ramon
//! Render the generated Starlight candidate as a Galactic Mollweide heatmap.
//!
//! This is maintainer/diagnostic tooling for the canonical sparse NESTED
//! HEALPix candidate. It does not alter or resample the scientific data product.
//! Omitted candidate pixels are rendered as physical zero flux, matching the
//! candidate-map contract.
//!
//! Example:
//!
//! ```text
//! cargo run --release --locked -p nsb --example starlight_heatmap -- \
//!   --map crates/nsb/data/starlight_nside128.csv \
//!   --output starlight_heatmap.png
//! ```

use csv::ReaderBuilder;
use plotters::coord::Shift;
use plotters::prelude::*;
use sha2::{Digest, Sha256};
use siderust::coordinates::frames::Galactic;
use siderust::healpix::{HealpixGrid, HealpixIndex, HealpixOrdering, Nside};
use std::collections::BTreeMap;
use std::env;
use std::error::Error;
use std::fs;
use std::fs::File;
use std::io::{self, Read};
use std::path::{Path, PathBuf};

const DEFAULT_MAP: &str = "crates/nsb/data/starlight_nside128.csv";
const DEFAULT_OUTPUT: &str = "starlight_heatmap.png";
const IMAGE_WIDTH: u32 = 1800;
const IMAGE_HEIGHT: u32 = 920;
const SKY_LEFT: f64 = 250.0;
const SKY_TOP: f64 = 105.0;
const SKY_WIDTH: f64 = 1300.0;
const SKY_HEIGHT: f64 = 650.0;
const SQRT_2: f64 = std::f64::consts::SQRT_2;

type AppResult<T> = Result<T, Box<dyn Error + Send + Sync>>;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Normalization {
    Log,
    Linear,
}

impl Normalization {
    fn parse(value: &str) -> AppResult<Self> {
        match value {
            "log" => Ok(Self::Log),
            "linear" => Ok(Self::Linear),
            other => Err(invalid_input(format!(
                "unsupported --norm {other:?}; expected log or linear"
            ))
            .into()),
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::Log => "log scale",
            Self::Linear => "linear scale",
        }
    }
}

#[derive(Debug)]
struct Args {
    map: PathBuf,
    output: PathBuf,
    normalization: Normalization,
}

#[derive(Debug)]
struct StarlightMapData {
    nside: u32,
    pixels: Vec<(u64, f64)>,
}

#[derive(Debug, Clone, Copy)]
struct FluxRange {
    min: f64,
    max: f64,
}

fn main() {
    if let Err(error) = run() {
        eprintln!("error: {error}");
        std::process::exit(2);
    }
}

fn run() -> AppResult<()> {
    let Some(args) = parse_args(env::args().skip(1))? else {
        print_help();
        return Ok(());
    };

    let raw = fs::read_to_string(&args.map)?;
    let data = parse_starlight_map(&raw)?;
    let checksum = sha256(&args.map)?;
    let range = flux_range(&data, args.normalization)?;

    render_heatmap(&args.output, &data, range, args.normalization, &checksum)?;

    println!(
        "wrote {} (nside={}, occupied={}/{}, flux {:.6e}..{:.6e}, sha256={})",
        args.output.display(),
        data.nside,
        data.pixels.len(),
        12_u64 * u64::from(data.nside) * u64::from(data.nside),
        range.min,
        range.max,
        checksum,
    );

    Ok(())
}

fn parse_args<I>(args: I) -> AppResult<Option<Args>>
where
    I: IntoIterator<Item = String>,
{
    let mut map = PathBuf::from(DEFAULT_MAP);
    let mut output = PathBuf::from(DEFAULT_OUTPUT);
    let mut normalization = Normalization::Log;
    let mut iter = args.into_iter();

    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "-h" | "--help" => return Ok(None),
            "--map" => map = PathBuf::from(next_value(&mut iter, "--map")?),
            "--output" => output = PathBuf::from(next_value(&mut iter, "--output")?),
            "--norm" => normalization = Normalization::parse(&next_value(&mut iter, "--norm")?)?,
            other => {
                return Err(invalid_input(format!(
                    "unknown argument {other:?}; use --help for usage"
                ))
                .into());
            }
        }
    }

    if !map.is_file() {
        return Err(
            invalid_input(format!("candidate map does not exist: {}", map.display())).into(),
        );
    }
    if output.as_os_str().is_empty() {
        return Err(invalid_input("--output path must not be empty").into());
    }
    let is_png = output
        .extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| extension.eq_ignore_ascii_case("png"));
    if !is_png {
        return Err(invalid_input(format!(
            "--output must have a .png extension, got {}",
            output.display()
        ))
        .into());
    }

    Ok(Some(Args {
        map,
        output,
        normalization,
    }))
}

fn next_value<I>(iter: &mut I, flag: &str) -> AppResult<String>
where
    I: Iterator<Item = String>,
{
    iter.next()
        .ok_or_else(|| invalid_input(format!("missing value after {flag}")))
        .map_err(Into::into)
}

fn print_help() {
    println!(
        "starlight_heatmap - render the generated Starlight HEALPix candidate\n\n\
Usage:\n  cargo run --release --locked -p nsb --example starlight_heatmap -- [OPTIONS]\n\n\
Options:\n  --map <candidate.csv>   Candidate map [default: {DEFAULT_MAP}]\n  --output <path.png>     Output PNG [default: {DEFAULT_OUTPUT}]\n  --norm <log|linear>     Colour normalization [default: log]\n  -h, --help              Show this help\n\n\
Projection: Galactic Mollweide with l=0° at centre and longitude increasing left.\n\
Quantity: integrated photon flux per HEALPix pixel, ph m^-2 s^-1."
    );
}

fn parse_starlight_map(raw: &str) -> AppResult<StarlightMapData> {
    let metadata = parse_metadata(raw);
    require_metadata(&metadata, "map_type", "healpix")?;
    require_metadata(&metadata, "coordinate_frame", "galactic")?;
    require_metadata(&metadata, "ordering", "nested")?;
    require_metadata(&metadata, "representation", "sparse")?;
    require_metadata(
        &metadata,
        "omitted_pixel_semantics",
        "zero_flux_and_source_counts",
    )?;
    require_metadata(&metadata, "flux_quantity", "integrated_per_pixel")?;
    require_metadata(&metadata, "flux_unit", "ph_m-2_s-1")?;

    let nside = metadata
        .get("nside")
        .ok_or_else(|| invalid_input("missing # nside metadata"))?
        .parse::<u32>()
        .map_err(|error| invalid_input(format!("invalid # nside metadata: {error}")))?;
    if nside == 0 || !nside.is_power_of_two() {
        return Err(invalid_input(format!(
            "candidate nside must be a positive power of two, got {nside}"
        ))
        .into());
    }

    let nside_typed = Nside::new(nside)?;
    let nested_grid = HealpixGrid::new(nside_typed, HealpixOrdering::Nested)?;
    let pixel_count = nested_grid.npix();

    let mut reader = ReaderBuilder::new()
        .has_headers(true)
        .comment(Some(b'#'))
        .trim(csv::Trim::All)
        .from_reader(raw.as_bytes());

    let headers = reader.headers()?.clone();
    let pixel_column = headers
        .iter()
        .position(|field| field == "pixel")
        .ok_or_else(|| invalid_input("candidate CSV is missing the pixel column"))?;
    let flux_column = headers
        .iter()
        .position(|field| field == "flux_ph_m2_s")
        .ok_or_else(|| invalid_input("candidate CSV is missing the flux_ph_m2_s column"))?;

    let mut pixels = Vec::new();
    let mut previous_pixel = None;
    for (row_index, record) in reader.records().enumerate() {
        let row_number = row_index + 2;
        let record = record?;
        let pixel = record
            .get(pixel_column)
            .ok_or_else(|| invalid_input(format!("row {row_number} is missing pixel")))?
            .parse::<u64>()
            .map_err(|error| {
                invalid_input(format!("row {row_number} has invalid pixel: {error}"))
            })?;
        let flux = record
            .get(flux_column)
            .ok_or_else(|| invalid_input(format!("row {row_number} is missing flux_ph_m2_s")))?
            .parse::<f64>()
            .map_err(|error| {
                invalid_input(format!(
                    "row {row_number} has invalid flux_ph_m2_s: {error}"
                ))
            })?;

        if previous_pixel.is_some_and(|previous| pixel <= previous) {
            return Err(invalid_input(format!(
                "row {row_number}: pixels must be strictly increasing"
            ))
            .into());
        }
        if pixel >= pixel_count {
            return Err(invalid_input(format!(
                "row {row_number}: pixel {pixel} is outside nside={nside} domain [0, {pixel_count})"
            ))
            .into());
        }
        if !flux.is_finite() || flux < 0.0 {
            return Err(invalid_input(format!(
                "row {row_number}: flux_ph_m2_s must be finite and non-negative"
            ))
            .into());
        }

        pixels.push((pixel, flux));
        previous_pixel = Some(pixel);
    }

    if pixels.is_empty() {
        return Err(invalid_input("candidate map has no occupied pixels").into());
    }

    Ok(StarlightMapData { nside, pixels })
}

fn parse_metadata(raw: &str) -> BTreeMap<String, String> {
    let mut metadata = BTreeMap::new();
    for line in raw.lines() {
        let stripped = line.trim();
        if !stripped.starts_with('#') {
            continue;
        }
        let body = stripped.trim_start_matches('#').trim();
        if let Some((key, value)) = body.split_once('=') {
            metadata.insert(key.trim().to_string(), value.trim().to_string());
        }
    }
    metadata
}

fn require_metadata(
    metadata: &BTreeMap<String, String>,
    key: &str,
    expected: &str,
) -> AppResult<()> {
    let actual = metadata.get(key).map(String::as_str);
    if actual != Some(expected) {
        return Err(invalid_input(format!(
            "expected metadata {key}={expected:?}, found {actual:?}"
        ))
        .into());
    }
    Ok(())
}

fn flux_range(data: &StarlightMapData, normalization: Normalization) -> AppResult<FluxRange> {
    let max = data
        .pixels
        .iter()
        .map(|(_, flux)| *flux)
        .fold(f64::NEG_INFINITY, f64::max);
    let min_positive = data
        .pixels
        .iter()
        .map(|(_, flux)| *flux)
        .filter(|flux| *flux > 0.0)
        .fold(f64::INFINITY, f64::min);

    if !max.is_finite() || max <= 0.0 || !min_positive.is_finite() {
        return Err(invalid_input("candidate map has no finite positive flux").into());
    }

    Ok(FluxRange {
        min: match normalization {
            Normalization::Log => min_positive,
            Normalization::Linear => 0.0,
        },
        max,
    })
}

fn render_heatmap(
    output: &Path,
    data: &StarlightMapData,
    range: FluxRange,
    normalization: Normalization,
    checksum: &str,
) -> AppResult<()> {
    if let Some(parent) = output
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
    {
        fs::create_dir_all(parent)?;
    }

    let root = BitMapBackend::new(output, (IMAGE_WIDTH, IMAGE_HEIGHT)).into_drawing_area();
    root.fill(&WHITE)?;

    root.draw(&Text::new(
        format!(
            "NSB Starlight nside={} — integrated flux (Galactic, {}…)",
            data.nside,
            &checksum[..8]
        ),
        (485, 55),
        ("sans-serif", 30).into_font(),
    ))?;

    let boundary = mollweide_boundary();
    root.draw(&Polygon::new(boundary.clone(), viridis(0.0).filled()))?;

    let nside = Nside::new(data.nside)?;
    let nested_grid = HealpixGrid::new(nside, HealpixOrdering::Nested)?;
    let ring_grid = HealpixGrid::new(nside, HealpixOrdering::Ring)?;
    debug_assert_eq!(nested_grid.npix(), ring_grid.npix());

    let pixel_size_deg = (4.0 * std::f64::consts::PI / nested_grid.npix() as f64)
        .sqrt()
        .to_degrees();
    let marker_radius = ((SKY_WIDTH / 360.0) * pixel_size_deg * 0.75)
        .ceil()
        .clamp(1.0, 8.0) as i32;

    for &(pixel, flux) in &data.pixels {
        let ring_index = nested_to_ring(data.nside, pixel);
        let direction =
            ring_grid.pixel_center_spherical::<Galactic>(HealpixIndex::new(ring_index))?;
        let longitude_deg = direction.l().value();
        let latitude_deg = direction.b().value();
        let point = sky_to_pixel(longitude_deg, latitude_deg);
        let color = viridis(normalize_flux(flux, range, normalization));
        root.draw(&Rectangle::new(
            [
                (point.0 - marker_radius, point.1 - marker_radius),
                (point.0 + marker_radius, point.1 + marker_radius),
            ],
            color.filled(),
        ))?;
    }

    draw_graticule(&root)?;
    root.draw(&PathElement::new(
        closed_path(&boundary),
        ShapeStyle::from(&BLACK).stroke_width(2),
    ))?;
    root.draw(&Text::new(
        "Galactic",
        (1430, 710),
        ("sans-serif", 24).into_font().style(FontStyle::Bold),
    ))?;
    draw_colorbar(&root, range, normalization)?;

    root.present()?;
    Ok(())
}

fn draw_graticule(root: &DrawingArea<BitMapBackend<'_>, Shift>) -> AppResult<()> {
    let style = ShapeStyle::from(&BLACK.mix(0.16)).stroke_width(1);

    for latitude in [-60.0, -30.0, 0.0, 30.0, 60.0] {
        let points = (-180..=180)
            .step_by(2)
            .map(|longitude| sky_to_pixel(f64::from(longitude), latitude))
            .collect::<Vec<_>>();
        root.draw(&PathElement::new(points, style))?;
    }

    for longitude in [-120.0, -60.0, 0.0, 60.0, 120.0] {
        let points = (-89..=89)
            .map(|latitude| sky_to_pixel(longitude, f64::from(latitude)))
            .collect::<Vec<_>>();
        root.draw(&PathElement::new(points, style))?;
    }

    Ok(())
}

fn draw_colorbar(
    root: &DrawingArea<BitMapBackend<'_>, Shift>,
    range: FluxRange,
    normalization: Normalization,
) -> AppResult<()> {
    const X0: i32 = 590;
    const X1: i32 = 1210;
    const Y0: i32 = 800;
    const Y1: i32 = 830;
    let width = X1 - X0;

    for x in 0..width {
        let t = f64::from(x) / f64::from(width - 1);
        root.draw(&Rectangle::new(
            [(X0 + x, Y0), (X0 + x + 1, Y1)],
            viridis(t).filled(),
        ))?;
    }
    root.draw(&Rectangle::new(
        [(X0, Y0), (X1, Y1)],
        ShapeStyle::from(&BLACK).stroke_width(1),
    ))?;

    root.draw(&Text::new(
        format!("{:.6e}", range.min),
        (X0 - 38, Y1 + 30),
        ("sans-serif", 19).into_font(),
    ))?;
    root.draw(&Text::new(
        format!("{:.6e}", range.max),
        (X1 - 72, Y1 + 30),
        ("sans-serif", 19).into_font(),
    ))?;
    root.draw(&Text::new(
        "ph m⁻² s⁻¹",
        (830, Y1 + 30),
        ("sans-serif", 22).into_font(),
    ))?;
    root.draw(&Text::new(
        normalization.label(),
        (845, Y1 + 58),
        ("sans-serif", 16).into_font(),
    ))?;

    Ok(())
}

fn normalize_flux(value: f64, range: FluxRange, normalization: Normalization) -> f64 {
    match normalization {
        Normalization::Linear => {
            if range.max <= range.min {
                0.5
            } else {
                ((value - range.min) / (range.max - range.min)).clamp(0.0, 1.0)
            }
        }
        Normalization::Log => {
            if value <= 0.0 || range.max <= range.min {
                return 0.0;
            }
            ((value.ln() - range.min.ln()) / (range.max.ln() - range.min.ln())).clamp(0.0, 1.0)
        }
    }
}

fn viridis(t: f64) -> RGBColor {
    const STOPS: &[(f64, (u8, u8, u8))] = &[
        (0.00, (68, 1, 84)),
        (0.25, (59, 82, 139)),
        (0.50, (33, 145, 140)),
        (0.75, (94, 201, 98)),
        (1.00, (253, 231, 37)),
    ];

    let t = t.clamp(0.0, 1.0);
    for window in STOPS.windows(2) {
        let (t0, c0) = window[0];
        let (t1, c1) = window[1];
        if t <= t1 {
            let local = (t - t0) / (t1 - t0);
            return RGBColor(
                lerp_u8(c0.0, c1.0, local),
                lerp_u8(c0.1, c1.1, local),
                lerp_u8(c0.2, c1.2, local),
            );
        }
    }
    RGBColor(253, 231, 37)
}

fn lerp_u8(a: u8, b: u8, t: f64) -> u8 {
    (f64::from(a) + t * (f64::from(b) - f64::from(a))).round() as u8
}

fn sky_to_pixel(longitude_deg: f64, latitude_deg: f64) -> (i32, i32) {
    let (x, y) = mollweide_project(longitude_deg, latitude_deg);
    let normalized_x = (x + 2.0 * SQRT_2) / (4.0 * SQRT_2);
    let normalized_y = (SQRT_2 - y) / (2.0 * SQRT_2);
    (
        (SKY_LEFT + normalized_x * SKY_WIDTH).round() as i32,
        (SKY_TOP + normalized_y * SKY_HEIGHT).round() as i32,
    )
}

fn mollweide_project(longitude_deg: f64, latitude_deg: f64) -> (f64, f64) {
    let wrapped_longitude = (longitude_deg + 180.0).rem_euclid(360.0) - 180.0;
    // Astronomical convention: Galactic longitude increases to the left.
    let longitude = -wrapped_longitude.to_radians();
    let latitude = latitude_deg.clamp(-90.0, 90.0).to_radians();

    let theta = if (latitude.abs() - std::f64::consts::FRAC_PI_2).abs() < 1.0e-12 {
        latitude.signum() * std::f64::consts::FRAC_PI_2
    } else {
        let target = std::f64::consts::PI * latitude.sin();
        let mut theta = latitude;
        for _ in 0..12 {
            let twice = 2.0 * theta;
            let numerator = twice + twice.sin() - target;
            let denominator = 2.0 + 2.0 * twice.cos();
            theta -= numerator / denominator;
        }
        theta
    };

    (
        2.0 * SQRT_2 / std::f64::consts::PI * longitude * theta.cos(),
        SQRT_2 * theta.sin(),
    )
}

fn mollweide_boundary() -> Vec<(i32, i32)> {
    (0..=360)
        .map(|degree| {
            let angle = f64::from(degree).to_radians();
            let x = 2.0 * SQRT_2 * angle.cos();
            let y = SQRT_2 * angle.sin();
            let normalized_x = (x + 2.0 * SQRT_2) / (4.0 * SQRT_2);
            let normalized_y = (SQRT_2 - y) / (2.0 * SQRT_2);
            (
                (SKY_LEFT + normalized_x * SKY_WIDTH).round() as i32,
                (SKY_TOP + normalized_y * SKY_HEIGHT).round() as i32,
            )
        })
        .collect()
}

fn closed_path(points: &[(i32, i32)]) -> Vec<(i32, i32)> {
    let mut closed = points.to_vec();
    if let Some(first) = points.first().copied() {
        closed.push(first);
    }
    closed
}

/// Integer NESTED -> RING conversion from the HEALPix reference layout.
///
/// Siderust deliberately supports pixel centres through RING geometry; the
/// candidate is NESTED, so the example converts only the index ordering before
/// asking Siderust for the typed Galactic pixel centre.
fn nested_to_ring(nside: u32, ipnest: u64) -> u64 {
    debug_assert!(nside.is_power_of_two() && nside > 0);
    let nside = i64::from(nside);
    let npface = nside * nside;
    let npix = 12 * npface;
    let ipnest = i64::try_from(ipnest).expect("validated nested index fits i64");
    debug_assert!((0..npix).contains(&ipnest));

    const JRLL: [i64; 12] = [2, 2, 2, 2, 3, 3, 3, 3, 4, 4, 4, 4];
    const JPLL: [i64; 12] = [1, 3, 5, 7, 0, 2, 4, 6, 1, 3, 5, 7];

    let face = usize::try_from(ipnest / npface).expect("HEALPix face fits usize");
    let ipf = u64::try_from(ipnest % npface).expect("face-local index is non-negative");
    let mut ix = 0_u64;
    let mut iy = 0_u64;
    for bit in 0..32_u32 {
        ix |= ((ipf >> (2 * bit)) & 1) << bit;
        iy |= ((ipf >> (2 * bit + 1)) & 1) << bit;
    }
    let ix = i64::try_from(ix).expect("x coordinate fits i64");
    let iy = i64::try_from(iy).expect("y coordinate fits i64");

    let jr = JRLL[face] * nside - ix - iy - 1;
    let nl4 = 4 * nside;
    let (nr, n_before, kshift) = if jr < nside {
        let nr = jr;
        (nr, 2 * nr * (nr - 1), 0)
    } else if jr > 3 * nside {
        let nr = nl4 - jr;
        (nr, npix - 2 * nr * (nr + 1), 0)
    } else {
        (
            nside,
            2 * nside * (nside - 1) + (jr - nside) * nl4,
            (jr - nside) & 1,
        )
    };

    let mut jp = (JPLL[face] * nr + ix - iy + 1 + kshift) / 2;
    if jp > nl4 {
        jp -= nl4;
    }
    if jp < 1 {
        jp += nl4;
    }

    u64::try_from(n_before + jp - 1).expect("RING index is non-negative")
}

fn sha256(path: &Path) -> AppResult<String> {
    let mut file = File::open(path)?;
    let mut digest = Sha256::new();
    let mut buffer = [0_u8; 1024 * 1024];
    loop {
        let read = file.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        digest.update(&buffer[..read]);
    }
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let bytes = digest.finalize();
    let mut hex = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        hex.push(char::from(HEX[usize::from(byte >> 4)]));
        hex.push(char::from(HEX[usize::from(byte & 0x0f)]));
    }
    Ok(hex)
}

fn invalid_input(message: impl Into<String>) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidInput, message.into())
}

#[cfg(test)]
mod tests {
    use super::*;

    const FIXTURE: &str = concat!(
        "# schema=nsb-healpix-starlight-candidate-v5\n",
        "# map_type=healpix\n",
        "# coordinate_frame=galactic\n",
        "# ordering=nested\n",
        "# representation=sparse\n",
        "# omitted_pixel_semantics=zero_flux_and_source_counts\n",
        "# nside=1\n",
        "# flux_quantity=integrated_per_pixel\n",
        "# flux_unit=ph_m-2_s-1\n",
        "pixel,flux_ph_m2_s,admitted_sources,excluded_sources\n",
        "0,1.5,2,0\n",
        "11,4.0,1,0\n",
    );

    fn close(a: f64, b: f64) -> bool {
        (a - b).abs() < 1.0e-10
    }

    #[test]
    fn parses_sparse_nested_candidate() {
        let map = parse_starlight_map(FIXTURE).expect("fixture should parse");
        assert_eq!(map.nside, 1);
        assert_eq!(map.pixels, vec![(0, 1.5), (11, 4.0)]);
    }

    #[test]
    fn rejects_unsorted_pixels() {
        let unsorted = FIXTURE.replace("0,1.5,2,0\n11,4.0,1,0", "5,1.5,2,0\n4,4.0,1,0");
        let error = parse_starlight_map(&unsorted).expect_err("unsorted pixels must fail");
        assert!(error.to_string().contains("strictly increasing"));
    }

    #[test]
    fn nside_one_nested_and_ring_indices_match() {
        for pixel in 0..12 {
            assert_eq!(nested_to_ring(1, pixel), pixel);
        }
    }

    #[test]
    fn mollweide_centres_galactic_origin() {
        let (x, y) = mollweide_project(0.0, 0.0);
        assert!(close(x, 0.0));
        assert!(close(y, 0.0));
    }

    #[test]
    fn mollweide_increases_galactic_longitude_to_the_left() {
        let east = mollweide_project(90.0, 0.0);
        let west = mollweide_project(270.0, 0.0);
        assert!(east.0 < 0.0);
        assert!(west.0 > 0.0);
    }
}
