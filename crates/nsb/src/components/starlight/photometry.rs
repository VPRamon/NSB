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

pub(crate) fn bilinear_outputs(
    q00: StarlightOutputs,
    q10: StarlightOutputs,
    q01: StarlightOutputs,
    q11: StarlightOutputs,
    tx: f64,
    ty: f64,
) -> StarlightOutputs {
    let w00 = (1.0 - tx) * (1.0 - ty);
    let w10 = tx * (1.0 - ty);
    let w01 = (1.0 - tx) * ty;
    let w11 = tx * ty;

    let mut interpolated = StarlightOutputs::new(
        q00.integrated * w00 + q10.integrated * w10 + q01.integrated * w01 + q11.integrated * w11,
        q00.b_flux_s10 * w00 + q10.b_flux_s10 * w10 + q01.b_flux_s10 * w01 + q11.b_flux_s10 * w11,
        q00.v_flux_s10 * w00 + q10.v_flux_s10 * w10 + q01.v_flux_s10 * w01 + q11.v_flux_s10 * w11,
    );
    interpolated.s10_diagnostics_provided = q00.s10_diagnostics_provided
        && q10.s10_diagnostics_provided
        && q01.s10_diagnostics_provided
        && q11.s10_diagnostics_provided;
    let weighted = |q00: Option<_>, q10: Option<_>, q01: Option<_>, q11: Option<_>| {
        Some(q00? * w00 + q10? * w10 + q01? * w01 + q11? * w11)
    };
    match (
        weighted(
            q00.statistical_uncertainty,
            q10.statistical_uncertainty,
            q01.statistical_uncertainty,
            q11.statistical_uncertainty,
        ),
        weighted(
            q00.systematic_uncertainty,
            q10.systematic_uncertainty,
            q01.systematic_uncertainty,
            q11.systematic_uncertainty,
        ),
        weighted(
            q00.total_uncertainty,
            q10.total_uncertainty,
            q01.total_uncertainty,
            q11.total_uncertainty,
        ),
    ) {
        (Some(statistical), Some(systematic), Some(total)) => {
            interpolated.with_uncertainties(statistical, systematic, total)
        }
        _ => interpolated,
    }
}
