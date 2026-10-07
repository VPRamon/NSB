//! Deterministic NSB-owned Mie phase-grid generation.
//!
//! The solver follows the amplitude recurrences in Bohren & Huffman (1983),
//! equations 4.74 and 4.88.  Lognormal modes are integrated in log-radius
//! with composite Simpson quadrature and weighted by scattering cross section.

use super::{Artifact, RunConfig, ValidationGate};
use anyhow::{bail, Context, Result};
use num_complex::Complex64;
use serde::Deserialize;
use std::f64::consts::PI;
use std::fmt::Write;
use std::fs;
use std::path::Path;

pub const MODEL_SOURCE: &str = "moonlight-aerosol-nsb-v1.toml";
pub const OUTPUT: &str = "moonlight_mie_nsb_v1.dat";
pub const SSCAT: &str = "sscatcor_m15s1.dat";

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Model {
    schema: String,
    solver_version: String,
    reference: String,
    distribution: String,
    refractive_index_real: f64,
    refractive_index_imaginary: f64,
    normalization: String,
    radius_sigma_bounds: f64,
    radius_intervals: usize,
    wavelength_start_nm: usize,
    wavelength_end_nm: usize,
    wavelength_step_nm: usize,
    angle_segments: Vec<AngleSegment>,
    modes: Vec<Mode>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct AngleSegment {
    start_deg: f64,
    end_deg: f64,
    step_deg: f64,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Mode {
    name: String,
    number_density_cm3: f64,
    modal_radius_um: f64,
    log10_geometric_sigma: f64,
    abundance: f64,
}

pub fn validate_config(config: &RunConfig) -> Result<()> {
    let names: Vec<_> = config
        .sources
        .iter()
        .map(|source| source.name.as_str())
        .collect();
    if names != [MODEL_SOURCE, SSCAT] {
        bail!("moonlight sources must be ordered as {MODEL_SOURCE:?}, {SSCAT:?}");
    }
    Ok(())
}

pub fn build(config: &RunConfig) -> Result<Vec<Artifact>> {
    let source_root = config.workspace.root.join("sources");
    let output_root = config.workspace.root.join("outputs");
    fs::create_dir_all(&output_root)?;
    let model = read_model(&source_root.join(MODEL_SOURCE))?;
    let bytes = generate(&model)?;
    let mie_path = output_root.join(OUTPUT);
    super::engine::atomic_write(&mie_path, bytes.as_bytes())?;
    let sscat_path = output_root.join(SSCAT);
    let sscat_bytes = fs::read(source_root.join(SSCAT)).context("run update before build")?;
    super::engine::atomic_write(&sscat_path, &sscat_bytes)?;
    let mut artifacts = vec![artifact(OUTPUT, &mie_path)?, artifact(SSCAT, &sscat_path)?];
    artifacts.sort_by(|a, b| a.name.cmp(&b.name));
    super::engine::atomic_write(
        &output_root.join("artifacts.json"),
        &serde_json::to_vec_pretty(&artifacts)?,
    )?;
    Ok(artifacts)
}

pub fn validate_artifact(name: &str, path: &Path) -> Result<()> {
    if name == SSCAT {
        let rows = fs::read_to_string(path)?
            .lines()
            .filter(|line| {
                let line = line.trim();
                !line.is_empty() && !line.starts_with('#')
            })
            .count();
        if rows < 2 {
            bail!("{name} contains too few data rows");
        }
        return Ok(());
    }
    if name != OUTPUT {
        bail!("unexpected moonlight artifact {name:?}");
    }
    let parsed = parse_grid(&fs::read_to_string(path)?)?;
    if parsed.wavelengths.first() != Some(&300.0) || parsed.wavelengths.last() != Some(&650.0) {
        bail!("Mie wavelength coverage must be 300--650 nm");
    }
    if parsed.angles.first() != Some(&0.0) || parsed.angles.last() != Some(&180.0) {
        bail!("Mie angular coverage must be 0--180 degrees");
    }
    if parsed
        .values
        .iter()
        .flatten()
        .any(|v| !v.is_finite() || *v < 0.0)
    {
        bail!("Mie phase values must be finite and non-negative");
    }
    Ok(())
}

pub fn validation_gates(config: &RunConfig, artifacts: &[Artifact]) -> Result<Vec<ValidationGate>> {
    let artifact = artifacts
        .iter()
        .find(|a| a.name == OUTPUT)
        .context("missing Mie artifact")?;
    let grid = parse_grid(&fs::read_to_string(&artifact.path)?)?;
    let model = read_model(&config.workspace.root.join("sources").join(MODEL_SOURCE))?;

    let mut worst_norm = 0.0_f64;
    let mut min_g = f64::INFINITY;
    let mut max_g = f64::NEG_INFINITY;
    let mut worst_g_delta = 0.0_f64;
    for (&wavelength_nm, row) in grid.wavelengths.iter().zip(&grid.values) {
        let (norm, grid_g) = angular_moments(&grid.angles, row);
        worst_norm = worst_norm.max((norm - 4.0 * PI).abs() / (4.0 * PI));
        min_g = min_g.min(grid_g);
        max_g = max_g.max(grid_g);
        let (_, analytic_g) = ensemble_phase(&model, wavelength_nm / 1000.0, &[])?;
        worst_g_delta = worst_g_delta.max((grid_g - analytic_g).abs());
    }
    let forward = grid.values.iter().all(|row| {
        let ninety = linear_lookup(&grid.angles, row, 90.0);
        row[0] > ninety
    });
    let interpolation_error = angular_interpolation_error(&model, &grid)?;

    Ok(vec![
        ValidationGate {
            name: "mie-4pi-normalization".into(),
            passed: worst_norm < 1.0e-4,
            detail: format!("worst independent trapezoid error={worst_norm:.3e}"),
        },
        ValidationGate {
            name: "mie-asymmetry-range".into(),
            passed: min_g > 0.4 && max_g < 0.95,
            detail: format!("grid g range={min_g:.6}..{max_g:.6}"),
        },
        ValidationGate {
            name: "mie-asymmetry-consistency".into(),
            passed: worst_g_delta < 5.0e-5,
            detail: format!("worst |grid g - coefficient g|={worst_g_delta:.3e}"),
        },
        ValidationGate {
            name: "mie-angular-interpolation".into(),
            passed: interpolation_error < 3.0e-3,
            detail: format!("worst direct-solver probe relative error={interpolation_error:.3e}"),
        },
        ValidationGate {
            name: "mie-forward-scattering".into(),
            passed: forward,
            detail: "P(0 degrees) exceeds P(90 degrees) at every wavelength".into(),
        },
    ])
}

fn angular_interpolation_error(model: &Model, grid: &Grid) -> Result<f64> {
    const PROBES: [f64; 14] = [
        0.00625, 0.01875, 0.0625, 0.1875, 0.5125, 1.025, 2.0625, 5.125, 9.875, 10.5, 90.5, 169.5,
        170.0625, 179.9375,
    ];
    let mut worst = 0.0_f64;
    let candidate_rows = [0, grid.values.len() / 2, grid.values.len() - 1];
    for row_index in candidate_rows {
        let wavelength_um = grid.wavelengths[row_index] / 1000.0;
        let (direct, _) = ensemble_phase(model, wavelength_um, &PROBES)?;
        for (&angle, &expected) in PROBES.iter().zip(&direct) {
            let interpolated = linear_lookup(&grid.angles, &grid.values[row_index], angle);
            worst = worst.max((interpolated / expected - 1.0).abs());
        }
    }
    Ok(worst)
}

fn linear_lookup(axis: &[f64], values: &[f64], value: f64) -> f64 {
    debug_assert_eq!(axis.len(), values.len());
    if value <= axis[0] {
        return values[0];
    }
    let last = axis.len() - 1;
    if value >= axis[last] {
        return values[last];
    }
    let upper = axis.partition_point(|&x| x <= value);
    let lower = upper - 1;
    let t = (value - axis[lower]) / (axis[upper] - axis[lower]);
    values[lower] + t * (values[upper] - values[lower])
}

fn artifact(name: &str, path: &Path) -> Result<Artifact> {
    Ok(Artifact {
        name: name.into(),
        path: path.to_path_buf(),
        sha256: crate::platform::checksum_io::sha256_file(path)?,
        bytes: fs::metadata(path)?.len(),
    })
}

fn read_model(path: &Path) -> Result<Model> {
    let raw =
        fs::read_to_string(path).with_context(|| format!("failed to read {}", path.display()))?;
    let model: Model = toml::from_str(&raw)?;
    validate_model(&model)?;
    Ok(model)
}

fn validate_model(model: &Model) -> Result<()> {
    if model.schema != "nsb-moonlight-aerosol-model-v1"
        || model.solver_version != "nsb-bh-mie-v1"
        || model.distribution != "lognormal-number-per-log10-radius"
        || model.normalization != "integral-over-solid-angle-equals-4pi"
        || model.reference.trim().is_empty()
        || !model.refractive_index_real.is_finite()
        || !model.refractive_index_imaginary.is_finite()
        || model.refractive_index_real <= 0.0
        || !model.radius_sigma_bounds.is_finite()
        || model.radius_sigma_bounds <= 0.0
        || model.modes.len() != 4
        || model.radius_intervals == 0
        || (model.radius_intervals & 1) != 0
        || model.wavelength_step_nm == 0
        || model.wavelength_start_nm > model.wavelength_end_nm
    {
        bail!("invalid NSB Mie model configuration");
    }
    let wavelength_span = model.wavelength_end_nm - model.wavelength_start_nm;
    if wavelength_span / model.wavelength_step_nm * model.wavelength_step_nm != wavelength_span {
        bail!("wavelength range must be divisible by wavelength_step_nm");
    }
    for mode in &model.modes {
        validate_mode(mode)?;
    }
    angle_grid(model)?;
    Ok(())
}

fn validate_mode(mode: &Mode) -> Result<()> {
    if mode.name.trim().is_empty()
        || !mode.number_density_cm3.is_finite()
        || !mode.modal_radius_um.is_finite()
        || !mode.log10_geometric_sigma.is_finite()
        || !mode.abundance.is_finite()
        || mode.modal_radius_um <= 0.0
        || mode.log10_geometric_sigma <= 0.0
        || mode.number_density_cm3 < 0.0
        || mode.abundance < 0.0
    {
        bail!("invalid aerosol mode");
    }
    Ok(())
}

fn angle_grid(model: &Model) -> Result<Vec<f64>> {
    if model.angle_segments.is_empty() {
        bail!("angle_segments must not be empty");
    }
    let mut angles = Vec::new();
    let mut previous_end: Option<f64> = None;
    for segment in &model.angle_segments {
        if !segment.start_deg.is_finite()
            || !segment.end_deg.is_finite()
            || !segment.step_deg.is_finite()
            || segment.step_deg <= 0.0
            || segment.start_deg < 0.0
            || segment.end_deg > 180.0
            || segment.start_deg >= segment.end_deg
        {
            bail!("invalid angular grid segment");
        }
        if previous_end.is_some_and(|end| (segment.start_deg - end).abs() > 1.0e-12) {
            bail!("angular grid segments must be contiguous");
        }
        let span = segment.end_deg - segment.start_deg;
        let interval_count = (span / segment.step_deg).round();
        if interval_count < 1.0 || (span - interval_count * segment.step_deg).abs() > 1.0e-10 {
            bail!("angular grid segment span must be divisible by its step");
        }
        let interval_count = interval_count as usize;
        for index in 0..=interval_count {
            if index == 0 && !angles.is_empty() {
                continue;
            }
            let value = if index == interval_count {
                segment.end_deg
            } else {
                segment.start_deg + index as f64 * segment.step_deg
            };
            angles.push(value);
        }
        previous_end = Some(segment.end_deg);
    }
    if angles.first() != Some(&0.0) || angles.last() != Some(&180.0) {
        bail!("angular grid must cover exactly 0--180 degrees");
    }
    Ok(angles)
}

fn generate(model: &Model) -> Result<String> {
    validate_model(model)?;
    let wavelengths: Vec<_> = (model.wavelength_start_nm..=model.wavelength_end_nm)
        .step_by(model.wavelength_step_nm)
        .collect();
    let angles = angle_grid(model)?;
    let mut rows = Vec::with_capacity(wavelengths.len());
    for &wavelength_nm in &wavelengths {
        let (row, _) = ensemble_phase(model, wavelength_nm as f64 / 1000.0, &angles)?;
        rows.push(row);
    }

    let angle_description = model
        .angle_segments
        .iter()
        .map(|segment| {
            format!(
                "{}..{}/{}",
                segment.start_deg, segment.end_deg, segment.step_deg
            )
        })
        .collect::<Vec<_>>()
        .join(";");

    let mut out = String::new();
    writeln!(out, "# schema = nsb-moonlight-mie-phase-v1")?;
    writeln!(out, "# aerosol_schema = nsb-moonlight-aerosol-model-v1")?;
    writeln!(
        out,
        "# solver = Bohren-Huffman amplitude recurrences, in-tree implementation v1"
    )?;
    writeln!(
        out,
        "# normalization = integral P(theta) dOmega = 4*pi; analytic coefficient normalization"
    )?;
    writeln!(
        out,
        "# wavelength_grid_nm = {}..{} step {}",
        model.wavelength_start_nm, model.wavelength_end_nm, model.wavelength_step_nm
    )?;
    writeln!(
        out,
        "# scattering_angle_grid_deg = nonuniform: {angle_description}"
    )?;
    writeln!(
        out,
        "# radius_quadrature = composite Simpson in ln(radius), {} intervals over +/-{} sigma",
        model.radius_intervals, model.radius_sigma_bounds
    )?;
    writeln!(
        out,
        "# axes = wavelength_um then scattering_angle_deg; rows are wavelength-major"
    )?;
    writeln!(out, "{} {}", wavelengths.len(), angles.len())?;
    write!(
        out,
        "{}",
        wavelengths
            .iter()
            .map(|w| format!("{:.6}", *w as f64 / 1000.0))
            .collect::<Vec<_>>()
            .join(" ")
    )?;
    out.push('\n');
    write!(
        out,
        "{}",
        angles
            .iter()
            .map(|angle| format!("{angle:.6}"))
            .collect::<Vec<_>>()
            .join(" ")
    )?;
    out.push('\n');
    for row in rows {
        writeln!(
            out,
            "{}",
            row.iter()
                .map(|v| format!("{v:.9e}"))
                .collect::<Vec<_>>()
                .join(" ")
        )?;
    }
    Ok(out)
}

fn ensemble_phase(model: &Model, wavelength_um: f64, angles: &[f64]) -> Result<(Vec<f64>, f64)> {
    if !wavelength_um.is_finite() || wavelength_um <= 0.0 {
        bail!("invalid wavelength");
    }
    let mus: Vec<_> = angles
        .iter()
        .map(|angle| angle.to_radians().cos())
        .collect();
    let refractive = Complex64::new(
        model.refractive_index_real,
        model.refractive_index_imaginary,
    );
    let mut accumulator = EnsembleAccumulator {
        phase: vec![0.0; angles.len()],
        scattering_weight: 0.0,
        asymmetry_weight: 0.0,
    };
    for mode in &model.modes {
        integrate_mode(
            model,
            mode,
            wavelength_um,
            refractive,
            &mus,
            &mut accumulator,
        )?;
    }
    if !accumulator.scattering_weight.is_finite() || accumulator.scattering_weight <= 0.0 {
        bail!("zero or invalid scattering weight");
    }
    for value in &mut accumulator.phase {
        *value /= accumulator.scattering_weight;
    }
    Ok((
        accumulator.phase,
        accumulator.asymmetry_weight / accumulator.scattering_weight,
    ))
}

struct EnsembleAccumulator {
    phase: Vec<f64>,
    scattering_weight: f64,
    asymmetry_weight: f64,
}

fn integrate_mode(
    model: &Model,
    mode: &Mode,
    wavelength_um: f64,
    refractive: Complex64,
    mus: &[f64],
    accumulator: &mut EnsembleAccumulator,
) -> Result<()> {
    validate_mode(mode)?;
    let sigma = mode.log10_geometric_sigma * 10.0_f64.ln();
    let lo = mode.modal_radius_um.ln() - model.radius_sigma_bounds * sigma;
    let hi = mode.modal_radius_um.ln() + model.radius_sigma_bounds * sigma;
    let h = (hi - lo) / model.radius_intervals as f64;
    for i in 0..=model.radius_intervals {
        let u = lo + i as f64 * h;
        let radius = u.exp();
        let z = (u - mode.modal_radius_um.ln()) / sigma;
        let number_per_ln_r = mode.number_density_cm3 * mode.abundance * (-0.5 * z * z).exp()
            / (sigma * (2.0 * PI).sqrt());
        let simpson = if i == 0 || i == model.radius_intervals {
            1.0
        } else if (i & 1) == 0 {
            2.0
        } else {
            4.0
        };
        let x = 2.0 * PI * radius / wavelength_um;
        let sample = mie_phase(x, refractive, mus)?;
        let weight = simpson * h / 3.0 * number_per_ln_r * PI * radius * radius * sample.qsca;
        accumulator.scattering_weight += weight;
        accumulator.asymmetry_weight += weight * sample.asymmetry;
        for (sum, value) in accumulator.phase.iter_mut().zip(sample.phase) {
            *sum += weight * value;
        }
    }
    Ok(())
}

#[derive(Debug)]
struct MieSample {
    qsca: f64,
    asymmetry: f64,
    phase: Vec<f64>,
}

/// Return scattering efficiency, analytic asymmetry and a phase function
/// whose continuous solid-angle integral is 4 pi.
fn mie_phase(x: f64, m: Complex64, mus: &[f64]) -> Result<MieSample> {
    if !x.is_finite() || x <= 0.0 || !m.re.is_finite() || !m.im.is_finite() || m.norm_sqr() == 0.0 {
        bail!("invalid Mie size parameter or refractive index");
    }
    let nstop = (x + 4.0 * x.cbrt() + 2.0).ceil() as usize;
    let nmx = (nstop as f64).max((m * x).norm()).ceil() as usize + 15;
    let mx = m * x;
    let mut d = vec![Complex64::new(0.0, 0.0); nmx + 1];
    for n in (1..=nmx).rev() {
        let z = Complex64::new(n as f64, 0.0) / mx;
        d[n - 1] = z - Complex64::new(1.0, 0.0) / (d[n] + z);
    }

    let mut psi0 = x.cos();
    let mut psi1 = x.sin();
    let mut chi0 = -x.sin();
    let mut chi1 = x.cos();
    let mut an = Vec::with_capacity(nstop);
    let mut bn = Vec::with_capacity(nstop);
    let mut qsum = 0.0;
    for (n, d_n) in d.iter().enumerate().take(nstop + 1).skip(1) {
        let nf = n as f64;
        let psi = (2.0 * nf - 1.0) / x * psi1 - psi0;
        let chi = (2.0 * nf - 1.0) / x * chi1 - chi0;
        let xi = Complex64::new(psi, -chi);
        let xi1 = Complex64::new(psi1, -chi1);
        let da = *d_n / m + Complex64::new(nf / x, 0.0);
        let db = m * *d_n + Complex64::new(nf / x, 0.0);
        let a = (da * psi - psi1) / (da * xi - xi1);
        let b = (db * psi - psi1) / (db * xi - xi1);
        qsum += (2.0 * nf + 1.0) * (a.norm_sqr() + b.norm_sqr());
        an.push(a);
        bn.push(b);
        psi0 = psi1;
        psi1 = psi;
        chi0 = chi1;
        chi1 = chi;
    }
    if !qsum.is_finite() || qsum <= 0.0 {
        bail!("invalid Mie scattering coefficient sum");
    }
    let qsca = 2.0 * qsum / (x * x);

    let mut gsum = 0.0;
    for (index, (a_pair, b_pair)) in an.windows(2).zip(bn.windows(2)).enumerate() {
        let n = (index + 1) as f64;
        gsum += n * (n + 2.0) / (n + 1.0)
            * ((a_pair[0] * a_pair[1].conj()).re + (b_pair[0] * b_pair[1].conj()).re);
    }
    for (index, (a, b)) in an.iter().zip(&bn).enumerate() {
        let n = (index + 1) as f64;
        gsum += (2.0 * n + 1.0) / (n * (n + 1.0)) * (*a * b.conj()).re;
    }
    let asymmetry = 2.0 * gsum / qsum;

    let mut phase = Vec::with_capacity(mus.len());
    for &mu in mus {
        let mut pi_nm1 = 0.0;
        let mut pi_n = 1.0;
        let mut s1 = Complex64::new(0.0, 0.0);
        let mut s2 = Complex64::new(0.0, 0.0);
        for (index, (a, b)) in an.iter().zip(&bn).enumerate() {
            let n = (index + 1) as f64;
            let tau = n * mu * pi_n - (n + 1.0) * pi_nm1;
            let factor = (2.0 * n + 1.0) / (n * (n + 1.0));
            s1 += factor * (*a * pi_n + *b * tau);
            s2 += factor * (*a * tau + *b * pi_n);
            let next = ((2.0 * n + 1.0) * mu * pi_n - (n + 1.0) * pi_nm1) / n;
            pi_nm1 = pi_n;
            pi_n = next;
        }
        phase.push((s1.norm_sqr() + s2.norm_sqr()) / qsum);
    }

    Ok(MieSample {
        qsca,
        asymmetry,
        phase,
    })
}

struct Grid {
    wavelengths: Vec<f64>,
    angles: Vec<f64>,
    values: Vec<Vec<f64>>,
}

fn parse_grid(raw: &str) -> Result<Grid> {
    let mut lines = raw.lines().filter(|line| {
        let t = line.trim();
        !t.is_empty() && !t.starts_with('#')
    });
    let dims: Vec<usize> = lines
        .next()
        .context("missing dimensions")?
        .split_whitespace()
        .map(str::parse)
        .collect::<std::result::Result<_, _>>()?;
    if dims.len() != 2 || dims[0] == 0 || dims[1] < 2 {
        bail!("bad dimensions");
    }
    let wavelengths = parse_row(lines.next().context("missing wavelengths")?)?;
    let angles = parse_row(lines.next().context("missing angles")?)?;
    let values: Vec<Vec<f64>> = lines.map(parse_row).collect::<Result<_>>()?;
    if wavelengths.len() != dims[0]
        || angles.len() != dims[1]
        || values.len() != dims[0]
        || values.iter().any(|row| row.len() != dims[1])
    {
        bail!("grid dimension mismatch");
    }
    if wavelengths.iter().any(|value| !value.is_finite())
        || angles.iter().any(|value| !value.is_finite())
        || !wavelengths.windows(2).all(|window| window[0] < window[1])
        || !angles.windows(2).all(|window| window[0] < window[1])
        || values
            .iter()
            .flatten()
            .any(|value| !value.is_finite() || *value < 0.0)
    {
        bail!("grid contains invalid axes or phase values");
    }
    Ok(Grid {
        wavelengths: wavelengths
            .into_iter()
            .map(|value| value * 1000.0)
            .collect(),
        angles,
        values,
    })
}

fn parse_row(row: &str) -> Result<Vec<f64>> {
    Ok(row
        .split_whitespace()
        .map(str::parse)
        .collect::<std::result::Result<_, _>>()?)
}

fn angular_moments(angles: &[f64], phase: &[f64]) -> (f64, f64) {
    let mut norm = 0.0;
    let mut cosine = 0.0;
    for i in 0..angles.len() - 1 {
        let t0 = angles[i].to_radians();
        let t1 = angles[i + 1].to_radians();
        let d = t1 - t0;
        norm += 2.0 * PI * 0.5 * d * (phase[i] * t0.sin() + phase[i + 1] * t1.sin());
        cosine += 2.0
            * PI
            * 0.5
            * d
            * (phase[i] * t0.sin() * t0.cos() + phase[i + 1] * t1.sin() * t1.cos());
    }
    (norm, cosine / norm)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dataset::config::{ExecutionConfig, WorkspaceConfig};
    use crate::dataset::{DatasetName, SourceConfig};
    use std::path::PathBuf;

    fn tiny_model() -> Model {
        Model {
            schema: "nsb-moonlight-aerosol-model-v1".into(),
            solver_version: "nsb-bh-mie-v1".into(),
            reference: "test reference".into(),
            distribution: "lognormal-number-per-log10-radius".into(),
            refractive_index_real: 1.5,
            refractive_index_imaginary: 0.0,
            normalization: "integral-over-solid-angle-equals-4pi".into(),
            radius_sigma_bounds: 3.0,
            radius_intervals: 8,
            wavelength_start_nm: 300,
            wavelength_end_nm: 300,
            wavelength_step_nm: 10,
            angle_segments: vec![AngleSegment {
                start_deg: 0.0,
                end_deg: 180.0,
                step_deg: 30.0,
            }],
            modes: vec![
                test_mode("one"),
                test_mode("two"),
                test_mode("three"),
                test_mode("four"),
            ],
        }
    }

    fn test_mode(name: &str) -> Mode {
        Mode {
            name: name.into(),
            number_density_cm3: 1.0,
            modal_radius_um: 0.01,
            log10_geometric_sigma: 0.1,
            abundance: 1.0,
        }
    }

    fn tiny_model_toml() -> &'static str {
        r#"schema = "nsb-moonlight-aerosol-model-v1"
solver_version = "nsb-bh-mie-v1"
reference = "test reference"
distribution = "lognormal-number-per-log10-radius"
refractive_index_real = 1.5
refractive_index_imaginary = 0.0
normalization = "integral-over-solid-angle-equals-4pi"
radius_sigma_bounds = 3.0
radius_intervals = 8
wavelength_start_nm = 300
wavelength_end_nm = 300
wavelength_step_nm = 10

[[angle_segments]]
start_deg = 0.0
end_deg = 180.0
step_deg = 30.0

[[modes]]
name = "one"
number_density_cm3 = 1.0
modal_radius_um = 0.01
log10_geometric_sigma = 0.1
abundance = 1.0

[[modes]]
name = "two"
number_density_cm3 = 1.0
modal_radius_um = 0.01
log10_geometric_sigma = 0.1
abundance = 1.0

[[modes]]
name = "three"
number_density_cm3 = 1.0
modal_radius_um = 0.01
log10_geometric_sigma = 0.1
abundance = 1.0

[[modes]]
name = "four"
number_density_cm3 = 1.0
modal_radius_um = 0.01
log10_geometric_sigma = 0.1
abundance = 1.0
"#
    }

    fn production_model() -> Model {
        read_model(
            &Path::new(env!("CARGO_MANIFEST_DIR")).join("config/moonlight-aerosol-nsb-v1.toml"),
        )
        .unwrap()
    }

    fn run_config(root: PathBuf) -> RunConfig {
        RunConfig {
            schema_version: 1,
            dataset: DatasetName::MoonlightScattering,
            workspace: WorkspaceConfig { root },
            execution: ExecutionConfig::default(),
            sources: Vec::new(),
            publish: None,
            starlight: None,
        }
    }

    fn source(name: &str) -> SourceConfig {
        SourceConfig {
            name: name.into(),
            path: Some(PathBuf::from(name)),
            url: None,
            sha256: "0".repeat(64),
            product_id: None,
            release: None,
            metadata_url: None,
            retrieved_at: None,
            license: None,
            units: None,
            reference_distance: None,
            partition: None,
        }
    }

    #[test]
    fn model_validation_rejects_bad_contracts() {
        let model = tiny_model();
        validate_model(&model).unwrap();

        let mut bad = model.clone();
        bad.schema = "wrong".into();
        assert!(validate_model(&bad).is_err());

        let mut bad = model.clone();
        bad.radius_intervals = 7;
        assert!(validate_model(&bad).is_err());

        let mut bad = model.clone();
        bad.radius_sigma_bounds = 0.0;
        assert!(validate_model(&bad).is_err());

        let mut bad = model.clone();
        bad.wavelength_step_nm = 0;
        assert!(validate_model(&bad).is_err());

        let mut bad = model.clone();
        bad.wavelength_end_nm = 301;
        bad.wavelength_step_nm = 2;
        assert!(validate_model(&bad).is_err());

        let mut bad = model.clone();
        bad.angle_segments = vec![
            AngleSegment {
                start_deg: 0.0,
                end_deg: 90.0,
                step_deg: 30.0,
            },
            AngleSegment {
                start_deg: 91.0,
                end_deg: 180.0,
                step_deg: 1.0,
            },
        ];
        assert!(validate_model(&bad).is_err());

        let mut bad = model;
        bad.modes[0].abundance = -1.0;
        assert!(validate_model(&bad).is_err());
    }

    #[test]
    fn production_angle_grid_is_refined_and_complete() {
        let angles = angle_grid(&production_model()).unwrap();
        assert_eq!(angles.len(), 355);
        assert_eq!(angles.first(), Some(&0.0));
        assert_eq!(angles.last(), Some(&180.0));
        assert!(angles.windows(2).all(|window| window[0] < window[1]));
        assert!(angles.contains(&0.0125));
        assert!(angles.contains(&2.125));
        assert!(angles.contains(&170.125));
    }

    #[test]
    fn rayleigh_limit_has_correct_absolute_normalization() {
        let mus = [1.0, 0.0, -1.0];
        let sample = mie_phase(1.0e-3, Complex64::new(1.5, 0.0), &mus).unwrap();
        assert!((sample.phase[0] - 1.5).abs() < 1.0e-5);
        assert!((sample.phase[1] - 0.75).abs() < 1.0e-5);
        assert!((sample.phase[2] - 1.5).abs() < 1.0e-5);
        assert!(sample.asymmetry.abs() < 1.0e-5);
        assert!(sample.qsca > 0.0);
    }

    #[test]
    fn single_particle_coefficient_normalization_matches_independent_integral() {
        let angles: Vec<f64> = (0..=1800).map(|index| index as f64 / 10.0).collect();
        let mus: Vec<f64> = angles
            .iter()
            .map(|angle| angle.to_radians().cos())
            .collect();
        let sample = mie_phase(5.0, Complex64::new(1.5, 0.0), &mus).unwrap();
        let (norm, integrated_g) = angular_moments(&angles, &sample.phase);
        assert!(((norm / (4.0 * PI)) - 1.0).abs() < 5.0e-6);
        assert!((integrated_g - sample.asymmetry).abs() < 2.0e-6);
        assert!(sample.phase[0] > sample.phase[900]);
    }

    #[test]
    fn mie_solver_rejects_nonphysical_inputs() {
        assert!(mie_phase(0.0, Complex64::new(1.5, 0.0), &[1.0]).is_err());
        assert!(mie_phase(1.0, Complex64::new(0.0, 0.0), &[1.0]).is_err());
        assert!(mie_phase(f64::NAN, Complex64::new(1.5, 0.0), &[1.0]).is_err());
    }

    #[test]
    fn generation_is_deterministic_and_parseable() {
        let model = tiny_model();
        let first = generate(&model).unwrap();
        let second = generate(&model).unwrap();
        assert_eq!(first, second);
        let grid = parse_grid(&first).unwrap();
        assert_eq!(grid.wavelengths, vec![300.0]);
        assert_eq!(grid.angles.len(), 7);
        assert!(grid
            .values
            .iter()
            .flatten()
            .all(|value| value.is_finite() && *value >= 0.0));
        assert!(first.contains("analytic coefficient normalization"));
    }

    #[test]
    fn parse_grid_fails_closed() {
        assert!(parse_grid("").is_err());
        assert!(parse_grid("0 2\n\n0 180\n").is_err());
        assert!(parse_grid("1 2\n0.3\n0 0\n1 1\n").is_err());
        assert!(parse_grid("1 2\n0.3\n0 180\n1 NaN\n").is_err());
        assert!(parse_grid("2 2\n0.3 0.3\n0 180\n1 1\n1 1\n").is_err());
    }

    #[test]
    fn validate_config_requires_exact_source_order() {
        let root = tempfile::tempdir().unwrap();
        let mut config = run_config(root.path().to_path_buf());
        config.sources = vec![source(MODEL_SOURCE), source(SSCAT)];
        validate_config(&config).unwrap();
        config.sources.swap(0, 1);
        assert!(validate_config(&config).is_err());
    }

    #[test]
    fn build_emits_generated_mie_and_copied_multiple_scattering_artifacts() {
        let temp = tempfile::tempdir().unwrap();
        let workspace = temp.path().join("workspace");
        let sources = workspace.join("sources");
        fs::create_dir_all(&sources).unwrap();
        fs::write(sources.join(MODEL_SOURCE), tiny_model_toml()).unwrap();
        fs::write(sources.join(SSCAT), "first\nsecond\n").unwrap();

        let artifacts = build(&run_config(workspace.clone())).unwrap();
        assert_eq!(artifacts.len(), 2);
        assert!(workspace.join("outputs").join(OUTPUT).is_file());
        assert_eq!(
            fs::read_to_string(workspace.join("outputs").join(SSCAT)).unwrap(),
            "first\nsecond\n"
        );
        assert!(workspace.join("outputs/artifacts.json").is_file());
    }

    #[test]
    fn artifact_validation_checks_all_supported_formats() {
        let temp = tempfile::tempdir().unwrap();
        let sscat = temp.path().join("sscat.dat");
        fs::write(&sscat, "# comment\n1\n2\n").unwrap();
        validate_artifact(SSCAT, &sscat).unwrap();
        fs::write(&sscat, "1\n").unwrap();
        assert!(validate_artifact(SSCAT, &sscat).is_err());

        let mie = temp.path().join("mie.dat");
        fs::write(&mie, "2 2\n0.300 0.650\n0 180\n1 1\n1 1\n").unwrap();
        validate_artifact(OUTPUT, &mie).unwrap();
        fs::write(&mie, "2 2\n0.300 0.650\n0 180\n1 -1\n1 1\n").unwrap();
        assert!(validate_artifact(OUTPUT, &mie).is_err());
        assert!(validate_artifact("unexpected.dat", &mie).is_err());
    }

    #[test]
    fn validation_gates_compare_artifact_with_direct_model() {
        let temp = tempfile::tempdir().unwrap();
        let workspace = temp.path().join("workspace");
        let sources = workspace.join("sources");
        fs::create_dir_all(&sources).unwrap();
        fs::write(sources.join(MODEL_SOURCE), tiny_model_toml()).unwrap();

        let artifact_path = temp.path().join(OUTPUT);
        fs::write(&artifact_path, generate(&tiny_model()).unwrap()).unwrap();
        let artifact = artifact(OUTPUT, &artifact_path).unwrap();
        let gates = validation_gates(&run_config(workspace), &[artifact]).unwrap();
        assert!(gates
            .iter()
            .any(|gate| gate.name == "mie-4pi-normalization"));
        assert!(gates
            .iter()
            .any(|gate| gate.name == "mie-asymmetry-consistency"));
        assert!(gates
            .iter()
            .any(|gate| gate.name == "mie-angular-interpolation"));
        assert!(validation_gates(&run_config(temp.path().join("missing")), &[]).is_err());
    }

    #[test]
    fn production_angular_interpolation_probes_are_bounded() {
        let model = production_model();
        let intervals: [(f64, f64); 5] = [
            (0.0, 0.0125),
            (0.5, 0.525),
            (2.0, 2.125),
            (5.0, 5.25),
            (179.875, 180.0),
        ];
        let mut probe_angles = Vec::new();
        for (lo, hi) in intervals {
            probe_angles.extend([lo, 0.5 * (lo + hi), hi]);
        }

        let mut worst = 0.0_f64;
        for wavelength_um in [0.3, 0.5, 0.65] {
            let (phase, _) = ensemble_phase(&model, wavelength_um, &probe_angles).unwrap();
            for values in phase.chunks_exact(3) {
                let interpolated = 0.5 * (values[0] + values[2]);
                worst = worst.max((interpolated / values[1] - 1.0).abs());
            }
        }
        assert!(worst < 3.0e-3, "worst interpolation error={worst:.6e}");
    }

    #[test]
    #[ignore = "production-size tail-convergence evidence; run explicitly when changing quadrature"]
    fn production_radius_tail_is_converged() {
        let production = production_model();
        let mut seven_sigma = production.clone();
        seven_sigma.radius_sigma_bounds = 7.0;
        seven_sigma.radius_intervals = 1120;
        let angles = [0.0, 0.025, 0.1, 0.5, 2.0, 10.0, 90.0, 180.0];

        let mut worst_phase = 0.0_f64;
        let mut worst_g = 0.0_f64;
        for wavelength_um in [0.3, 0.5, 0.65] {
            let (reference, reference_g) =
                ensemble_phase(&production, wavelength_um, &angles).unwrap();
            let (candidate, candidate_g) =
                ensemble_phase(&seven_sigma, wavelength_um, &angles).unwrap();
            for (&reference, &candidate) in reference.iter().zip(&candidate) {
                worst_phase = worst_phase.max((candidate / reference - 1.0).abs());
            }
            worst_g = worst_g.max((candidate_g - reference_g).abs());
        }
        eprintln!(
            "7 sigma / 1120 versus production: max pointwise relative={worst_phase:.6e}, max |delta g|={worst_g:.6e}"
        );
        assert!(worst_phase < 3.0e-4, "worst phase delta={worst_phase:.6e}");
        assert!(worst_g < 2.0e-8, "worst g delta={worst_g:.6e}");
    }

    #[test]
    #[ignore = "full 36-wavelength regeneration convergence study"]
    fn production_quadrature_convergence() {
        let production = production_model();
        let production_grid = parse_grid(&generate(&production).unwrap()).unwrap();
        let mut candidate = production.clone();
        candidate.radius_sigma_bounds = 7.0;
        candidate.radius_intervals = 1120;
        let candidate_grid = parse_grid(&generate(&candidate).unwrap()).unwrap();

        let mut max_relative = 0.0_f64;
        let mut max_g_delta = 0.0_f64;
        for (reference, trial) in production_grid.values.iter().zip(&candidate_grid.values) {
            for (&a, &b) in reference.iter().zip(trial) {
                max_relative = max_relative.max((b / a - 1.0).abs());
            }
            let (_, reference_g) = angular_moments(&production_grid.angles, reference);
            let (_, trial_g) = angular_moments(&candidate_grid.angles, trial);
            max_g_delta = max_g_delta.max((trial_g - reference_g).abs());
        }
        eprintln!(
            "7 sigma / 1120 versus production: max pointwise relative={max_relative:.6e}, max |delta g|={max_g_delta:.6e}"
        );
    }
}
