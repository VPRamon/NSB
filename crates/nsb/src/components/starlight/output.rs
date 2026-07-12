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
    /// Statistical one-sigma uncertainty of the integrated photon radiance.
    pub statistical_uncertainty: Option<BandPhotonRadiance>,
    /// Systematic one-sigma uncertainty of the integrated photon radiance.
    pub systematic_uncertainty: Option<BandPhotonRadiance>,
    /// Total one-sigma uncertainty of the integrated photon radiance.
    pub total_uncertainty: Option<BandPhotonRadiance>,
}

impl StarlightOutputs {
    /// Construct starlight outputs.
    pub fn new(integrated: BandPhotonRadiance, b_flux_s10: S10s, v_flux_s10: S10s) -> Self {
        Self {
            integrated,
            b_flux_s10,
            v_flux_s10,
            statistical_uncertainty: None,
            systematic_uncertainty: None,
            total_uncertainty: None,
        }
    }

    /// Attach a complete absolute-uncertainty triplet.
    ///
    /// Map validation rejects non-finite, negative, partial, or inconsistent
    /// triplets before an output can be returned by a [`super::StarlightMap`].
    pub fn with_uncertainties(
        mut self,
        statistical: BandPhotonRadiance,
        systematic: BandPhotonRadiance,
        total: BandPhotonRadiance,
    ) -> Self {
        self.statistical_uncertainty = Some(statistical);
        self.systematic_uncertainty = Some(systematic);
        self.total_uncertainty = Some(total);
        self
    }

    /// Relative total one-sigma uncertainty when it is mathematically defined.
    pub fn relative_uncertainty(self) -> Option<f64> {
        let total = self.total_uncertainty?.value();
        let integrated = self.integrated.value();
        if integrated > 0.0 {
            Some(total / integrated)
        } else if integrated == 0.0 && total == 0.0 {
            Some(0.0)
        } else {
            None
        }
    }

    pub(crate) fn is_finite_non_negative(self) -> bool {
        let values_are_valid = self.integrated.is_finite()
            && self.b_flux_s10.is_finite()
            && self.v_flux_s10.is_finite()
            && self.integrated.value() >= 0.0
            && self.b_flux_s10.value() >= 0.0
            && self.v_flux_s10.value() >= 0.0;
        values_are_valid && self.has_valid_uncertainties()
    }

    pub(crate) fn has_uncertainties(self) -> bool {
        self.statistical_uncertainty.is_some()
    }

    fn has_valid_uncertainties(self) -> bool {
        match (
            self.statistical_uncertainty,
            self.systematic_uncertainty,
            self.total_uncertainty,
        ) {
            (None, None, None) => true,
            (Some(statistical), Some(systematic), Some(total)) => {
                statistical.is_finite()
                    && systematic.is_finite()
                    && total.is_finite()
                    && statistical.value() >= 0.0
                    && systematic.value() >= 0.0
                    && total >= statistical
                    && total >= systematic
            }
            _ => false,
        }
    }
}
