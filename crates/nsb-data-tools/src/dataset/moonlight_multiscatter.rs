//! Reproducible multiple-scattering correction for the Jones moonlight model.
//!
//! The production calculation is an order-resolved, forced-collision Monte
//! Carlo solution of scalar radiative transfer in a stratified plane-parallel
//! atmosphere.  It estimates `I_DS / I_SS`; the higher orders use the explicit
//! geometric-series approximation and 0.9 cap from Jones et al. (2013),
//! Sect. 2.4.3.  The historical ESO correction is never read here.

use super::{Artifact, RunConfig, ValidationGate};
use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use std::f64::consts::PI;
use std::fmt::Write;
use std::fs;
use std::path::{Path, PathBuf};

pub const OUTPUT: &str = "moonlight_multiscatter_nsb_v1.dat";
const AEROSOL_CONFIG: &str = "moonlight-aerosol-nsb-v1.toml";
const MIE_INPUT: &str = "moonlight_mie_nsb_v1.dat";
const RT_CONFIG: &str = "moonlight-multiscatter-nsb-v1.toml";
const PARTITION_SCHEMA: &str = "nsb-moonlight-multiscatter-partition-v1";

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RtConfig {
    schema: String,
    solver: String,
    reference: String,
    mie_schema: String,
    mie_artifact_sha256: String,
    aerosol_model_sha256: String,
    wavelength_start_nm: u32,
    wavelength_end_nm: u32,
    wavelength_step_nm: u32,
    separation_deg: Vec<u32>,
    photons_per_replicate: usize,
    replicates: usize,
    seed_namespace: String,
    ratio_cap: f64,
    rayleigh_pressure_hpa: f64,
    observer_altitude_km: f64,
    rayleigh_scale_height_km: f64,
    aerosol_scale_height_km: f64,
    aerosol_single_scattering_albedo: f64,
    aerosol_extinction_mag_per_airmass_500nm: f64,
    aerosol_angstrom_exponent: f64,
    geometry_zenith_floor_deg: Vec<f64>,
    geometry_zenith_cap_deg: f64,
    serialization_digits: usize,
}

#[derive(Clone, Debug)]
struct PhaseGrid {
    wavelengths_nm: Vec<f64>,
    angles_rad: Vec<f64>,
    rows: Vec<Vec<f64>>,
}

#[derive(Clone, Debug)]
struct PhaseSampler<'a> {
    angles_rad: &'a [f64],
    values: &'a [f64],
    cdf: Vec<f64>,
}

#[derive(Clone, Copy, Debug)]
enum Species {
    Rayleigh,
    Aerosol,
}

