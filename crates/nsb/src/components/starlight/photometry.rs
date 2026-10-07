// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Vallés Puig, Ramon
use super::output::StarlightOutputs;
use crate::units::ScaleFactors;

pub(crate) fn scale_outputs(out: StarlightOutputs, scale: ScaleFactors) -> StarlightOutputs {
    let scale = scale.value();
    let mut scaled = StarlightOutputs::new(
        out.integrated * scale,
        out.b_flux_s10 * scale,
        out.v_flux_s10 * scale,
    );
    scaled.s10_diagnostics_provided = out.s10_diagnostics_provided;
    match (
        out.statistical_uncertainty,
        out.systematic_uncertainty,
        out.total_uncertainty,
    ) {
        (Some(statistical), Some(systematic), Some(total)) => {
            scaled.with_uncertainties(statistical * scale, systematic * scale, total * scale)
        }
        _ => scaled,
    }
}
