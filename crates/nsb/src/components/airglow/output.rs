use qtty::radiometry::{PhotonsPerSquareCentimeterNanosecondSteradian as BandPhotonRadiance, S10s};

#[derive(Debug, Clone)]
/// Integrated airglow radiance and diagnostic B/V values.
pub struct AirglowOutputs {
    /// Photon radiance integrated over 300–650 nm.
    pub integrated: BandPhotonRadiance,
    /// Monochromatic B-reference S10 diagnostic.
    pub b_flux_s10: S10s,
    /// Monochromatic V-reference S10 diagnostic.
    pub v_flux_s10: S10s,
    /// Relative one-sigma uncertainty estimated from the bundled empirical
    /// continuum calibration. `None` means the airglow model returned no
    /// physical emission for the query, so no relative uncertainty is defined.
    pub relative_uncertainty: Option<f64>,
}

impl AirglowOutputs {
    pub(crate) fn zero() -> Self {
        Self {
            integrated: BandPhotonRadiance::zero(),
            b_flux_s10: S10s::zero(),
            v_flux_s10: S10s::zero(),
            relative_uncertainty: None,
        }
    }
}
