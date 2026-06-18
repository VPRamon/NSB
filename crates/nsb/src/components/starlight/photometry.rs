use super::output::StarlightOutputs;
use qtty::radiometry::{PhotonsPerSquareCentimeterNanosecondSteradian as BandPhotonRadiance, S10s};

pub(crate) fn scale_outputs(out: StarlightOutputs, scale: f64) -> StarlightOutputs {
    StarlightOutputs::new(
        BandPhotonRadiance::new(out.integrated.value() * scale),
        S10s::new(out.b_flux_s10.value() * scale),
        S10s::new(out.v_flux_s10.value() * scale),
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
        BandPhotonRadiance::new(
            q00.integrated.value() * w00
                + q10.integrated.value() * w10
                + q01.integrated.value() * w01
                + q11.integrated.value() * w11,
        ),
        S10s::new(
            q00.b_flux_s10.value() * w00
                + q10.b_flux_s10.value() * w10
                + q01.b_flux_s10.value() * w01
                + q11.b_flux_s10.value() * w11,
        ),
        S10s::new(
            q00.v_flux_s10.value() * w00
                + q10.v_flux_s10.value() * w10
                + q01.v_flux_s10.value() * w01
                + q11.v_flux_s10.value() * w11,
        ),
    )
}
