use qtty::radiometry::{PhotonsPerSquareCentimeterNanosecondSteradian as BandPhotonRadiance, S10s};

#[derive(Debug, Clone)]
pub struct AirglowOutputs {
    pub integrated: BandPhotonRadiance,
    pub b_flux_s10: S10s,
    pub v_flux_s10: S10s,
}

impl AirglowOutputs {
    pub(crate) fn zero() -> Self {
        Self {
            integrated: BandPhotonRadiance::zero(),
            b_flux_s10: S10s::zero(),
            v_flux_s10: S10s::zero(),
        }
    }
}
