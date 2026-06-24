use qtty::radiometry::{PhotonsPerSquareCentimeterNanosecondSteradian as BandPhotonRadiance, S10s};

#[derive(Debug, Clone, Copy, PartialEq)]
/// Integrated starlight radiance and diagnostic B/V values.
pub struct StarlightOutputs {
    /// Photon radiance integrated over 300–650 nm.
    pub integrated: BandPhotonRadiance,
    /// B-reference S10 diagnostic.
    pub b_flux_s10: S10s,
    /// V-reference S10 diagnostic.
    pub v_flux_s10: S10s,
}

impl StarlightOutputs {
    /// Construct starlight outputs.
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
