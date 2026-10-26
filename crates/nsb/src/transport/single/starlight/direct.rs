// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Vallés Puig, Ramon
//! Bounded native-bin integration of the shared direct operator, without angles.
use super::*;

pub(crate) struct DirectBin {
    pub direct: BandRadiance,
    pub order: usize,
    pub change: BandRadiance,
    pub planned_work: usize,
}

pub(crate) fn direct_bin(
    atmosphere: &LayeredAtmosphere,
    sky: &StarlightSky<'_>,
    view: Horizontal,
    settings: WavelengthSettings,
) -> Result<DirectBin> {
    settings.validate()?;
    let n = settings.nodes;
    let r = settings.refinements;
    let calls = n * ((1 << (r + 1)) - 1);
    let wavelength_work: usize = (0..=r)
        .map(|i| {
            let order = n * (1 << i);
            64 * order * order.div_ceil(2)
        })
        .sum();
    let planned_work = (atmosphere.edges().len() - 1)
        .checked_add(1)
        .and_then(|layers| layers.checked_mul(calls))
        .and_then(|work| work.checked_add(wavelength_work))
        .ok_or_else(|| invalid("Starlight direct work estimate overflow"))?;
    if planned_work > budget::MAX_WORK {
        return Err(invalid(
            "Starlight direct plan exceeds 250 million work units",
        ));
    }
    let domain = atmosphere.wavelength_domain();
    if sky.band[0] < domain[0] || sky.band[1] > domain[1] {
        return Err(invalid("atmosphere must cover the complete Starlight bin"));
    }
    let integrate = |order| -> Result<f64> {
        let mut sum = 0.0;
        for (node, weight) in quadrature::gauss(order)? {
            let w = sky.band[0] + (sky.band[1] - sky.band[0]) * node;
            let (direct, _) = transmitted_illumination(atmosphere, sky, w, view)?;
            sum += direct * (sky.band[1] - sky.band[0]).value() * weight;
        }
        finite_nonnegative(sum, "integrated Starlight direct radiance")
    };
    let mut previous = integrate(n)?;
    for i in 1..=r {
        let order = n * (1 << i);
        let current = integrate(order)?;
        let change = (current - previous).abs();
        if change
            <= settings.absolute_tolerance.value()
                + settings.relative_tolerance * current.abs().max(previous.abs())
        {
            return Ok(DirectBin {
                direct: BandRadiance::new(current),
                order,
                change: BandRadiance::new(change),
                planned_work,
            });
        }
        previous = current;
    }
    Err(invalid("Starlight direct wavelength convergence failed"))
}
