use super::output::StarlightOutputs;

pub(crate) fn scale_outputs(out: StarlightOutputs, scale: f64) -> StarlightOutputs {
    StarlightOutputs::new(
        out.integrated * scale,
        out.b_flux_s10 * scale,
        out.v_flux_s10 * scale,
    )
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

    StarlightOutputs::new(
        q00.integrated * w00
            + q10.integrated * w10
            + q01.integrated * w01
            + q11.integrated * w11,
        q00.b_flux_s10 * w00
            + q10.b_flux_s10 * w10
            + q01.b_flux_s10 * w01
            + q11.b_flux_s10 * w11,
        q00.v_flux_s10 * w00
            + q10.v_flux_s10 * w10
            + q01.v_flux_s10 * w01
            + q11.v_flux_s10 * w11,
    )
}
