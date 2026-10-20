// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Vallés Puig, Ramon
//! Exact azimuth-circulant factorization of the existing sampled phase operator.
//! All Fourier modes are retained. No Legendre truncation or renormalization.
use super::*;

#[derive(Clone, Copy, Default, Debug)]
pub(super) struct Complex {
    pub re: f64,
    pub im: f64,
}
impl Complex {
    fn times(self, rhs: Self) -> Self {
        Self {
            re: self.re * rhs.re - self.im * rhs.im,
            im: self.re * rhs.im + self.im * rhs.re,
        }
    }
}

// Radix two for large grids; the small direct DFT preserves support for the
// previously allowed non-power-of-two azimuth counts. Both have explicit bounds.
pub(super) fn transform(values: &mut [Complex], inverse: bool) {
    let n = values.len();
    let sign = if inverse { 1.0 } else { -1.0 };
    if n.is_power_of_two() {
        let mut j = 0;
        for i in 1..n {
            let mut bit = n >> 1;
            while j & bit != 0 {
                j ^= bit;
                bit >>= 1;
            }
            j ^= bit;
            if i < j {
                values.swap(i, j);
            }
        }
        let mut length = 2;
        while length <= n {
            let angle = sign * TAU / length as f64;
            let root = Complex {
                re: angle.cos(),
                im: angle.sin(),
            };
            for chunk in values.chunks_exact_mut(length) {
                let mut factor = Complex { re: 1.0, im: 0.0 };
                for k in 0..length / 2 {
                    let a = chunk[k];
                    let b = chunk[k + length / 2].times(factor);
                    chunk[k] = Complex {
                        re: a.re + b.re,
                        im: a.im + b.im,
                    };
                    chunk[k + length / 2] = Complex {
                        re: a.re - b.re,
                        im: a.im - b.im,
                    };
                    factor = factor.times(root);
                }
            }
            length *= 2;
        }
    } else {
        let input = values.to_vec();
        for (k, result) in values.iter_mut().enumerate() {
            *result = Complex::default();
            for (j, v) in input.iter().enumerate() {
                let angle = sign * TAU * (k * j) as f64 / n as f64;
                let t = v.times(Complex {
                    re: angle.cos(),
                    im: angle.sin(),
                });
                result.re += t.re;
                result.im += t.im;
            }
        }
    }
    if inverse {
        for v in values {
            v.re /= n as f64;
            v.im /= n as f64;
        }
    }
}

pub(super) enum Operator {
    Dense {
        ray: Vec<f64>,
        aer: Vec<f64>,
        n: usize,
    },
    Fourier {
        ray: Vec<f64>,
        aer: Vec<f64>,
        q: usize,
        p: usize,
    },
}
impl Operator {
    pub fn new(
        dirs: &[Direction],
        p: usize,
        w: Nanometers,
        phase: &AerosolPhase,
    ) -> Result<(Self, [f64; 2], [f64; 2])> {
        let n = dirs.len();
        let q = n / p;
        let dense = n <= 256;
        let length = if dense { n * n } else { q * q * p };
        let mut ray = vec![0.0; length];
        let mut aer = vec![0.0; length];
        let mut min = [f64::INFINITY; 2];
        let mut max = [0.0_f64; 2];
        let mut buffer = vec![Complex::default(); p];
        for i in 0..if dense { n } else { q } {
            let mut sum = [0.0; 2];
            if dense {
                for j in 0..n {
                    let pair = phase_pair(w, phase, dirs[i].vector, dirs[j].vector)?;
                    ray[i * n + j] = pair[0] * dirs[j].weight;
                    aer[i * n + j] = pair[1] * dirs[j].weight;
                    sum[0] += ray[i * n + j];
                    sum[1] += aer[i * n + j];
                }
            } else {
                for j in 0..q {
                    for (k, v) in buffer.iter_mut().enumerate() {
                        let pair =
                            phase_pair(w, phase, dirs[i * p].vector, dirs[j * p + k].vector)?;
                        *v = Complex {
                            re: pair[0] * dirs[j * p + k].weight,
                            im: pair[1] * dirs[j * p + k].weight,
                        };
                        sum[0] += v.re;
                        sum[1] += v.im;
                    }
                    // Transform the two real even phase kernels separately.
                    let mut other: Vec<_> = buffer
                        .iter()
                        .map(|v| Complex { re: v.im, im: 0.0 })
                        .collect();
                    for v in &mut buffer {
                        v.im = 0.0;
                    }
                    transform(&mut buffer, false);
                    transform(&mut other, false);
                    for k in 0..p {
                        ray[(i * q + j) * p + k] = buffer[k].re;
                        aer[(i * q + j) * p + k] = other[k].re;
                    }
                }
            }
            for c in 0..2 {
                min[c] = min[c].min(sum[c]);
                max[c] = max[c].max(sum[c]);
            }
        }
        Ok((
            if dense {
                Self::Dense { ray, aer, n }
            } else {
                Self::Fourier { ray, aer, q, p }
            },
            min,
            max,
        ))
    }

