// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Vallés Puig, Ramon
//! Conservative, solver-local work counts for the complete refinement plan.
use super::*;
use std::mem::size_of;

pub(super) const MAX_WORK: usize = 250_000_000;
pub(super) const MAX_TEMPORARY_BYTES: usize = 8 * 1024 * 1024;

#[derive(Debug, Default, PartialEq, Eq)]
pub(super) struct Work {
    pub angular_samples: usize,
    pub source_terms: usize,
    pub layer_visits: usize,
    pub legendre_steps: usize,
    pub peak_bytes: usize,
}

fn checked(value: Option<usize>) -> Result<usize> {
    value.ok_or_else(|| invalid("single-scattering computational budget estimate overflow"))
}

impl Work {
    pub fn total(&self) -> Result<usize> {
        checked(
            self.angular_samples
                .checked_add(self.source_terms)
                .and_then(|n| n.checked_add(self.layer_visits))
                .and_then(|n| n.checked_add(self.legendre_steps)),
        )
    }

    fn integration(&mut self, layers: usize, [mu, azimuth, path]: [usize; 3]) -> Result<()> {
        let samples = checked(mu.checked_mul(azimuth))?;
        let path_points = checked(layers.checked_mul(path))?;
        self.angular_samples = checked(self.angular_samples.checked_add(samples))?;
        self.source_terms = checked(
            path_points
                .checked_mul(samples)
                .and_then(|n| self.source_terms.checked_add(n)),
        )?;
        // Two exact depth queries per path point, each visiting up to L layers.
        // Also conservatively allow L comparisons for every coefficient lookup.
        let visits = checked(
            path_points
                .checked_mul(2)
                .and_then(|n| n.checked_add(layers))
                .and_then(|n| n.checked_mul(layers)),
        )?;
        self.layer_visits = checked(self.layer_visits.checked_add(visits))?;
        // gauss() allows 64 Newton iterations; each root evaluates n Legendre
        // recurrence steps, for ceil(n/2) roots. Include both rules each time.
        for n in [mu, path] {
            let recurrence = checked(n.checked_mul(n.div_ceil(2)).and_then(|n| n.checked_mul(64)))?;
            self.legendre_steps = checked(self.legendre_steps.checked_add(recurrence))?;
        }
        // During angular sampling the mu rule coexists with Sample storage;
        // afterward the path rule replaces it. Integrations run sequentially.
        let bytes = checked(samples.checked_mul(quadrature::SAMPLE_BYTES).and_then(|n| {
            mu.max(path)
                .checked_mul(size_of::<(f64, f64)>())
                .and_then(|rules| n.checked_add(rules))
        }))?;
        self.peak_bytes = self.peak_bytes.max(bytes);
        Ok(())
    }

    pub fn enforce(&self, layers: usize, settings: SolverSettings) -> Result<()> {
        let total = self.total()?;
        if total > MAX_WORK || self.peak_bytes > MAX_TEMPORARY_BYTES {
            return Err(invalid(&format!(
                "single-scattering computational budget exceeded: layers={layers}, settings={settings:?}, work={total} (maximum {MAX_WORK}), temporary bytes={} (maximum {MAX_TEMPORARY_BYTES}); reduce layers, orders or permitted refinements",
                self.peak_bytes
            )));
        }
        Ok(())
    }
}

/// Validate settings before looping; even adversarial refinement counts are bounded.
/// Charge all configured refinements, despite possible early numerical convergence
/// or zero-scattering exits. The previous joint result supplies the next coarse
/// result and is not charged again. No sky queries or numerical arrays are needed.
pub(super) fn estimate(settings: SolverSettings, layers: usize) -> Result<Work> {
    settings.validate()?;
    let mut work = Work {
        layer_visits: layers,
        ..Work::default()
    }; // Direct column.
    let mut orders = [
        settings.mu_nodes,
        settings.azimuth_nodes,
        settings.path_nodes,
    ];
    work.integration(layers, orders)?;
    for _ in 0..settings.refinements {
        let refined = [
            checked(orders[0].checked_mul(2))?,
            checked(orders[1].checked_mul(2))?,
            checked(orders[2].checked_mul(2))?,
        ];
        work.integration(layers, [refined[0], refined[1], orders[2]])?;
        work.integration(layers, [orders[0], orders[1], refined[2]])?;
        work.integration(layers, refined)?;
        orders = refined;
    }
    Ok(work)
}
