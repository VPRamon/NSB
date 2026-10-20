// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Vallés Puig, Ramon
//! Positive partition-of-unity factorization of boundary kernel quadrature.
//! The source remains its native pixel mean. We interpolate the smooth transport
//! kernel, not the sky: sum_b I_b w_b K(b) ~= sum_a [sum_b I_b w_b h_a(b)] K(a).
//! Fourier convolution evaluates this factorization without input/output pairs.
use super::*;
use crate::transport::multiple::angular::{transform, Complex};

pub(super) struct ProjectedBoundary {
    pub mu: usize,
    pub phi: usize,
    pub mass: Vec<f64>,
    pub kernels: Option<std::sync::Arc<PhaseKernels>>,
}
impl ProjectedBoundary {
    pub fn new(boundary: &impl IncidentBoundary, mu: usize, phi: usize) -> Result<Self> {
        if mu == 0 || mu > 512 || !phi.is_power_of_two() || !(16..=1024).contains(&phi) {
            return Err(invalid("unsupported boundary kernel projection grid"));
        }
        let length = product(&[mu + 1, phi])?;
        let mut mass = vec![0.0; length];
        boundary.visit(&mut |d| {
            if !d.mu.is_finite() || d.mu <= 0.0 || d.mu > 1.0 + 1e-12 {
                return Err(invalid("invalid projected incident cosine"));
            }
            let cosine = d.mu.min(1.0);
            let i = (((2.0 / std::f64::consts::PI) * cosine.sqrt().asin() * mu as f64).floor()
                as usize)
                .min(mu - 1);
            let lo = (std::f64::consts::PI * 0.5 * i as f64 / mu as f64)
                .sin()
                .powi(2);
            let hi = (std::f64::consts::PI * 0.5 * (i + 1) as f64 / mu as f64)
                .sin()
                .powi(2);
            let t = ((cosine - lo) / (hi - lo)).clamp(0.0, 1.0);
            let az = d.vector[1].atan2(d.vector[0]).rem_euclid(TAU) * phi as f64 / TAU;
            let j = az.floor() as usize % phi;
            let f = az - az.floor();
            let incoming = nonnegative(d.incoming * d.weight, "projected incident radiance mass")?;
            for (row, weight) in [(i, 1.0 - t), (i + 1, t)] {
                let left = row * phi + j;
                let right = row * phi + (j + 1) % phi;
                mass[left] = nonnegative(
                    mass[left] + incoming * weight * (1.0 - f),
                    "projected incident mass",
                )?;
                mass[right] = nonnegative(
                    mass[right] + incoming * weight * f,
                    "projected incident mass",
                )?;
            }
            Ok(())
        })?;
        Ok(Self {
            mu,
            phi,
            mass,
            kernels: None,
        })
    }
    fn cosine(&self, i: usize) -> f64 {
        (std::f64::consts::PI * 0.5 * i as f64 / self.mu as f64)
            .sin()
            .powi(2)
            .max(f64::MIN_POSITIVE)
    }
}
impl IncidentBoundary for ProjectedBoundary {
    fn visit(&self, visitor: &mut dyn FnMut(Direction) -> Result<()>) -> Result<()> {
        for i in 0..=self.mu {
            let mu = self.cosine(i);
            let r = (1.0 - mu * mu).sqrt();
            for p in 0..self.phi {
                let az = TAU * p as f64 / self.phi as f64;
                visitor(Direction {
                    mu,
                    vector: [r * az.cos(), r * az.sin(), mu],
                    weight: 1.0,
                    incoming: self.mass[i * self.phi + p],
                })?;
            }
        }
        Ok(())
    }
    fn project(&self, ctx: BoundaryProjection<'_>) -> Result<Option<(f64, f64, f64)>> {
        let BoundaryProjection {
            cells,
            dirs,
            w,
            phase,
            view,
            sources,
            views,
        } = ctx;
        if self
            .kernels
            .as_ref()
            .is_some_and(|k| k.mu != self.mu || k.phi != self.phi)
        {
            return Err(invalid("phase cache projection grid identity mismatch"));
        }
        let n = dirs.len();
        let m = cells.len();
        let p = self.phi;
        // Incoming azimuthal flux moments are source/geometry dependent only.
        // They are transformed once here; no spectral optical term is cached.
        let mut moments: Vec<_> = self
            .mass
            .iter()
            .map(|&re| Complex { re, im: 0.0 })
            .collect();
        for row in moments.chunks_exact_mut(p) {
            transform(row, false);
        }
        let mut attenuation = vec![0.0; (self.mu + 1) * m];
        let mut flux = (0.0, 0.0, 0.0);
        for i in 0..=self.mu {
            let mu = self.cosine(i);
            let incoming = moments[i * p].re;
            let d = Direction {
                mu,
                vector: [0.0; 3],
                weight: 1.0,
                incoming,
            };
            let f = boundary_flux(cells, &[d])?;
            flux.0 += f.0;
            flux.1 += f.1;
            flux.2 += f.2;
            let mut above = 0.0;
            for j in (0..m).rev() {
                attenuation[i * m + j] = (-(above + 0.5 * cells[j].depth) / mu).exp();
                above += cells[j].depth;
            }
        }
        let internal_p = dirs.iter().position(|d| d.mu != dirs[0].mu).unwrap_or(n);
        if !p.is_multiple_of(internal_p) {
            return Err(invalid(
                "projection azimuth grid must contain diffuse azimuth nodes",
            ));
        }
        let mut ray = vec![Complex::default(); p];
        let mut aer = ray.clone();
        let mut output = vec![Complex::default(); m * p];
        let mut targets: Vec<_> = dirs.iter().step_by(internal_p).map(|d| d.vector).collect();
        let view_az = view[1].atan2(view[0]);
        targets.push([(1.0 - view[2] * view[2]).sqrt(), 0.0, view[2]]);
        for (q, target) in targets.iter().enumerate() {
            output.fill(Complex::default());
            for i in 0..=self.mu {
                if let Some(kernels) = &self.kernels {
                    kernels.row(target[2], i, &mut ray, &mut aer)?;
                } else {
                    let mu = self.cosine(i);
                    let r = (1.0 - mu * mu).sqrt();
                    for k in 0..p {
                        let az = TAU * k as f64 / p as f64;
                        let pair = phase_pair(w, phase, *target, [r * az.cos(), r * az.sin(), mu])?;
                        ray[k] = Complex {
                            re: pair[0],
                            im: 0.0,
                        };
                        aer[k] = Complex {
                            re: pair[1],
                            im: 0.0,
                        };
                    }
                    transform(&mut ray, false);
                    transform(&mut aer, false);
                }
                for j in 0..m {
                    let factor = attenuation[i * m + j];
                    for k in 0..p {
                        let kernel =
                            (cells[j].rayleigh * ray[k].re + cells[j].aerosol * aer[k].re) * factor;
                        output[j * p + k].re += kernel * moments[i * p + k].re;
                        output[j * p + k].im += kernel * moments[i * p + k].im;
                    }
                }
            }
            for j in 0..m {
                let row = &mut output[j * p..(j + 1) * p];
                if q == targets.len() - 1 {
                    // Evaluate the real trigonometric interpolant at the actual
                    // requested azimuth, retaining every sampled Fourier mode.
                    let mut value = row[0].re;
                    for (k, coefficient) in row.iter().enumerate().take(p / 2).skip(1) {
                        let angle = k as f64 * view_az;
                        value +=
                            2.0 * (coefficient.re * angle.cos() - coefficient.im * angle.sin());
                    }
                    value += row[p / 2].re * (p as f64 * 0.5 * view_az).cos();
                    views[j] = nonnegative(value / p as f64, "projected view source")?;
                } else {
                    transform(row, true);
                    for k in 0..internal_p {
                        sources[j * n + q * internal_p + k] =
                            nonnegative(row[k * (p / internal_p)].re, "projected diffuse source")?;
                    }
                }
            }
        }
        Ok(Some(flux))
    }
}