    pub fn apply(&self, input: &[f64], output: &mut [f64], cell: Cell) -> Result<()> {
        match self {
            Self::Dense { ray, aer, n } => {
                for (i, value) in output.iter_mut().enumerate() {
                    let mut r = 0.0;
                    let mut a = 0.0;
                    for k in 0..*n {
                        r += ray[i * n + k] * input[k];
                        a += aer[i * n + k] * input[k];
                    }
                    *value = nonnegative(cell.rayleigh * r + cell.aerosol * a, "diffuse source")?;
                }
            }
            Self::Fourier { ray, aer, q, p } => {
                let mut field: Vec<_> = input.iter().map(|&re| Complex { re, im: 0.0 }).collect();
                for row in field.chunks_exact_mut(*p) {
                    transform(row, false);
                }
                let mut source = vec![Complex::default(); *p];
                let scale =
                    input.iter().copied().fold(0.0_f64, f64::max) * (cell.rayleigh + cell.aerosol);
                for i in 0..*q {
                    source.fill(Complex::default());
                    for j in 0..*q {
                        for k in 0..*p {
                            let factor = cell.rayleigh * ray[(i * q + j) * p + k]
                                + cell.aerosol * aer[(i * q + j) * p + k];
                            source[k].re += factor * field[j * p + k].re;
                            source[k].im += factor * field[j * p + k].im;
                        }
                    }
                    transform(&mut source, true);
                    for k in 0..*p {
                        // A positive sampled convolution can have a negative
                        // result only at transform roundoff. Do not mask a
                        // resolved negative source or a nonfinite result.
                        let value = source[k].re;
                        if value < -1e-12 * scale || !value.is_finite() {
                            return Err(invalid("Fourier source violates positivity"));
                        }
                        output[i * p + k] = value.max(0.0);
                    }
                }
            }
        }
        Ok(())
    }
}

// Same constant-source formal sweeps as the authoritative recurrence, without
// observational flux bookkeeping. Used only to construct a positive weighted
// norm when the old unweighted grazing-ray bound cannot establish contraction.
fn propagate(cells: &[Cell], dirs: &[Direction], source: &[f64], field: &mut [f64]) {
    let n = dirs.len();
    for (i, d) in dirs.iter().enumerate() {
        let mut incoming = 0.0;
        for k in 0..cells.len() {
            let j = if d.mu > 0.0 { cells.len() - 1 - k } else { k };
            let cell = cells[j];
            let distance = if cell.extinction > 0.0 {
                cell.depth / cell.extinction / d.mu.abs()
            } else {
                0.0
            };
            field[j * n + i] = step(incoming, source[j * n + i], cell, distance * 0.5);
            incoming = step(incoming, source[j * n + i], cell, distance);
        }
    }
}

