// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Vallés Puig, Ramon
//! Solver-local positive quadrature; not a general numerical integration API.
use super::*;
use crate::units::qtty::{Per, Quantity, Ratio, Steradian, Steradians};
use std::f64::consts::TAU;

/// GL nodes/weights on (0,1), ascending, without a positive altitude cutoff.
/// Newton iteration is bounded; failed root construction is an explicit error.
pub(super) fn gauss(n: usize) -> Result<Vec<(f64, f64)>> {
    let mut nodes = vec![(0.0, 0.0); n];
    for i in 0..n.div_ceil(2) {
        let mut x = (PI * (i as f64 + 0.75) / (n as f64 + 0.5)).cos();
        let mut root = None;
        for _ in 0..64 {
            let (mut p, mut previous) = (1.0, 0.0);
            for k in 1..=n {
                let next = ((2 * k - 1) as f64 * x * p - (k - 1) as f64 * previous) / k as f64;
                previous = p;
                p = next;
            }
            let derivative = n as f64 * (x * p - previous) / (x * x - 1.0);
            let delta = p / derivative;
            if delta.abs() < 4.0 * f64::EPSILON {
                root = Some((x, 1.0 / ((1.0 - x * x) * derivative * derivative)));
                break;
            }
            x -= delta;
        }
        let (x, weight) = root.ok_or_else(|| invalid("Gauss–Legendre root did not converge"))?;
        if !weight.is_finite() || weight <= 0.0 || !x.is_finite() || x.abs() >= 1.0 {
            return Err(invalid("invalid angular/path quadrature weight or node"));
        }
        nodes[i] = ((1.0 - x) / 2.0, weight);
        nodes[n - 1 - i] = ((1.0 + x) / 2.0, weight);
    }
    Ok(nodes)
}

/// Explicit conversion seam: upstream normalized dimensionless phase marker is
/// physically sr^-1. Multiplication by typed solid angle returns a pure weight.
fn phase_solid_angle(density: f64, angle: Steradians) -> f64 {
    (Quantity::<Per<Ratio, Steradian>>::new(density) * angle).value()
}

struct Sample {
    mu: f64,
    rayleigh: f64,
    aerosol: f64,
}

pub(super) const SAMPLE_BYTES: usize = std::mem::size_of::<Sample>();

pub(super) struct Kernel<'a, S> {
    pub atmosphere: &'a LayeredAtmosphere,
    pub phase: &'a AerosolPhase,
    pub sky: &'a S,
    pub wavelength: Nanometers,
    pub view: Horizontal,
}
impl<S: IncidentSky> Kernel<'_, S> {
    pub fn integrate(&self, orders: [usize; 3]) -> Result<f64> {
        let [nmu, nphi, nz] = orders;
        let (out_mu, out_az) = (
            self.view.alt().to::<Radian>().sin(),
            self.view.az().to::<Radian>().value(),
        );
        let mut samples = Vec::with_capacity(nmu * nphi);
        for (mu, weight) in gauss(nmu)? {
            for j in 0..nphi {
                let az = TAU * j as f64 / nphi as f64;
                let direction = Horizontal::new(
                    Degrees::new(mu.asin().to_degrees()),
                    Degrees::new(az.to_degrees()),
                );
                let incident = finite_nonnegative(
                    self.sky.radiance(self.wavelength, direction)?.value(),
                    "angular illumination",
                )?;
                // Both propagation vectors are negatives of the source/view
                // directions. Roundoff-only cosine clipping before acos.
                let cosine = mu * out_mu
                    + (1.0 - mu * mu).sqrt() * (1.0 - out_mu * out_mu).sqrt() * (az - out_az).cos();
                let theta = Radians::new(cosine.clamp(-1.0, 1.0).acos());
                let solid_angle = Steradians::new(weight * TAU / nphi as f64);
                let r = phase_solid_angle(
                    RayleighPhaseFunction.phase(self.wavelength, theta).value(),
                    solid_angle,
                );
                let a = phase_solid_angle(self.phase.density(self.wavelength, theta)?, solid_angle);
                samples.push(Sample {
                    mu,
                    rayleigh: finite_nonnegative(incident * r, "weighted Rayleigh illumination")?,
                    aerosol: finite_nonnegative(incident * a, "weighted aerosol illumination")?,
                });
            }
        }
        let path = gauss(nz)?;
        let edges = self.atmosphere.edges();
        let mut result = 0.0;
        for bounds in edges.windows(2) {
            let thickness = bounds[1].value() - bounds[0].value();
            let optics = self
                .atmosphere
                .coefficients(self.wavelength, bounds[0])
                .map_err(upstream)?;
            let (r, a) = (
                optics.rayleigh().sigma_s.value(),
                optics.aerosol().sigma_s.value(),
            );
            if r == 0.0 && a == 0.0 {
                continue;
            }
            for &(node, weight) in &path {
                let height = Meters::new(bounds[0].value() + node * thickness);
                let above = self
                    .atmosphere
                    .vertical_optical_depth(self.wavelength, height, edges[edges.len() - 1])
                    .map_err(upstream)?
                    .extinction()
                    .value();
                let below = self
                    .atmosphere
                    .vertical_optical_depth(self.wavelength, edges[0], height)
                    .map_err(upstream)?
                    .extinction()
                    .value();
                let mut source = 0.0;
                for sample in &samples {
                    source +=
                        (r * sample.rayleigh + a * sample.aerosol) * (-above / sample.mu).exp();
                }
                result += source * (-below / out_mu).exp() * (thickness / out_mu) * weight;
            }
        }
        finite_nonnegative(result, "single-scattered radiance (arithmetic overflow)")
    }
}
