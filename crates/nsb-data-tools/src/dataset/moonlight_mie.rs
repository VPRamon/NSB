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
    angle_start_deg: usize,
    angle_end_deg: usize,
    angle_step_deg: usize,
    modes: Vec<Mode>,
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

pub fn validation_gates(artifacts: &[Artifact]) -> Result<Vec<ValidationGate>> {
    let artifact = artifacts
        .iter()
        .find(|a| a.name == OUTPUT)
        .context("missing Mie artifact")?;
    let grid = parse_grid(&fs::read_to_string(&artifact.path)?)?;
    let mut worst_norm = 0.0_f64;
    let mut min_g = f64::INFINITY;
    let mut max_g = f64::NEG_INFINITY;
    for row in &grid.values {
        let (norm, g) = angular_moments(&grid.angles, row);
        worst_norm = worst_norm.max((norm - 4.0 * PI).abs() / (4.0 * PI));
        min_g = min_g.min(g);
        max_g = max_g.max(g);
    }
    let forward = grid.values.iter().all(|row| row[0] > row[row.len() / 2]);
    Ok(vec![
        ValidationGate {
            name: "mie-4pi-normalization".into(),
            passed: worst_norm < 5.0e-4,
            detail: format!("worst relative trapezoid error={worst_norm:.3e}"),
        },
        ValidationGate {
            name: "mie-asymmetry-range".into(),
            passed: min_g > 0.4 && max_g < 0.95,
            detail: format!("g range={min_g:.6}..{max_g:.6}"),
        },
        ValidationGate {
            name: "mie-forward-scattering".into(),
            passed: forward,
            detail: "P(0 degrees) exceeds P(90 degrees) at every wavelength".into(),
        },
    ])
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
    if model.schema != "nsb-moonlight-aerosol-model-v1"
        || model.solver_version != "nsb-bh-mie-v1"
        || model.distribution != "lognormal-number-per-log10-radius"
        || model.normalization != "integral-over-solid-angle-equals-4pi"
        || model.reference.trim().is_empty()
        || model.modes.len() != 4
        || model.radius_intervals == 0
        || model.radius_intervals % 2 != 0
        || model.wavelength_step_nm == 0
        || model.angle_step_deg == 0
    {
        bail!("invalid NSB Mie model configuration");
    }
    Ok(model)
}

fn generate(model: &Model) -> Result<String> {
    let wavelengths: Vec<_> = (model.wavelength_start_nm..=model.wavelength_end_nm)
        .step_by(model.wavelength_step_nm)
        .collect();
    let angles: Vec<_> = (model.angle_start_deg..=model.angle_end_deg)
        .step_by(model.angle_step_deg)
        .collect();
    let mus: Vec<_> = angles
        .iter()
        .map(|a| (*a as f64).to_radians().cos())
        .collect();
    let refractive = Complex64::new(
        model.refractive_index_real,
        model.refractive_index_imaginary,
    );
    let mut rows = Vec::with_capacity(wavelengths.len());
    for &wavelength_nm in &wavelengths {
        let mut row = vec![0.0; angles.len()];
        let mut scattering_weight = 0.0;
        for mode in &model.modes {
            integrate_mode(
                model,
                mode,
                wavelength_nm as f64 / 1000.0,
                refractive,
                &mus,
                &mut row,
                &mut scattering_weight,
            )?;
        }
        if scattering_weight <= 0.0 {
            bail!("zero scattering weight");
        }
        for value in &mut row {
            *value /= scattering_weight;
        }
        let (norm, _) =
            angular_moments(&angles.iter().map(|a| *a as f64).collect::<Vec<_>>(), &row);
        for value in &mut row {
            *value *= 4.0 * PI / norm;
        }
        rows.push(row);
    }
    let mut out = String::new();
    writeln!(out, "# schema = nsb-moonlight-mie-phase-v1")?;
    writeln!(out, "# aerosol_schema = nsb-moonlight-aerosol-model-v1")?;
    writeln!(
        out,
        "# solver = Bohren-Huffman amplitude recurrences, in-tree implementation v1"
    )?;
    writeln!(out, "# normalization = integral P(theta) dOmega = 4*pi")?;
    writeln!(out, "# wavelength_grid_nm = 300..650 step 10")?;
    writeln!(out, "# scattering_angle_grid_deg = 0..180 step 1")?;
    writeln!(
        out,
        "# radius_quadrature = composite Simpson in ln(radius), 800 intervals over +/-5 sigma"
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
            .map(|a| a.to_string())
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

fn integrate_mode(
    model: &Model,
    mode: &Mode,
    wavelength_um: f64,
    refractive: Complex64,
    mus: &[f64],
    total: &mut [f64],
    total_weight: &mut f64,
) -> Result<()> {
    if mode.name.trim().is_empty()
        || mode.modal_radius_um <= 0.0
        || mode.log10_geometric_sigma <= 0.0
        || mode.number_density_cm3 < 0.0
        || mode.abundance < 0.0
    {
        bail!("invalid aerosol mode");
    }
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
        } else if i % 2 == 0 {
            2.0
        } else {
            4.0
        };
        let x = 2.0 * PI * radius / wavelength_um;
        let (qsca, phase) = mie_phase(x, refractive, mus)?;
        let weight = simpson * h / 3.0 * number_per_ln_r * PI * radius * radius * qsca;
        *total_weight += weight;
        for (sum, value) in total.iter_mut().zip(phase) {
            *sum += weight * value;
        }
    }
    Ok(())
}

