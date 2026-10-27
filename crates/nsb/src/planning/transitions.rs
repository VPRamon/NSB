// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Vallés Puig, Ramon
//! Bounded-memory empirical discovery of changes in a discrete physical regime.
//!
//! Always probe the midpoint, including equal-endpoint intervals. Refine both
//! halves of a detected change, so a bracket need not contain only one edge.
//! Excursions wholly between the discovery probes can still be missed; this is
//! intentionally not a continuous geometric certificate.
use crate::{NsbError, Result};
use siderust::qtty::Days;
use siderust::time::{Interval, ModifiedJulianDate};

pub(super) const TOLERANCE: f64 = 0.0001 / 86400.0;
const MAX_PROBES: usize = 5_000_000;
const MAX_CUTS: usize = 100_000;

pub(super) fn discover<S: Copy + Eq>(
    windows: &[Interval<ModifiedJulianDate>],
    step: f64,
    probe: impl Fn(ModifiedJulianDate) -> Result<S>,
) -> Result<Vec<ModifiedJulianDate>> {
    let mut remaining = MAX_PROBES;
    let mut sample = |t| {
        remaining = remaining
            .checked_sub(1)
            .ok_or_else(|| NsbError::Unsupported("physical regime probe budget exceeded".into()))?;
        probe(t)
    };
    let mut cuts = Vec::new();
    for window in windows {
        let duration = window.length().value();
        if !duration.is_finite() || duration <= 0.0 {
            continue;
        }
        let mut offset = 0.0;
        let mut left = window.start;
        let mut state = sample(left)?;
        while offset < duration {
            // Offsets are local to each eligible interval. No accumulated MJD
            // additions or materialized sample arrays, including at block edges.
            let next = (offset + step).min(duration);
            let right = window.start + Days::new(next);
            if right <= left {
                return Err(NsbError::Unsupported(
                    "physical regime time resolution".into(),
                ));
            }
            let right_state = sample(right)?;
            refine(left, state, right, right_state, &mut sample, &mut cuts)?;
            left = right;
            state = right_state;
            offset = next;
        }
    }
    cuts.sort_by(|a, b| a.partial_cmp(b).expect("finite TT instants"));
    // Two model edges (or roundoff around an exact shared vertex) can yield
    // cuts closer than our refinement resolution. Keep one representative:
    // otherwise a sub-resolution physical window can straddle the same jump
    // and cannot be fitted. Each discarded cut is within TOLERANCE of the
    // retained one, inside the adaptive engine's 1 ms open-interior inset.
    cuts.dedup_by(|right, left| (*right - *left).value().abs() <= TOLERANCE);
    Ok(cuts)
}

fn refine<S: Copy + Eq>(
    left: ModifiedJulianDate,
    a: S,
    right: ModifiedJulianDate,
    b: S,
    sample: &mut impl FnMut(ModifiedJulianDate) -> Result<S>,
    cuts: &mut Vec<ModifiedJulianDate>,
) -> Result<()> {
    let mid = left + Days::new((right - left).value() * 0.5);
    if (right - left).value() <= TOLERANCE || mid == left || mid == right {
        if a != b {
            if cuts.len() >= MAX_CUTS {
                return Err(NsbError::Unsupported(
                    "physical regime transition budget exceeded".into(),
                ));
            }
            cuts.push(mid);
        }
        return Ok(());
    }
    let m = sample(mid)?;
    if a != m || m != b {
        refine(left, a, mid, m, sample, cuts)?;
        refine(mid, m, right, b, sample, cuts)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn discovers_return_excursions_and_multiple_edges() {
        let start = ModifiedJulianDate::new(60000.0);
        let windows = [Interval::new(start, start + Days::new(1.0))];
        for edges in [vec![0.4, 0.6], vec![0.2, 0.4, 0.6, 0.8]] {
            let cuts = discover(&windows, 0.5, |t| {
                Ok(edges.iter().filter(|&&x| (t - start).value() >= x).count() % 2)
            })
            .unwrap();
            assert_eq!(cuts.len(), edges.len());
            for (cut, expected) in cuts.iter().zip(edges) {
                assert!(((*cut - start).value() - expected).abs() <= TOLERANCE);
            }
        }
    }
    #[test]
    fn empty_windows_do_not_probe() {
        assert!(discover::<u8>(&[], 1.0, |_| panic!("irrelevant probe"))
            .unwrap()
            .is_empty());
    }
}
