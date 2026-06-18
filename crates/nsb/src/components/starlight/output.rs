use qtty::radiometry::{PhotonsPerSquareCentimeterNanosecondSteradian as BandPhotonRadiance, S10s};

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct StarlightOutputs {
    pub integrated: BandPhotonRadiance,
    pub b_flux_s10: S10s,
    pub v_flux_s10: S10s,
}

impl StarlightOutputs {
    pub fn new(integrated: BandPhotonRadiance, b_flux_s10: S10s, v_flux_s10: S10s) -> Self {
        Self {
            integrated,
            b_flux_s10,
            v_flux_s10,
        }
    }

    pub(crate) fn is_finite_non_negative(self) -> bool {
        self.integrated.is_finite()
            && self.b_flux_s10.is_finite()
            && self.v_flux_s10.is_finite()
            && self.integrated.value() >= 0.0
            && self.b_flux_s10.value() >= 0.0
            && self.v_flux_s10.value() >= 0.0
    }
}