pub(super) fn weighted_norm(
    operator: &Operator,
    cells: &[Cell],
    dirs: &[Direction],
) -> Result<(Vec<f64>, f64)> {
    let n = dirs.len();
    let length = cells.len() * n;
    let mut weight = vec![1.0; length];
    let mut power = vec![1.0; length];
    let mut source = vec![0.0; length];
    let mut next = vec![0.0; length];
    let mut best: Option<(Vec<f64>, f64)> = None;
    for _ in 0..8 {
        for (j, cell) in cells.iter().enumerate() {
            operator.apply(
                &power[j * n..(j + 1) * n],
                &mut source[j * n..(j + 1) * n],
                *cell,
            )?;
        }
        propagate(cells, dirs, &source, &mut next);
        // For w=sum_{r=0}^{p-1} K^r 1, K w = w-1+K^p 1.
        // A componentwise K w <= q w gives a genuine weighted sup-norm
        // contraction even if the unweighted one-step norm is near unity.
        let q = weight
            .iter()
            .zip(&next)
            .map(|(w, k)| (w - 1.0 + k) / w)
            .fold(0.0_f64, f64::max)
            + 1e-12;
        if q < 1.0 && best.as_ref().is_none_or(|(_, old)| q < *old) {
            best = Some((weight.clone(), q));
        }
        if q < 0.9 {
            return Ok((weight, q));
        }
        for (w, x) in weight.iter_mut().zip(&next) {
            *w += x;
        }
        std::mem::swap(&mut power, &mut next);
    }
    best.ok_or_else(|| invalid("discrete scattering contraction is not established"))
}

pub(super) fn workspace(directions: usize, cells: usize, azimuths: usize) -> Result<usize> {
    if directions <= 256 {
        return workspace_bytes(directions, cells);
    }
    let old = workspace_bytes(directions, cells)?;
    let dense = product(&[2, directions, directions, std::mem::size_of::<f64>()])?;
    let compressed = product(&[
        2,
        directions / azimuths,
        directions,
        std::mem::size_of::<f64>(),
    ])?;
    let fft = product(&[4, directions + azimuths, std::mem::size_of::<Complex>()])?;
    old.checked_sub(dense)
        .and_then(|x| x.checked_add(compressed))
        .and_then(|x| x.checked_add(fft))
        .ok_or_else(|| invalid("Fourier workspace overflow"))
}

