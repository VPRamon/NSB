//! Render a proof-of-concept all-sky natural NSB map for ORM.
//!
//! The map uses the production-safe [`NsbEvaluator::new`] configuration and
//! [`ComponentMask::ALL`]. It is a model of the natural NSB for the visible sky
//! geometry at the Roque de los Muchachos Observatory, not an atmosphere model
//! calibrated specifically for ORM. In particular it does not model artificial
//! light pollution, clouds, the actual aerosol state of the requested night, a
//! local terrain/horizon mask, or local meteorological variability not already
//! represented by the current NSB component models.
//!
//! The fisheye projection is equidistant in zenith distance: zenith is at the
//! centre and the astronomical horizon is the outer circle. The view is that of
//! an observer looking upward with North at the top and East at the left.

use chrono::{DateTime, SecondsFormat, Utc};
use nsb::{ComponentMask, NsbEvaluator, Observer, PointQuery, Target};
use plotters::prelude::*;
use siderust::catalogs::observatories::ObservatoryCatalog;
use siderust::coordinates::frames::EquatorialMeanJ2000;
use siderust::coordinates::spherical;
use siderust::coordinates::transform::SphericalDirectionAstroExt;
use siderust::qtty::Degrees;
use siderust::time::JulianDate;
use std::env;
use std::error::Error;
use std::io;
use std::path::{Path, PathBuf};
use std::time::Instant;
use tempoch::{Time, JD, TT, UTC};

const ORM_NAME: &str = "Roque de los Muchachos Observatory";
const DEFAULT_TIME: &str = "2026-09-17T23:00:00Z";
const DEFAULT_OUTPUT: &str = "orm_nsb.png";
const DEFAULT_STEP_DEG: f64 = 5.0;
const IMAGE_WIDTH: u32 = 1400;
const IMAGE_HEIGHT: u32 = 1000;
const SKY_CENTER: (f64, f64) = (500.0, 515.0);
const SKY_RADIUS: f64 = 390.0;

type AppResult<T> = Result<T, Box<dyn Error>>;

#[derive(Debug)]
struct Args {
    time: DateTime<Utc>,
    output: PathBuf,
    step_deg: f64,
}

#[derive(Debug, Clone, Copy)]
struct SkyCellGeometry {
    az_min_deg: f64,
    az_max_deg: f64,
    alt_min_deg: f64,
    alt_max_deg: f64,
    sample_az_deg: f64,
    sample_alt_deg: f64,
}

#[derive(Debug, Clone, Copy)]
struct SkyCell {
    geometry: SkyCellGeometry,
    radiance: f64,
}

#[derive(Debug, Clone, Copy)]
struct ColorRange {
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

    let catalog = ObservatoryCatalog::builtin();
    let observatory = catalog.get(ORM_NAME).ok_or_else(|| {
        invalid_input(format!(
            "Siderust builtin observatory catalog does not contain `{ORM_NAME}`"
        ))
    })?;
    let observer = observatory.geodetic();
    let time = Time::<UTC>::from_chrono(args.time);
    let jd_tt: JulianDate = time.to::<TT>().to::<JD>();

    // Construct the evaluator exactly once. Its parsed immutable component data
    // is intentionally reused for every direction in the sky grid.
    let evaluator = NsbEvaluator::new()?;
    let component_names = evaluator
        .describe_components(observer, ComponentMask::ALL)?
        .into_iter()
        .map(|component| display_component_name(component.name))
        .collect::<Vec<_>>();

    let grid = generate_sky_grid(args.step_deg);
    let started = Instant::now();
    let cells = evaluate_sky(&evaluator, observer, time, jd_tt, &grid)?;
    let elapsed = started.elapsed();
    let color_range = radiance_range(&cells)?;

    render_map(&args.output, &args, &cells, color_range, &component_names)?;

    println!(
        "wrote {} samples to {} in {:.3} s (radiance {:.6e}..{:.6e} ph cm^-2 ns^-1 sr^-1)",
        cells.len(),
        args.output.display(),
        elapsed.as_secs_f64(),
        color_range.min,
        color_range.max
    );

