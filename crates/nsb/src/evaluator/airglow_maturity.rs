use super::types::NsbModelConfig;
use crate::components::airglow::AirglowScientificProfile;
use crate::site::CalibrationStatus;

impl NsbModelConfig {
    /// Return the explicitly selected Airglow scientific profile.
    ///
    /// Observer coordinates and observatory identity are query inputs, not part
    /// of this profile. Operational builders such as F10.7 and geometry preserve
    /// this value.
    pub const fn airglow_scientific_profile(&self) -> AirglowScientificProfile {
        AirglowScientificProfile::BuiltIn(self.site_profile)
    }

    /// Return the evidence-backed Airglow calibration maturity.
    pub const fn airglow_calibration_status(&self) -> CalibrationStatus {
        self.airglow_scientific_profile().calibration_status()
    }

    /// Return true only when the selected Airglow profile is site-calibrated.
    pub const fn is_airglow_site_calibrated(&self) -> bool {
        self.airglow_scientific_profile().is_site_calibrated()
    }
}