pub(super) fn transform_work(p: usize) -> Result<usize> {
    if p.is_power_of_two() {
        product(&[5, p, p.ilog2() as usize])
    } else {
        product(&[12, p, p])
    }
}
pub(super) fn plan_work(directions: usize, cells: usize, p: usize, orders: usize) -> Result<usize> {
    if directions <= 256 {
        return product(&[
            2 * directions * directions + 8 * directions,
            cells,
            orders + 1,
        ]);
    }
    let q = directions / p;
    let transforms = product(&[2, q, transform_work(p)?])?;
    let multiply = product(&[8, q, q, p])?;
    let sweeps = product(&[8, directions])?;
    let phase = product(&[q, q, 2 * p + 2 * transform_work(p)?])?;
    product(&[transforms + multiply + sweeps, cells, orders + 9])?
        .checked_add(phase)
        .ok_or_else(|| invalid("Fourier work overflow"))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn transform_round_trip_and_known_mode() {
        for n in [4, 7, 32, 64] {
            let original: Vec<_> = (0..n)
                .map(|j| Complex {
                    re: (3.0 * TAU * j as f64 / n as f64).cos() + 0.3,
                    im: (2.0 * TAU * j as f64 / n as f64).sin(),
                })
                .collect();
            let mut x = original.clone();
            transform(&mut x, false);
            transform(&mut x, true);
            for (a, b) in x.iter().zip(original) {
                assert!((a.re - b.re).abs() < 2e-14);
                assert!((a.im - b.im).abs() < 2e-14);
            }
        }
    }
    #[test]
    fn fourier_phase_matches_direct_positive_convolution() {
        let map = crate::components::starlight::StarlightMap::from_json_bytes(
            include_bytes!("../../../tests/data/starlight-transport/uniform.json"),
            "35058caa944e51096e5d9ff8ca33eb19be08ef0b6c727ec37192758a64bdc24f",
        )
        .unwrap();
        let sky = crate::transport::single::starlight::StarlightSky::new(
            &map,
            crate::transport::single::starlight::StarlightGeometry {
                observer: crate::Observer::new(
                    Degrees::new(0.),
                    Degrees::new(30.),
                    crate::units::qtty::Meters::new(0.),
                ),
                tt: tempoch::JulianDate::new(2459680.8),
                ut1: tempoch::JulianDate::new(2459680.7992),
            },
            [Nanometers::new(360.), Nanometers::new(390.)],
            crate::transport::single::starlight::WithinBinSpectrum::FlatPhotonDensity,
        )
        .unwrap();
        for g in [0., 0.4, 0.65] {
            let phase = AerosolPhase::HenyeyGreenstein(
                siderust::optica::phase::HenyeyGreensteinPhaseFunction::try_new(g).unwrap(),
            );
            let dirs = ordinates(&sky, Nanometers::new(375.), 8, 32, false).unwrap();
            let (op, _, _) = Operator::new(&dirs, 32, Nanometers::new(375.), &phase).unwrap();
            let input: Vec<_> = (0..dirs.len())
                .map(|k| {
                    if k % 17 == 0 {
                        0.1
                    } else {
                        (k as f64).sin().abs() * 0.01
                    }
                })
                .collect();
            let cell = Cell {
                depth: 0.1,
                extinction: 0.1,
                rayleigh: 0.03,
                aerosol: 0.04,
            };
            let mut output = vec![0.; dirs.len()];
            op.apply(&input, &mut output, cell).unwrap();
            for (i, out) in dirs.iter().enumerate() {
                let expected: f64 = dirs
                    .iter()
                    .zip(&input)
                    .map(|(d, x)| {
                        let pair = phase_pair(Nanometers::new(375.), &phase, out.vector, d.vector)
                            .unwrap();
                        (cell.rayleigh * pair[0] + cell.aerosol * pair[1]) * x * d.weight
                    })
                    .sum();
                assert!((output[i] - expected).abs() < 2e-15);
                assert!(output[i] >= 0.);
            }
        }
    }
}

#[cfg(test)]
mod analytic_tests {
    use super::*;
    #[test]
    fn known_legendre_illumination_has_analytic_phase_eigenvalues() {
        struct Source;
        impl IncidentSky for Source {
            fn coverage(&self) -> SkyCoverage {
                SkyCoverage {
                    identity: "analytic phase eigenfunction".into(),
                    wavelengths: [Nanometers::new(360.), Nanometers::new(570.)],
                    full_hemisphere: true,
                    origin: crate::transport::RadianceOrigin::TopOfAtmosphere,
                }
            }
            fn radiance(&self, _: Nanometers, _: Horizontal) -> Result<Radiance> {
                Ok(Radiance::new(1.))
            }
        }
        let w = Nanometers::new(375.);
        let dirs = ordinates(&Source, w, 16, 64, false).unwrap();
        for (g, rayleigh) in [(0., true), (0., false), (0.4, false), (0.65, false)] {
            let phase = AerosolPhase::HenyeyGreenstein(
                siderust::optica::phase::HenyeyGreensteinPhaseFunction::try_new(g).unwrap(),
            );
            let (op, _, _) = Operator::new(&dirs, 64, w, &phase).unwrap();
            let polynomial = |mu: f64| {
                if rayleigh {
                    0.5 * (3. * mu * mu - 1.)
                } else {
                    mu
                }
            };
            let input: Vec<_> = dirs.iter().map(|d| 1. + 0.2 * polynomial(d.mu)).collect();
            let mut output = vec![0.; dirs.len()];
            op.apply(
                &input,
                &mut output,
                Cell {
                    depth: 1.,
                    extinction: 1.,
                    rayleigh: f64::from(rayleigh),
                    aerosol: f64::from(!rayleigh),
                },
            )
            .unwrap();
            let eigenvalue = if rayleigh { 0.1 } else { g };
            for (d, result) in dirs.iter().zip(output) {
                assert!((result - (1. + 0.2 * eigenvalue * polynomial(d.mu))).abs() < 1e-6);
            }
        }
    }
}