#[derive(Clone, Copy, Debug)]
struct Vec3 {
    x: f64,
    y: f64,
    z: f64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct PartitionResult {
    schema: String,
    partition: String,
    wavelength_nm: u32,
    config_sha256: String,
    aerosol_model_sha256: String,
    mie_artifact_sha256: String,
    photons_per_replicate: usize,
    replicates: usize,
    cells: Vec<CellResult>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct CellResult {
    separation_deg: u32,
    double_to_single: f64,
    standard_error: f64,
    correction: f64,
}

#[derive(Clone, Debug)]
struct SplitMix64(u64);

impl SplitMix64 {
    fn next_u64(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9e37_79b9_7f4a_7c15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        z ^ (z >> 31)
    }

    fn uniform(&mut self) -> f64 {
        let bits = self.next_u64() >> 11;
        ((bits as f64) + 0.5) * (1.0 / ((1_u64 << 53) as f64))
    }
}

pub fn validate_config(config: &RunConfig) -> Result<()> {
    super::moonlight_mie::validate_config(config)?;
    let names = config
        .sources
        .iter()
        .map(|source| source.name.as_str())
        .collect::<Vec<_>>();
    if names != [AEROSOL_CONFIG, MIE_INPUT, RT_CONFIG] {
        bail!(
            "moonlight sources must be ordered as {AEROSOL_CONFIG:?}, {MIE_INPUT:?}, {RT_CONFIG:?}"
        );
    }
    let rt = read_rt_source(config)?;
    validate_rt(&rt)?;
    let aerosol = config
        .sources
        .iter()
        .find(|source| source.name == AEROSOL_CONFIG)
        .context("missing aerosol model source")?;
    let mie = config
        .sources
        .iter()
        .find(|source| source.name == MIE_INPUT)
        .context("missing Mie source")?;
    if aerosol.sha256 != rt.aerosol_model_sha256 || mie.sha256 != rt.mie_artifact_sha256 {
        bail!("#216 aerosol/Mie identities do not match the #217 RT contract");
    }
    Ok(())
}

pub fn partitions(config: &RunConfig) -> Result<Vec<String>> {
    let rt = read_rt_source(config)?;
    validate_rt(&rt)?;
    Ok(wavelengths(&rt).map(partition_name).collect())
}

pub fn build(config: &RunConfig, selected: &[String]) -> Result<Vec<Artifact>> {
    let rt = read_rt_workspace(config)?;
    validate_rt(&rt)?;
    let mie_path = config.workspace.root.join("sources").join(MIE_INPUT);
    let grid = parse_phase_grid(&fs::read_to_string(&mie_path)?)?;
    verify_phase_identity(config, &rt, &mie_path, &grid)?;
    let requested = if selected.is_empty() {
        partitions(config)?
    } else {
        selected.to_vec()
    };
    let output_root = config.workspace.root.join("outputs/partitions");
    fs::create_dir_all(&output_root)?;
    let config_sha256 = source_sha(config, RT_CONFIG)?;
    let mut artifacts = Vec::with_capacity(requested.len());
    for partition in requested {
        let wavelength_nm = parse_partition(&partition)?;
        if !wavelengths(&rt).any(|value| value == wavelength_nm) {
            bail!("partition {partition:?} is outside the configured wavelength grid");
        }
        let row_index = grid
            .wavelengths_nm
            .iter()
            .position(|value| (*value - f64::from(wavelength_nm)).abs() < 1.0e-9)
            .with_context(|| format!("Mie grid has no {wavelength_nm} nm row"))?;
        let sampler = PhaseSampler::new(&grid.angles_rad, &grid.rows[row_index])?;
        let cells = rt
            .separation_deg
            .iter()
            .map(|&separation| simulate_cell(&rt, &sampler, wavelength_nm, separation))
            .collect::<Result<Vec<_>>>()?;
        let result = PartitionResult {
            schema: PARTITION_SCHEMA.into(),
            partition: partition.clone(),
            wavelength_nm,
            config_sha256: config_sha256.clone(),
            aerosol_model_sha256: rt.aerosol_model_sha256.clone(),
            mie_artifact_sha256: rt.mie_artifact_sha256.clone(),
            photons_per_replicate: rt.photons_per_replicate,
            replicates: rt.replicates,
            cells,
        };
        let path = output_root.join(format!("{partition}.json"));
        super::engine::atomic_write(&path, &serde_json::to_vec_pretty(&result)?)?;
        artifacts.push(artifact(&format!("partitions/{partition}.json"), &path)?);
    }
    artifacts.sort_by(|left, right| left.name.cmp(&right.name));
    super::engine::atomic_write(
        &config.workspace.root.join("outputs/artifacts.json"),
        &serde_json::to_vec_pretty(&artifacts)?,
    )?;
    Ok(artifacts)
}

pub fn finalize(config: &RunConfig) -> Result<Vec<Artifact>> {
    let rt = read_rt_workspace(config)?;
    validate_rt(&rt)?;
    let expected = partitions(config)?;
    let config_sha256 = source_sha(config, RT_CONFIG)?;
    let mut rows = Vec::with_capacity(expected.len());
    for partition in &expected {
        let local = config
            .workspace
            .root
            .join("outputs/partitions")
            .join(format!("{partition}.json"));
        let worker = config
            .workspace
            .root
            .join("workers")
            .join(partition)
            .join("outputs/partitions")
            .join(format!("{partition}.json"));
        let path = select_partition_path(&local, &worker, partition)?;
        verify_partition_checksum(&path, partition)?;
        let result: PartitionResult = serde_json::from_slice(&fs::read(&path)?)
            .with_context(|| format!("invalid partition {}", path.display()))?;
        validate_partition(&result, partition, &config_sha256, &rt)?;
        rows.push(result);
    }
    let output_root = config.workspace.root.join("outputs");
    fs::create_dir_all(&output_root)?;
    let serialized = serialize_grid(&rt, &rows)?;
    let output_path = output_root.join(OUTPUT);
    super::engine::atomic_write(&output_path, serialized.as_bytes())?;
    let regenerated = super::moonlight_mie::build(config)?;
    let regenerated_mie = regenerated
        .iter()
        .find(|artifact| artifact.name == MIE_INPUT)
        .context("Mie generator did not produce its canonical artifact")?;
    if regenerated_mie.sha256 != rt.mie_artifact_sha256 {
        bail!("regenerated #216 Mie bytes do not match the admitted input identity");
    }
    let source_mie = config.workspace.root.join("sources").join(MIE_INPUT);
    let output_mie = output_root.join(MIE_INPUT);
    super::engine::atomic_write(&output_mie, &fs::read(source_mie)?)?;
    let artifacts = vec![
        artifact(MIE_INPUT, &output_mie)?,
        artifact(OUTPUT, &output_path)?,
    ];
    super::engine::atomic_write(
        &output_root.join("artifacts.json"),
        &serde_json::to_vec_pretty(&artifacts)?,
    )?;
    Ok(artifacts)
}

pub fn validate_artifact(name: &str, path: &Path) -> Result<()> {
    if name == MIE_INPUT {
        return super::moonlight_mie::validate_artifact(name, path);
    }
    if name != OUTPUT {
        bail!("unexpected moonlight artifact {name:?}");
    }
    let grid = parse_phase_grid(&fs::read_to_string(path)?)?;
    if grid.wavelengths_nm.first() != Some(&300.0)
        || grid.wavelengths_nm.last() != Some(&650.0)
        || grid.angles_rad.first() != Some(&0.0)
        || (grid.angles_rad.last().copied().unwrap_or_default() - PI).abs() > 1.0e-12
    {
        bail!("multiple-scattering grid must span 300--650 nm and 0--180 degrees");
    }
    if grid
        .rows
        .iter()
        .flatten()
        .any(|value| !value.is_finite() || !(1.0..=10.0).contains(value))
    {
        bail!("correction values must be finite and within the Jones cap [1, 10]");
    }
    Ok(())
}

pub fn validation_gates(config: &RunConfig, artifacts: &[Artifact]) -> Result<Vec<ValidationGate>> {
    let rt = read_rt_workspace(config)?;
    let correction = artifacts
        .iter()
        .find(|artifact| artifact.name == OUTPUT)
        .context("missing multiple-scattering artifact")?;
    let grid = parse_phase_grid(&fs::read_to_string(&correction.path)?)?;
    let max_adjacent_jump = grid
        .rows
        .iter()
        .flat_map(|row| row.windows(2))
        .map(|pair| (pair[1] - pair[0]).abs())
        .fold(0.0_f64, f64::max);
    let mut max_standard_error = 0.0_f64;
    for partition in partitions(config)? {
        let local = config
            .workspace
            .root
            .join("outputs/partitions")
            .join(format!("{partition}.json"));
        let worker = config
            .workspace
            .root
            .join("workers")
            .join(&partition)
            .join("outputs/partitions")
            .join(format!("{partition}.json"));
        let path = select_partition_path(&local, &worker, &partition)?;
        let result: PartitionResult = serde_json::from_slice(&fs::read(path)?)?;
        max_standard_error = result
            .cells
            .iter()
            .map(|cell| cell.standard_error)
            .fold(max_standard_error, f64::max);
    }
    let mut gates = super::moonlight_mie::validation_gates(config, artifacts)?;
    gates.extend([
        ValidationGate {
            name: "multiscatter-physical-range".into(),
            passed: grid
                .rows
                .iter()
                .flatten()
                .all(|value| value.is_finite() && (1.0..=10.0).contains(value)),
            detail: "all factors are finite and respect the Jones ratio cap".into(),
        },
        ValidationGate {
            name: "multiscatter-angular-continuity".into(),
            passed: max_adjacent_jump < 1.0,
            detail: format!("largest adjacent angular jump={max_adjacent_jump:.6}"),
        },
        ValidationGate {
            name: "multiscatter-statistical-precision".into(),
            passed: max_standard_error < 0.03,
            detail: format!(
                "largest replicate standard error in I_DS/I_SS={max_standard_error:.6}"
            ),
        },
        ValidationGate {
            name: "multiscatter-mie-compatibility".into(),
            passed: rt.mie_artifact_sha256 == source_sha(config, MIE_INPUT)?
                && rt.aerosol_model_sha256 == source_sha(config, AEROSOL_CONFIG)?,
            detail: format!(
                "aerosol={} mie={}",
                rt.aerosol_model_sha256, rt.mie_artifact_sha256
            ),
        },
    ]);
    Ok(gates)
}

fn simulate_cell(
    rt: &RtConfig,
    mie: &PhaseSampler<'_>,
    wavelength_nm: u32,
    separation_deg: u32,
) -> Result<CellResult> {
    if separation_deg == 180 {
        let mut boundary = simulate_cell(rt, mie, wavelength_nm, 140)?;
        boundary.separation_deg = 180;
        return Ok(boundary);
    }
    let mut estimates = Vec::new();
    for (geometry_index, &floor) in rt.geometry_zenith_floor_deg.iter().enumerate() {
        let zenith = floor
            .max(f64::from(separation_deg) * 0.5)
            .min(rt.geometry_zenith_cap_deg);
        let (source, observer) = symmetric_geometry(zenith, f64::from(separation_deg))?;
        for replicate in 0..rt.replicates {
            let seed = stable_seed(&format!(
                "{}:{wavelength_nm}:{separation_deg}:{geometry_index}:{replicate}",
                rt.seed_namespace
            ));
            let ratio = simulate_ratio(rt, mie, wavelength_nm, source, observer, seed)?;
            estimates.push(ratio);
        }
    }
    let mean = estimates.iter().sum::<f64>() / estimates.len() as f64;
    let variance = if estimates.len() > 1 {
        estimates
            .iter()
            .map(|value| (value - mean).powi(2))
            .sum::<f64>()
            / (estimates.len() - 1) as f64
    } else {
        0.0
    };
    let standard_error = (variance / estimates.len() as f64).sqrt();
    let bounded = mean.clamp(0.0, rt.ratio_cap);
    Ok(CellResult {
        separation_deg,
        double_to_single: mean,
        standard_error,
        correction: 1.0 / (1.0 - bounded),
    })
}

fn simulate_ratio(
    rt: &RtConfig,
    mie: &PhaseSampler<'_>,
    wavelength_nm: u32,
    source: Vec3,
    observer: Vec3,
    seed: u64,
) -> Result<f64> {
    let wavelength_um = f64::from(wavelength_nm) / 1000.0;
    let tau_r = rayleigh_optical_depth(rt, wavelength_um);
    let tau_a = rt.aerosol_extinction_mag_per_airmass_500nm / 1.085_736_204_758_129_6
        * (wavelength_um / 0.5).powf(rt.aerosol_angstrom_exponent);
    let total = tau_r + tau_a;
    if !total.is_finite() || total <= 0.0 || observer.z <= 0.0 || source.z <= 0.0 {
        bail!("invalid optical depth or geometry");
    }
    let mut rng = SplitMix64(seed);
    let mut single = 0.0;
    let mut double = 0.0;
    for _ in 0..rt.photons_per_replicate {
        let mut t = 0.0;
        let mut direction = source;
        let mut weight = 1.0;
        for order in 1..=2 {
            let boundary_distance = if direction.z > 1.0e-12 {
                (total - t) / direction.z
            } else if direction.z < -1.0e-12 {
                -t / direction.z
            } else {
                break;
            };
            if boundary_distance <= 0.0 {
                break;
            }
            let collision_probability = -(-boundary_distance).exp_m1();
            weight *= collision_probability;
            let distance = -(1.0 - rng.uniform() * collision_probability).ln();
            t = (t + direction.z * distance).clamp(0.0, total);
            let rayleigh_probability = local_rayleigh_fraction(rt, tau_r, tau_a, total - t);
            let species = if rng.uniform() < rayleigh_probability {
                Species::Rayleigh
            } else {
                Species::Aerosol
            };
            if matches!(species, Species::Aerosol) {
                weight *= rt.aerosol_single_scattering_albedo;
            }
            let cos_scatter = dot(direction, observer).clamp(-1.0, 1.0);
            let phase = match species {
                Species::Rayleigh => rayleigh_phase(cos_scatter),
                Species::Aerosol => mie.lookup(cos_scatter.acos()),
            };
            let transmittance = (-(total - t) / observer.z).exp();
            let contribution = weight * phase * transmittance / (4.0 * PI * observer.z);
            if order == 1 {
                single += contribution;
            } else {
                double += contribution;
            }
            let (theta, phi) = match species {
                Species::Rayleigh => (sample_rayleigh(&mut rng), 2.0 * PI * rng.uniform()),
                Species::Aerosol => (mie.sample(&mut rng), 2.0 * PI * rng.uniform()),
            };
            direction = rotate(direction, theta, phi);
        }
    }
    if !single.is_finite() || !double.is_finite() || single <= 0.0 || double < 0.0 {
        bail!("non-finite order-resolved Monte Carlo estimate");
    }
    Ok(double / single)
}

fn rayleigh_optical_depth(rt: &RtConfig, wavelength_um: f64) -> f64 {
    let exponent = -(3.9 + 0.074 * wavelength_um + 0.050 / wavelength_um);
    rt.rayleigh_pressure_hpa / 1013.0
        * (8.6e-3 + 6.5e-6 * rt.observer_altitude_km)
        * wavelength_um.powf(exponent)
}

fn local_rayleigh_fraction(rt: &RtConfig, tau_r: f64, tau_a: f64, remaining: f64) -> f64 {
    let mut low = 0.0;
    let mut high = 200.0;
    for _ in 0..60 {
        let height = 0.5 * (low + high);
        let column = tau_r * (-height / rt.rayleigh_scale_height_km).exp()
            + tau_a * (-height / rt.aerosol_scale_height_km).exp();
        if column > remaining {
            low = height;
        } else {
            high = height;
        }
    }
    let height = 0.5 * (low + high);
    let rayleigh_rate =
        tau_r / rt.rayleigh_scale_height_km * (-height / rt.rayleigh_scale_height_km).exp();
    let aerosol_rate =
        tau_a / rt.aerosol_scale_height_km * (-height / rt.aerosol_scale_height_km).exp();
    rayleigh_rate / (rayleigh_rate + aerosol_rate)
}

fn symmetric_geometry(zenith_deg: f64, separation_deg: f64) -> Result<(Vec3, Vec3)> {
    let z = zenith_deg.to_radians();
    let rho = separation_deg.to_radians();
    let denominator = z.sin().powi(2);
    let cos_delta = if denominator < 1.0e-15 {
        if separation_deg.abs() < 1.0e-12 {
            1.0
        } else {
            bail!("zero-zenith geometry cannot represent nonzero separation");
        }
    } else {
        ((rho.cos() - z.cos().powi(2)) / denominator).clamp(-1.0, 1.0)
    };
    let delta = cos_delta.acos();
    Ok((
        Vec3 {
            x: z.sin(),
            y: 0.0,
            z: z.cos(),
        },
        Vec3 {
            x: z.sin() * delta.cos(),
            y: z.sin() * delta.sin(),
            z: z.cos(),
        },
    ))
}

impl<'a> PhaseSampler<'a> {
    fn new(angles_rad: &'a [f64], values: &'a [f64]) -> Result<Self> {
        if angles_rad.len() != values.len() || angles_rad.len() < 2 {
            bail!("invalid Mie phase row");
        }
        let mut cdf = Vec::with_capacity(angles_rad.len());
        cdf.push(0.0);
        for index in 1..angles_rad.len() {
            let a = angles_rad[index - 1];
            let b = angles_rad[index];
            let area = 0.5 * (values[index - 1] * a.sin() + values[index] * b.sin()) * (b - a);
            cdf.push(cdf[index - 1] + area.max(0.0));
        }
        let norm = *cdf.last().context("empty Mie CDF")?;
        if !norm.is_finite() || norm <= 0.0 {
            bail!("Mie phase row has zero/non-finite integral");
        }
        for value in &mut cdf {
            *value /= norm;
        }
        Ok(Self {
            angles_rad,
            values,
            cdf,
        })
    }

    fn lookup(&self, angle: f64) -> f64 {
        interpolate(self.angles_rad, self.values, angle)
    }

    fn sample(&self, rng: &mut SplitMix64) -> f64 {
        let u = rng.uniform();
        let upper = self
            .cdf
            .partition_point(|value| *value <= u)
            .min(self.cdf.len() - 1);
        let lower = upper.saturating_sub(1);
        let span = self.cdf[upper] - self.cdf[lower];
        let fraction = if span > 0.0 {
            (u - self.cdf[lower]) / span
        } else {
            0.0
        };
        self.angles_rad[lower] + fraction * (self.angles_rad[upper] - self.angles_rad[lower])
    }
}

fn sample_rayleigh(rng: &mut SplitMix64) -> f64 {
    loop {
        let cosine = 2.0 * rng.uniform() - 1.0;
        if rng.uniform() <= 0.5 * (1.0 + cosine * cosine) {
            return cosine.acos();
        }
    }
}

fn rayleigh_phase(cosine: f64) -> f64 {
    0.75 * (1.0 + cosine * cosine)
}

fn rotate(direction: Vec3, theta: f64, phi: f64) -> Vec3 {
    let reference = if direction.z.abs() < 0.9 {
        Vec3 {
            x: 0.0,
            y: 0.0,
            z: 1.0,
        }
    } else {
        Vec3 {
            x: 1.0,
            y: 0.0,
            z: 0.0,
        }
    };
    let e1 = normalize(cross(reference, direction));
    let e2 = cross(direction, e1);
    normalize(Vec3 {
        x: direction.x * theta.cos() + theta.sin() * (e1.x * phi.cos() + e2.x * phi.sin()),
        y: direction.y * theta.cos() + theta.sin() * (e1.y * phi.cos() + e2.y * phi.sin()),
        z: direction.z * theta.cos() + theta.sin() * (e1.z * phi.cos() + e2.z * phi.sin()),
    })
}

fn dot(a: Vec3, b: Vec3) -> f64 {
    a.x * b.x + a.y * b.y + a.z * b.z
}

fn cross(a: Vec3, b: Vec3) -> Vec3 {
    Vec3 {
        x: a.y * b.z - a.z * b.y,
        y: a.z * b.x - a.x * b.z,
        z: a.x * b.y - a.y * b.x,
    }
}

fn normalize(value: Vec3) -> Vec3 {
    let norm = dot(value, value).sqrt();
    Vec3 {
        x: value.x / norm,
        y: value.y / norm,
        z: value.z / norm,
    }
}

fn stable_seed(value: &str) -> u64 {
    value
        .as_bytes()
        .iter()
        .fold(0xcbf2_9ce4_8422_2325, |hash, byte| {
            (hash ^ u64::from(*byte)).wrapping_mul(0x0000_0100_0000_01b3)
        })
}

fn read_rt_source(config: &RunConfig) -> Result<RtConfig> {
    let path = config
        .sources
        .iter()
        .find(|source| source.name == RT_CONFIG)
        .and_then(|source| source.path.as_ref())
        .context("missing local RT configuration source")?;
    read_rt(path)
}

fn read_rt_workspace(config: &RunConfig) -> Result<RtConfig> {
    read_rt(&config.workspace.root.join("sources").join(RT_CONFIG))
}

fn read_rt(path: &Path) -> Result<RtConfig> {
    let raw = fs::read_to_string(path)
        .with_context(|| format!("failed to read RT config {}", path.display()))?;
    toml::from_str(&raw).context("invalid multiple-scattering RT configuration")
}

fn validate_rt(rt: &RtConfig) -> Result<()> {
    if rt.schema != "nsb-moonlight-multiscatter-model-v1"
        || rt.solver != "nsb-forced-collision-plane-parallel-mc-v1"
        || rt.mie_schema != "nsb-moonlight-mie-phase-v1"
        || rt.reference.trim().is_empty()
        || rt.seed_namespace.trim().is_empty()
        || rt.wavelength_step_nm == 0
        || rt.wavelength_start_nm > rt.wavelength_end_nm
        || !(rt.wavelength_end_nm - rt.wavelength_start_nm).is_multiple_of(rt.wavelength_step_nm)
        || rt.separation_deg.first() != Some(&0)
        || rt.separation_deg.last() != Some(&180)
        || !rt.separation_deg.windows(2).all(|pair| pair[0] < pair[1])
        || rt.separation_deg.iter().any(|value| *value > 180)
        || rt.photons_per_replicate < 1024
        || rt.replicates < 2
        || !rt.ratio_cap.is_finite()
        || !(0.0..1.0).contains(&rt.ratio_cap)
        || !rt.rayleigh_pressure_hpa.is_finite()
        || rt.rayleigh_pressure_hpa <= 0.0
        || !rt.observer_altitude_km.is_finite()
        || rt.observer_altitude_km < 0.0
        || !rt.rayleigh_scale_height_km.is_finite()
        || rt.rayleigh_scale_height_km <= 0.0
        || !rt.aerosol_scale_height_km.is_finite()
        || rt.aerosol_scale_height_km <= 0.0
        || !rt.aerosol_single_scattering_albedo.is_finite()
        || !(0.0..=1.0).contains(&rt.aerosol_single_scattering_albedo)
        || !rt.aerosol_extinction_mag_per_airmass_500nm.is_finite()
        || rt.aerosol_extinction_mag_per_airmass_500nm < 0.0
        || !rt.aerosol_angstrom_exponent.is_finite()
        || rt.geometry_zenith_floor_deg.is_empty()
        || !rt.geometry_zenith_cap_deg.is_finite()
        || !(0.0..90.0).contains(&rt.geometry_zenith_cap_deg)
        || rt
            .geometry_zenith_floor_deg
            .iter()
            .any(|floor| !floor.is_finite() || *floor < 0.0 || *floor > rt.geometry_zenith_cap_deg)
        || !(6..=17).contains(&rt.serialization_digits)
        || !is_sha256(&rt.mie_artifact_sha256)
        || !is_sha256(&rt.aerosol_model_sha256)
    {
        bail!("invalid multiple-scattering RT configuration");
    }
    Ok(())
}

fn verify_phase_identity(
    config: &RunConfig,
    rt: &RtConfig,
    path: &Path,
    grid: &PhaseGrid,
) -> Result<()> {
    let actual = crate::platform::checksum_io::sha256_file(path)?;
    if actual != rt.mie_artifact_sha256 || actual != source_sha(config, MIE_INPUT)? {
        bail!("canonical Mie artifact checksum mismatch");
    }
    if grid.wavelengths_nm.first() != Some(&300.0) || grid.wavelengths_nm.last() != Some(&650.0) {
        bail!("canonical Mie artifact has incompatible wavelength coverage");
    }
    Ok(())
}

fn source_sha(config: &RunConfig, name: &str) -> Result<String> {
    config
        .sources
        .iter()
        .find(|source| source.name == name)
        .map(|source| source.sha256.clone())
        .with_context(|| format!("missing source {name:?}"))
}

fn wavelengths(rt: &RtConfig) -> impl Iterator<Item = u32> + '_ {
    (rt.wavelength_start_nm..=rt.wavelength_end_nm).step_by(rt.wavelength_step_nm as usize)
}

fn partition_name(wavelength_nm: u32) -> String {
    format!("w{wavelength_nm:04}")
}

fn parse_partition(partition: &str) -> Result<u32> {
    partition
        .strip_prefix('w')
        .context("moonlight partition must start with 'w'")?
        .parse()
        .context("moonlight partition has invalid wavelength")
}

fn validate_partition(
    result: &PartitionResult,
    expected_partition: &str,
    config_sha256: &str,
    rt: &RtConfig,
) -> Result<()> {
    if result.schema != PARTITION_SCHEMA
        || result.partition != expected_partition
        || result.wavelength_nm != parse_partition(expected_partition)?
        || result.config_sha256 != config_sha256
        || result.aerosol_model_sha256 != rt.aerosol_model_sha256
        || result.mie_artifact_sha256 != rt.mie_artifact_sha256
        || result.photons_per_replicate != rt.photons_per_replicate
        || result.replicates != rt.replicates
        || result.cells.len() != rt.separation_deg.len()
    {
        bail!("partition {expected_partition} has incompatible provenance or shape");
    }
    for (cell, expected_separation) in result.cells.iter().zip(&rt.separation_deg) {
        if cell.separation_deg != *expected_separation
            || !cell.double_to_single.is_finite()
            || cell.double_to_single < 0.0
            || !cell.standard_error.is_finite()
            || cell.standard_error < 0.0
            || !cell.correction.is_finite()
            || !(1.0..=10.0).contains(&cell.correction)
        {
            bail!("partition {expected_partition} contains an invalid cell");
        }
    }
    Ok(())
}

fn verify_partition_checksum(path: &Path, partition: &str) -> Result<()> {
    let manifest_path = path
        .parent()
        .and_then(Path::parent)
        .map(|root| root.join("artifacts.json"))
        .context("partition has no artifact manifest")?;
    let artifacts: Vec<Artifact> = serde_json::from_slice(
        &fs::read(&manifest_path)
            .with_context(|| format!("missing partition manifest {}", manifest_path.display()))?,
    )
    .with_context(|| format!("invalid partition manifest {}", manifest_path.display()))?;
    let name = format!("partitions/{partition}.json");
    let expected = artifacts
        .iter()
        .find(|artifact| artifact.name == name)
        .with_context(|| format!("partition manifest omits {name}"))?;
    let actual = crate::platform::checksum_io::sha256_file(path)?;
    if actual != expected.sha256 {
        bail!("partition {partition} checksum does not match its artifact manifest");
    }
    Ok(())
}

fn is_sha256(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn select_partition_path(local: &Path, worker: &Path, partition: &str) -> Result<PathBuf> {
    if local.is_file() {
        Ok(local.to_path_buf())
    } else if worker.is_file() {
        Ok(worker.to_path_buf())
    } else {
        bail!("required partition {partition} is missing; finalization fails closed")
    }
}

fn serialize_grid(rt: &RtConfig, rows: &[PartitionResult]) -> Result<String> {
    if rows.len() != wavelengths(rt).count() {
        bail!("cannot serialize an incomplete wavelength set");
    }
    let mut text = String::new();
    writeln!(text, "# schema = nsb-moonlight-multiscatter-v1")?;
    writeln!(
        text,
        "# definition = f = I_total/I_SS = 1/(1-min(I_DS/I_SS,0.9))"
    )?;
    writeln!(text, "# solver = {}", rt.solver)?;
    writeln!(text, "# aerosol_model_sha256 = {}", rt.aerosol_model_sha256)?;
    writeln!(text, "# mie_artifact_sha256 = {}", rt.mie_artifact_sha256)?;
    writeln!(text, "# seed_namespace = {}", rt.seed_namespace)?;
    writeln!(text, "{} {}", rows.len(), rt.separation_deg.len())?;
    for (index, row) in rows.iter().enumerate() {
        if index > 0 {
            text.push(' ');
        }
        write!(text, "{:.6}", f64::from(row.wavelength_nm) / 1000.0)?;
    }
    text.push('\n');
    for (index, angle) in rt.separation_deg.iter().enumerate() {
        if index > 0 {
            text.push(' ');
        }
        write!(text, "{angle}")?;
    }
    text.push('\n');
    for row in rows {
        for (index, cell) in row.cells.iter().enumerate() {
            if index > 0 {
                text.push(' ');
            }
            write!(text, "{:.*}", rt.serialization_digits, cell.correction)?;
        }
        text.push('\n');
    }
    Ok(text)
}

fn parse_phase_grid(raw: &str) -> Result<PhaseGrid> {
    let mut lines = raw.lines().filter_map(|line| {
        let line = line.trim();
        (!line.is_empty() && !line.starts_with('#')).then_some(line)
    });
    let dimensions = parse_numbers::<usize>(lines.next().context("missing grid dimensions")?)?;
    if dimensions.len() != 2 || dimensions[0] == 0 || dimensions[1] < 2 {
        bail!("invalid grid dimensions");
    }
    let wavelengths_um = parse_numbers::<f64>(lines.next().context("missing wavelength axis")?)?;
    let angles_deg = parse_numbers::<f64>(lines.next().context("missing angle axis")?)?;
    if wavelengths_um.len() != dimensions[0] || angles_deg.len() != dimensions[1] {
        bail!("grid axis length mismatch");
    }
    let mut rows = Vec::with_capacity(dimensions[0]);
    for _ in 0..dimensions[0] {
        let row = parse_numbers::<f64>(lines.next().context("premature grid EOF")?)?;
        if row.len() != dimensions[1] || row.iter().any(|value| !value.is_finite() || *value < 0.0)
        {
            bail!("invalid grid row");
        }
        rows.push(row);
    }
    if lines.next().is_some() {
        bail!("unexpected data after grid rows");
    }
    Ok(PhaseGrid {
        wavelengths_nm: wavelengths_um
            .into_iter()
            .map(|value| value * 1000.0)
            .collect(),
        angles_rad: angles_deg.into_iter().map(f64::to_radians).collect(),
        rows,
    })
}

fn parse_numbers<T: std::str::FromStr>(line: &str) -> Result<Vec<T>>
where
    T::Err: std::error::Error + Send + Sync + 'static,
{
    line.split_whitespace()
        .map(|value| value.parse::<T>().map_err(anyhow::Error::from))
        .collect()
}

fn interpolate(axis: &[f64], values: &[f64], value: f64) -> f64 {
    if value <= axis[0] {
        return values[0];
    }
    let last = axis.len() - 1;
    if value >= axis[last] {
        return values[last];
    }
    let upper = axis.partition_point(|entry| *entry <= value);
    let lower = upper - 1;
    let fraction = (value - axis[lower]) / (axis[upper] - axis[lower]);
    values[lower] + fraction * (values[upper] - values[lower])
}

fn artifact(name: &str, path: &Path) -> Result<Artifact> {
    Ok(Artifact {
        name: name.into(),
        path: PathBuf::from(path),
        sha256: crate::platform::checksum_io::sha256_file(path)?,
        bytes: fs::metadata(path)?.len(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn jones_geometric_series_is_multiplicative_and_capped() {
        assert!((1.0 / (1.0 - 0.2_f64.min(0.9)) - 1.25).abs() < 1.0e-15);
        assert!((1.0 / (1.0 - 1.2_f64.min(0.9)) - 10.0).abs() < 1.0e-12);
    }

    #[test]
    fn stable_seed_is_reproducible_and_partition_specific() {
        assert_eq!(stable_seed("v1:w0300:0"), stable_seed("v1:w0300:0"));
        assert_ne!(stable_seed("v1:w0300:0"), stable_seed("v1:w0310:0"));
    }

    #[test]
    fn symmetric_geometry_has_requested_separation() {
        let (source, observer) = symmetric_geometry(70.0, 120.0).unwrap();
        assert!((dot(source, observer).acos().to_degrees() - 120.0).abs() < 1.0e-10);
    }

    #[test]
    fn one_eighty_degree_boundary_reuses_one_forty_degree_result() {
        let mut rt: RtConfig = toml::from_str(include_str!(
            "../../config/moonlight-multiscatter-nsb-v1.toml"
        ))
        .unwrap();
        rt.photons_per_replicate = 1024;
        rt.replicates = 2;
        let grid =
            parse_phase_grid(include_str!("../../../nsb/data/moonlight_mie_nsb_v1.dat")).unwrap();
        for (index, wavelength_nm) in (300..=650).step_by(10).enumerate() {
            let sampler = PhaseSampler::new(&grid.angles_rad, &grid.rows[index]).unwrap();
            let at_140 = simulate_cell(&rt, &sampler, wavelength_nm, 140).unwrap();
            let at_180 = simulate_cell(&rt, &sampler, wavelength_nm, 180).unwrap();
            assert_eq!(at_180.double_to_single, at_140.double_to_single);
            assert_eq!(at_180.standard_error, at_140.standard_error);
            assert_eq!(at_180.correction, at_140.correction);
        }
    }

    #[test]
    fn serialization_is_byte_deterministic() {
        let rt: RtConfig = toml::from_str(include_str!(
            "../../config/moonlight-multiscatter-nsb-v1.toml"
        ))
        .unwrap();
        let rows = wavelengths(&rt)
            .map(|wavelength_nm| PartitionResult {
                schema: PARTITION_SCHEMA.into(),
                partition: partition_name(wavelength_nm),
                wavelength_nm,
                config_sha256: "a".repeat(64),
                aerosol_model_sha256: rt.aerosol_model_sha256.clone(),
                mie_artifact_sha256: rt.mie_artifact_sha256.clone(),
                photons_per_replicate: rt.photons_per_replicate,
                replicates: rt.replicates,
                cells: rt
                    .separation_deg
                    .iter()
                    .map(|&separation_deg| CellResult {
                        separation_deg,
                        double_to_single: 0.1,
                        standard_error: 0.001,
                        correction: 1.111_111_111,
                    })
                    .collect(),
            })
            .collect::<Vec<_>>();
        assert_eq!(
            serialize_grid(&rt, &rows).unwrap(),
            serialize_grid(&rt, &rows).unwrap()
        );
    }

    #[test]
    fn missing_partition_fails_closed() {
        let temp = tempfile::tempdir().unwrap();
        let error = select_partition_path(
            &temp.path().join("local.json"),
            &temp.path().join("worker.json"),
            "w0300",
        )
        .unwrap_err();
        assert!(error.to_string().contains("finalization fails closed"));
    }

    #[test]
    fn admitted_production_manifest_is_complete_and_compatible() {
        let manifest: toml::Value = toml::from_str(include_str!(
            "../../../../docs/nsb_components/moonlight/production-runs/multiscatter-nsb-v1.toml"
        ))
        .unwrap();
        let rt: RtConfig = toml::from_str(include_str!(
            "../../config/moonlight-multiscatter-nsb-v1.toml"
        ))
        .unwrap();
        let run_config_sha256 = crate::platform::checksum_io::sha256_bytes(include_bytes!(
            "../../config/moonlight-scattering.toml"
        ));
        let rt_config_sha256 = crate::platform::checksum_io::sha256_bytes(include_bytes!(
            "../../config/moonlight-multiscatter-nsb-v1.toml"
        ));
        let output_sha256 = crate::platform::checksum_io::sha256_bytes(include_bytes!(
            "../../../nsb/data/moonlight_multiscatter_nsb_v1.dat"
        ));
        assert_eq!(
            manifest["run_config_sha256"].as_str(),
            Some(run_config_sha256.as_str())
        );
        assert_eq!(
            manifest["rt_config_sha256"].as_str(),
            Some(rt_config_sha256.as_str())
        );
        assert_eq!(
            manifest["output_sha256"].as_str(),
            Some(output_sha256.as_str())
        );

        let committed_grid = parse_phase_grid(include_str!(
            "../../../nsb/data/moonlight_multiscatter_nsb_v1.dat"
        ))
        .unwrap();
        let at_140 = committed_grid
            .angles_rad
            .iter()
            .position(|angle| (*angle - 140.0_f64.to_radians()).abs() < 1.0e-12)
            .unwrap();
        let at_180 = committed_grid.angles_rad.len() - 1;
        assert!(committed_grid
            .rows
            .iter()
            .all(|row| row[at_180] == row[at_140]));
        assert_eq!(
            manifest["schema"].as_str(),
            Some("nsb-moonlight-multiscatter-production-run-v1")
        );
        assert_eq!(manifest["model_schema"].as_str(), Some(rt.schema.as_str()));
        assert_eq!(manifest["solver"].as_str(), Some(rt.solver.as_str()));
        assert_eq!(
            manifest["aerosol_model_sha256"].as_str(),
            Some(rt.aerosol_model_sha256.as_str())
        );
        assert_eq!(
            manifest["mie_artifact_sha256"].as_str(),
            Some(rt.mie_artifact_sha256.as_str())
        );
        assert_eq!(
            manifest["photons_per_replicate"].as_integer(),
            Some(rt.photons_per_replicate as i64)
        );
        assert_eq!(
            manifest["replicates"].as_integer(),
            Some(rt.replicates as i64)
        );
        let partition_hashes = manifest["partition_sha256"].as_table().unwrap();
        assert_eq!(partition_hashes.len(), wavelengths(&rt).count());
        for wavelength_nm in wavelengths(&rt) {
            let hash = partition_hashes[&partition_name(wavelength_nm)]
                .as_str()
                .unwrap();
            assert_eq!(hash.len(), 64);
            assert!(hash.bytes().all(|byte| byte.is_ascii_hexdigit()));
        }
        for key in [
            "production_run_id",
            "run_config_sha256",
            "rt_config_sha256",
            "output_sha256",
        ] {
            let hash = manifest[key].as_str().unwrap();
            assert_eq!(hash.len(), 64, "invalid {key}");
            assert!(hash.bytes().all(|byte| byte.is_ascii_hexdigit()));
        }
    }

    #[test]
    #[ignore = "production convergence evidence; run explicitly when changing sampling"]
    fn production_photon_count_convergence() {
        let production: RtConfig = toml::from_str(include_str!(
            "../../config/moonlight-multiscatter-nsb-v1.toml"
        ))
        .unwrap();
        let grid =
            parse_phase_grid(include_str!("../../../nsb/data/moonlight_mie_nsb_v1.dat")).unwrap();
        let mut coarse = production.clone();
        coarse.photons_per_replicate = 8192;
        let mut worst_relative = 0.0_f64;
        for wavelength_nm in [300, 500, 650] {
            let row = grid
                .wavelengths_nm
                .iter()
                .position(|value| (*value - f64::from(wavelength_nm)).abs() < 1.0e-9)
                .unwrap();
            let sampler = PhaseSampler::new(&grid.angles_rad, &grid.rows[row]).unwrap();
            for separation_deg in [0, 90, 140] {
                let low = simulate_cell(&coarse, &sampler, wavelength_nm, separation_deg).unwrap();
                let high =
                    simulate_cell(&production, &sampler, wavelength_nm, separation_deg).unwrap();
                let relative = (low.correction / high.correction - 1.0).abs();
                worst_relative = worst_relative.max(relative);
                eprintln!(
                    "convergence wavelength_nm={wavelength_nm} separation_deg={separation_deg} n8192={:.8} n32768={:.8} relative={relative:.6}",
                    low.correction, high.correction
                );
            }
        }
        assert!(
            worst_relative < 0.03,
            "worst relative delta={worst_relative}"
        );
    }
}