/// Return scattering efficiency and a phase function normalized analytically to 4 pi.
fn mie_phase(x: f64, m: Complex64, mus: &[f64]) -> Result<(f64, Vec<f64>)> {
    if x <= 0.0 {
        bail!("non-positive size parameter");
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
    for n in 1..=nstop {
        let nf = n as f64;
        let psi = (2.0 * nf - 1.0) / x * psi1 - psi0;
        let chi = (2.0 * nf - 1.0) / x * chi1 - chi0;
        let xi = Complex64::new(psi, -chi);
        let xi1 = Complex64::new(psi1, -chi1);
        let da = d[n] / m + Complex64::new(nf / x, 0.0);
        let db = m * d[n] + Complex64::new(nf / x, 0.0);
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
    let qsca = 2.0 * qsum / (x * x);
    let mut phase = Vec::with_capacity(mus.len());
    for &mu in mus {
        let mut pi_nm1 = 0.0;
        let mut pi_n = 1.0;
        let mut s1 = Complex64::new(0.0, 0.0);
        let mut s2 = Complex64::new(0.0, 0.0);
        for n in 1..=nstop {
            let nf = n as f64;
            let tau = nf * mu * pi_n - (nf + 1.0) * pi_nm1;
            let factor = (2.0 * nf + 1.0) / (nf * (nf + 1.0));
            s1 += factor * (an[n - 1] * pi_n + bn[n - 1] * tau);
            s2 += factor * (an[n - 1] * tau + bn[n - 1] * pi_n);
            let next = ((2.0 * nf + 1.0) * mu * pi_n - (nf + 1.0) * pi_nm1) / nf;
            pi_nm1 = pi_n;
            pi_n = next;
        }
        phase.push(2.0 * (s1.norm_sqr() + s2.norm_sqr()) / qsum);
    }
    Ok((qsca, phase))
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
    if dims.len() != 2 {
        bail!("bad dimensions");
    }
    let wavelengths = parse_row(lines.next().context("missing wavelengths")?)?;
    let angles = parse_row(lines.next().context("missing angles")?)?;
    let values: Vec<Vec<f64>> = lines.map(parse_row).collect::<Result<_>>()?;
    if wavelengths.len() != dims[0]
        || angles.len() != dims[1]
        || values.len() != dims[0]
        || values.iter().any(|r| r.len() != dims[1])
    {
        bail!("grid dimension mismatch");
    }
    if !wavelengths.windows(2).all(|w| w[0] < w[1]) || !angles.windows(2).all(|w| w[0] < w[1]) {
        bail!("axes must be strictly increasing");
    }
    Ok(Grid {
        wavelengths: wavelengths.into_iter().map(|v| v * 1000.0).collect(),
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
    #[test]
    fn single_particle_phase_is_normalized_and_forward_scattering() {
        let angles: Vec<f64> = (0..=180).map(|v| v as f64).collect();
        let mus: Vec<f64> = angles.iter().map(|v| v.to_radians().cos()).collect();
        let (_, mut p) = mie_phase(5.0, Complex64::new(1.5, 0.0), &mus).unwrap();
        let (norm, _) = angular_moments(&angles, &p);
        for v in &mut p {
            *v *= 4.0 * PI / norm;
        }
        let (norm, g) = angular_moments(&angles, &p);
        assert!((norm - 4.0 * PI).abs() < 1e-10);
        assert!(p[0] > p[90]);
        assert!((-1.0..=1.0).contains(&g));
    }

    #[test]
    #[ignore = "documented production convergence study"]
    fn production_quadrature_convergence() {
        let path =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("config/moonlight-aerosol-nsb-v1.toml");
        let production = read_model(&path).unwrap();
        let production_grid = parse_grid(&generate(&production).unwrap()).unwrap();
        for (label, mut candidate) in [
            ("400-radius-intervals", production.clone()),
            ("four-sigma-radius-bounds", production.clone()),
            ("six-sigma-radius-bounds", production.clone()),
        ] {
            if label.starts_with("400") {
                candidate.radius_intervals = 400;
            } else if label.starts_with("four") {
                candidate.radius_sigma_bounds = 4.0;
            } else {
                candidate.radius_sigma_bounds = 6.0;
            }
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
                "{label}: max pointwise relative={max_relative:.6e}, max |delta g|={max_g_delta:.6e}"
            );
        }
    }
}