// Evaluation-local phase kernels. Only analytic Optica HG/double-HG use this
// cache: their phase density is explicitly wavelength independent. Native sky,
// epoch, optical coefficients and attenuation are never cached here.
pub(super) struct PhaseKernels {
    mu: usize,
    phi: usize,
    targets: Vec<f64>,
    ray: Vec<[f64; 3]>,
    aer: Vec<f64>,
}
impl PhaseKernels {
    pub fn new(
        sky: &StarlightSky<'_>,
        phase: &AerosolPhase,
        view: Horizontal,
        settings: MultipleScatteringSettings,
        grid: [usize; 2],
        all_angles: bool,
    ) -> Result<Self> {
        let [mu, phi] = grid;
        let mut targets = Vec::new();
        for a in if all_angles { vec![1, 2] } else { vec![2] } {
            let dirs = ordinates(
                sky,
                sky.band[0],
                settings.mu_nodes * a,
                settings.azimuth_nodes * a,
                false,
            )?;
            for d in dirs.iter().step_by(settings.azimuth_nodes * a) {
                if !targets.contains(&d.mu) {
                    targets.push(d.mu);
                }
            }
        }
        let view_mu = view.alt().to::<Radian>().sin();
        if !targets.contains(&view_mu) {
            targets.push(view_mu);
        }
        let mut ray = vec![[0.; 3]; targets.len() * (mu + 1)];
        let mut aer = vec![0.; targets.len() * (mu + 1) * (phi / 2 + 1)];
        let mut r = vec![Complex::default(); phi];
        let mut a = r.clone();
        for (q, &out) in targets.iter().enumerate() {
            let v = [(1. - out * out).sqrt(), 0., out];
            for i in 0..=mu {
                let inc = (std::f64::consts::PI * 0.5 * i as f64 / mu as f64)
                    .sin()
                    .powi(2)
                    .max(f64::MIN_POSITIVE);
                let transverse = (1. - inc * inc).sqrt();
                for k in 0..phi {
                    let az = TAU * k as f64 / phi as f64;
                    let pair = phase_pair(
                        sky.band[0],
                        phase,
                        v,
                        [transverse * az.cos(), transverse * az.sin(), inc],
                    )?;
                    r[k] = Complex {
                        re: pair[0],
                        im: 0.,
                    };
                    a[k] = Complex {
                        re: pair[1],
                        im: 0.,
                    };
                }
                transform(&mut r, false);
                transform(&mut a, false);
                for k in 0..3 {
                    ray[q * (mu + 1) + i][k] = r[k].re;
                }
                for k in 0..=phi / 2 {
                    aer[(q * (mu + 1) + i) * (phi / 2 + 1) + k] = a[k].re;
                }
            }
        }
        Ok(Self {
            mu,
            phi,
            targets,
            ray,
            aer,
        })
    }
    fn row(
        &self,
        target: f64,
        input: usize,
        ray: &mut [Complex],
        aer: &mut [Complex],
    ) -> Result<()> {
        let q = self
            .targets
            .iter()
            .position(|&x| x == target)
            .ok_or_else(|| invalid("phase cache angular identity mismatch"))?;
        for k in 0..self.phi {
            let mode = k.min(self.phi - k);
            ray[k] = Complex {
                re: if mode < 3 {
                    self.ray[q * (self.mu + 1) + input][mode]
                } else {
                    0.
                },
                im: 0.,
            };
            aer[k] = Complex {
                re: self.aer[(q * (self.mu + 1) + input) * (self.phi / 2 + 1) + mode],
                im: 0.,
            };
        }
        Ok(())
    }
}
pub(super) fn phase_bytes(u: usize, p: usize, q: usize) -> Result<usize> {
    product(&[u + 1, q, p / 2 + 1 + 3, std::mem::size_of::<f64>()])
}
pub(super) fn phase_work(u: usize, p: usize, q: usize) -> Result<usize> {
    product(&[u + 1, q, 2 * p + 2 * angular::transform_work(p)?])
}