    Ok(())
}

fn parse_args<I>(args: I) -> AppResult<Option<Args>>
where
    I: IntoIterator<Item = String>,
{
    let mut time_text = DEFAULT_TIME.to_string();
    let mut output = PathBuf::from(DEFAULT_OUTPUT);
    let mut step_deg = DEFAULT_STEP_DEG;
    let mut iter = args.into_iter();

    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "-h" | "--help" => return Ok(None),
            "--time" => {
                time_text = next_value(&mut iter, "--time")?;
            }
            "--output" => {
                output = PathBuf::from(next_value(&mut iter, "--output")?);
            }
            "--step-deg" => {
                let text = next_value(&mut iter, "--step-deg")?;
                step_deg = text.parse::<f64>().map_err(|error| {
                    invalid_input(format!("invalid --step-deg `{text}`: {error}"))
                })?;
            }
            _ => {
                return Err(invalid_input(format!(
                    "unknown argument `{arg}`; use --help for usage"
                ))
                .into());
            }
        }
    }

    if !step_deg.is_finite() || !(0.0 < step_deg && step_deg <= 30.0) {
        return Err(invalid_input(format!(
            "--step-deg must be finite and in (0, 30], got {step_deg}"
        ))
        .into());
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
            "--output must have a .png extension, got `{}`",
            output.display()
        ))
        .into());
    }

    let parsed = DateTime::parse_from_rfc3339(&time_text).map_err(|error| {
        invalid_input(format!(
            "invalid --time `{time_text}`; expected RFC3339 UTC (for example {DEFAULT_TIME}): {error}"
        ))
    })?;
    if parsed.offset().local_minus_utc() != 0 {
        return Err(invalid_input(format!(
            "--time must be UTC (Z or +00:00), got `{time_text}`"
        ))
        .into());
    }

    Ok(Some(Args {
        time: parsed.with_timezone(&Utc),
        output,
        step_deg,
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
        "all_sky_orm - render a natural NSB all-sky map for ORM\n\n\
Usage:\n  cargo run --release --locked -p nsb --example all_sky_orm -- [OPTIONS]\n\n\
Options:\n  --time <RFC3339 UTC>   Observation time [default: {DEFAULT_TIME}]\n  --output <path.png>    Output PNG [default: {DEFAULT_OUTPUT}]\n  --step-deg <degrees>   Horizontal grid angular step in (0, 30] [default: {DEFAULT_STEP_DEG}]\n  -h, --help             Show this help\n\n\
Projection: observer looking upward, North up, East left, zenith at centre, horizon at rim.\n\
Colour: linear total integrated 300-650 nm photon radiance."
    );
}

fn generate_sky_grid(step_deg: f64) -> Vec<SkyCellGeometry> {
    let altitude_bins = (90.0 / step_deg).ceil() as usize;
    let azimuth_bins = (360.0 / step_deg).ceil() as usize;
    let mut cells = Vec::with_capacity(altitude_bins * azimuth_bins);

    let mut alt_min_deg = 0.0;
    while alt_min_deg < 90.0 {
        let alt_max_deg = (alt_min_deg + step_deg).min(90.0);
        let mut az_min_deg = 0.0;
        while az_min_deg < 360.0 {
            let az_max_deg = (az_min_deg + step_deg).min(360.0);
            cells.push(SkyCellGeometry {
                az_min_deg,
                az_max_deg,
                alt_min_deg,
                alt_max_deg,
                sample_az_deg: 0.5 * (az_min_deg + az_max_deg),
                sample_alt_deg: 0.5 * (alt_min_deg + alt_max_deg),
            });
            az_min_deg = az_max_deg;
        }
        alt_min_deg = alt_max_deg;
    }

    cells
}

fn evaluate_sky(
    evaluator: &NsbEvaluator,
    observer: Observer,
    time: Time<UTC>,
    jd_tt: JulianDate,
    grid: &[SkyCellGeometry],
) -> AppResult<Vec<SkyCell>> {
    let mut cells = Vec::with_capacity(grid.len());

    for geometry in grid {
        let target = horizontal_to_target(
            observer,
            jd_tt,
            geometry.sample_az_deg,
            geometry.sample_alt_deg,
        );
        let query = PointQuery::new(observer, time, target).with_components(ComponentMask::ALL);
        let result = evaluator.evaluate(&query)?;
        let radiance = result.integrated.value();
        if !radiance.is_finite() || radiance < 0.0 {
            return Err(io::Error::other(format!(
                "non-finite or negative NSB at az={:.3} deg alt={:.3} deg: {radiance}",
                geometry.sample_az_deg, geometry.sample_alt_deg
            ))
            .into());
        }
        cells.push(SkyCell {
            geometry: *geometry,
            radiance,
        });
    }

    Ok(cells)
}

fn horizontal_to_target(
    observer: Observer,
    jd_tt: JulianDate,
    azimuth_deg: f64,
    altitude_deg: f64,
) -> Target {
    // Siderust's horizontal direction constructor is (altitude, azimuth), with
    // azimuth measured clockwise from North through East. The public horizontal
    // transform returns true-of-date equatorial coordinates; rotate those into
    // the J2000 mean equatorial frame used by NSB's Target alias.
    let horizontal = spherical::direction::Horizontal::new(
        Degrees::new(altitude_deg),
        Degrees::new(azimuth_deg),
    );
    let true_of_date = horizontal.to_equatorial(&jd_tt, &observer);
    true_of_date.to_frame::<EquatorialMeanJ2000>(&jd_tt)
}

fn radiance_range(cells: &[SkyCell]) -> AppResult<ColorRange> {
    let mut min = f64::INFINITY;
    let mut max = f64::NEG_INFINITY;
    for cell in cells {
        min = min.min(cell.radiance);
        max = max.max(cell.radiance);
    }

    if !min.is_finite() || !max.is_finite() {
        return Err(io::Error::other("cannot normalize an empty/non-finite sky map").into());
    }

    Ok(ColorRange { min, max })
}

fn render_map(
    output: &Path,
    args: &Args,
    cells: &[SkyCell],
    range: ColorRange,
    components: &[&'static str],
) -> AppResult<()> {
    let root = BitMapBackend::new(output, (IMAGE_WIDTH, IMAGE_HEIGHT)).into_drawing_area();
    root.fill(&WHITE)?;

    root.draw(&Text::new(
        "Natural Night Sky Background - Roque de los Muchachos Observatory",
        (45, 42),
        ("sans-serif", 34).into_font(),
    ))?;
    root.draw(&Text::new(
        "Natural NSB modelled for the visible sky geometry at Roque de los Muchachos Observatory.",
        (45, 78),
        ("sans-serif", 20).into_font(),
    ))?;

    for cell in cells {
        let g = cell.geometry;
        let corners = [
            project_fisheye(g.az_min_deg, g.alt_min_deg, SKY_CENTER, SKY_RADIUS),
            project_fisheye(g.az_max_deg, g.alt_min_deg, SKY_CENTER, SKY_RADIUS),
            project_fisheye(g.az_max_deg, g.alt_max_deg, SKY_CENTER, SKY_RADIUS),
            project_fisheye(g.az_min_deg, g.alt_max_deg, SKY_CENTER, SKY_RADIUS),
        ]
        .into_iter()
        .map(to_pixel)
        .collect::<Vec<_>>();
        root.draw(&Polygon::new(
            corners,
            radiance_color(normalize(cell.radiance, range)).filled(),
        ))?;
    }

    draw_altitude_grid(&root)?;
    draw_cardinals(&root)?;
    draw_color_bar(&root, range)?;
    draw_metadata(&root, args, components, cells.len())?;

    root.present()?;
    Ok(())
}

fn draw_altitude_grid(
    root: &DrawingArea<BitMapBackend<'_>, plotters::coord::Shift>,
) -> AppResult<()> {
    let center = to_pixel(SKY_CENTER);
    let grid_style = ShapeStyle::from(&BLACK.mix(0.55)).stroke_width(1);

    // Put altitude labels in the north-west quadrant of the *sky* (upper-right
    // on this overhead chart), away from the cardinal labels and colour bar.
    const LABEL_AZIMUTH_DEG: f64 = 315.0;
    for altitude in [0.0, 30.0, 60.0] {
        let radius = SKY_RADIUS * (90.0 - altitude) / 90.0;
        root.draw(&Circle::new(center, radius.round() as i32, grid_style))?;
        let label_position = project_fisheye(LABEL_AZIMUTH_DEG, altitude, SKY_CENTER, SKY_RADIUS);
        root.draw(&Text::new(
            format!("{altitude:.0} deg"),
            to_pixel((label_position.0 + 7.0, label_position.1 - 5.0)),
            ("sans-serif", 15).into_font(),
        ))?;
    }

    root.draw(&Circle::new(
        center,
        SKY_RADIUS.round() as i32,
        ShapeStyle::from(&BLACK).stroke_width(2),
    ))?;
    root.draw(&Circle::new(center, 3, BLACK.filled()))?;
    root.draw(&Text::new(
        "Zenith (90 deg)",
        to_pixel((SKY_CENTER.0 + 10.0, SKY_CENTER.1 - 12.0)),
        ("sans-serif", 15).into_font(),
    ))?;

    Ok(())
}

fn draw_cardinals(root: &DrawingArea<BitMapBackend<'_>, plotters::coord::Shift>) -> AppResult<()> {
    let offset = SKY_RADIUS + 28.0;
    let labels = [
        ("N", (SKY_CENTER.0, SKY_CENTER.1 - offset)),
        ("E", (SKY_CENTER.0 - offset, SKY_CENTER.1)),
        ("S", (SKY_CENTER.0, SKY_CENTER.1 + offset)),
        ("W", (SKY_CENTER.0 + offset, SKY_CENTER.1)),
    ];
    for (label, position) in labels {
        root.draw(&Text::new(
            label,
            to_pixel((position.0 - 9.0, position.1 + 9.0)),
            ("sans-serif", 26).into_font().style(FontStyle::Bold),
        ))?;
    }

    root.draw(&Text::new(
        "Looking upward: North up, East left",
        (325, 965),
        ("sans-serif", 17).into_font(),
    ))?;
    Ok(())
}

fn draw_color_bar(
    root: &DrawingArea<BitMapBackend<'_>, plotters::coord::Shift>,
    range: ColorRange,
) -> AppResult<()> {
    const X0: i32 = 995;
    const X1: i32 = 1035;
    const Y0: i32 = 165;
    const Y1: i32 = 610;
    let height = Y1 - Y0;

    for pixel in 0..height {
        let t = 1.0 - f64::from(pixel) / f64::from(height - 1);
        root.draw(&Rectangle::new(
            [(X0, Y0 + pixel), (X1, Y0 + pixel + 1)],
            radiance_color(t).filled(),
        ))?;
    }
    root.draw(&Rectangle::new(
        [(X0, Y0), (X1, Y1)],
        ShapeStyle::from(&BLACK).stroke_width(1),
    ))?;

    root.draw(&Text::new(
        "Total 300-650 nm photon radiance",
        (955, 120),
        ("sans-serif", 18).into_font(),
    ))?;
    root.draw(&Text::new(
        "ph cm^-2 ns^-1 sr^-1  (linear scale)",
        (955, 145),
        ("sans-serif", 17).into_font(),
    ))?;

    for tick in 0..=5 {
        let t = f64::from(tick) / 5.0;
        let y = Y1 - (f64::from(height) * t).round() as i32;
        let value = range.min + t * (range.max - range.min);
        root.draw(&PathElement::new(vec![(X1, y), (X1 + 8, y)], BLACK))?;
        root.draw(&Text::new(
            format!("{value:.3e}"),
            (X1 + 14, y + 5),
            ("monospace", 16).into_font(),
        ))?;
    }

    Ok(())
}

fn draw_metadata(
    root: &DrawingArea<BitMapBackend<'_>, plotters::coord::Shift>,
    args: &Args,
    components: &[&'static str],
    sample_count: usize,
) -> AppResult<()> {
    let timestamp = args.time.to_rfc3339_opts(SecondsFormat::Secs, true);
    let mut lines = vec![
        format!("UTC: {timestamp}"),
        format!("Site: {ORM_NAME} (ORM, La Palma)"),
        format!("Grid step: {:.3} deg", args.step_deg),
        format!("Evaluated directions: {sample_count}"),
        "Components used:".to_string(),
    ];
    lines.extend(components.iter().map(|name| format!("  - {name}")));
    lines.extend([
        "Evaluator: NsbEvaluator::new(), ComponentMask::ALL".to_string(),
        String::new(),
        "Scientific interpretation:".to_string(),
        "- Natural NSB model; not a measured sky map.".to_string(),
        "- Not an atmosphere calibrated specifically for ORM.".to_string(),
        "- No artificial light pollution or clouds.".to_string(),
        "- No measured nightly aerosol state or local horizon mask.".to_string(),
        "- No additional local meteorological variability.".to_string(),
    ]);

    let scientific_heading = lines
        .iter()
        .position(|line| line == "Scientific interpretation:")
        .ok_or_else(|| io::Error::other("missing scientific interpretation heading"))?;

    let mut y = 640;
    for (index, line) in lines.iter().enumerate() {
        let font = if index == scientific_heading {
            ("sans-serif", 17).into_font().style(FontStyle::Bold)
        } else {
            ("sans-serif", 14).into_font()
        };
        root.draw(&Text::new(line.clone(), (900, y), font))?;
        y += 18;
    }

    Ok(())
}

fn normalize(value: f64, range: ColorRange) -> f64 {
    let span = range.max - range.min;
    if span <= f64::EPSILON {
        0.5
    } else {
        ((value - range.min) / span).clamp(0.0, 1.0)
    }
}

fn radiance_color(t: f64) -> RGBColor {
    // Compact viridis-like sequential map. Keeping the mapping explicit makes
    // the color bar and sky use exactly the same linear normalization.
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

fn project_fisheye(
    azimuth_deg: f64,
    altitude_deg: f64,
    center: (f64, f64),
    radius: f64,
) -> (f64, f64) {
    let zenith_distance_fraction = (90.0 - altitude_deg) / 90.0;
    let r = radius * zenith_distance_fraction;
    let azimuth = azimuth_deg.to_radians();

    // Siderust azimuth increases North -> East. For a chart viewed from below
    // (observer looking upward), East is on the left: x therefore uses -sin(Az).
    let x = center.0 - r * azimuth.sin();
    let y = center.1 - r * azimuth.cos();
    (x, y)
}

fn to_pixel(point: (f64, f64)) -> (i32, i32) {
    (point.0.round() as i32, point.1.round() as i32)
}

fn display_component_name(name: &'static str) -> &'static str {
    match name {
        "starlight" => "integrated starlight",
        "moon" => "scattered moonlight",
        other => other,
    }
}

fn invalid_input(message: impl Into<String>) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidInput, message.into())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: f64, b: f64) -> bool {
        (a - b).abs() < 1.0e-10
    }

    #[test]
    fn fisheye_places_zenith_at_center() {
        let center = (10.0, 20.0);
        let projected = project_fisheye(217.0, 90.0, center, 5.0);
        assert!(close(projected.0, center.0));
        assert!(close(projected.1, center.1));
    }

    #[test]
    fn fisheye_cardinals_match_overhead_orientation() {
        let center = (0.0, 0.0);
        let radius = 10.0;
        let north = project_fisheye(0.0, 0.0, center, radius);
        let east = project_fisheye(90.0, 0.0, center, radius);
        let south = project_fisheye(180.0, 0.0, center, radius);
        let west = project_fisheye(270.0, 0.0, center, radius);

        assert!(close(north.0, 0.0) && close(north.1, -radius));
        assert!(close(east.0, -radius) && close(east.1, 0.0));
        assert!(close(south.0, 0.0) && close(south.1, radius));
        assert!(close(west.0, radius) && close(west.1, 0.0));
    }
}
