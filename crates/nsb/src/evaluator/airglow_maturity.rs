use super::types::NsbModelConfig;
use crate::site::CalibrationStatus;

impl NsbModelConfig {
    /// Return the evidence-backed Airglow calibration maturity.
    ///
    /// Observer coordinates, geometry, and solar-activity inputs do not change
    /// the scientific maturity selected by the site profile.
    pub const fn airglow_calibration_status(&self) -> CalibrationStatus {
        self.site_profile.calibration_status()
    }

    /// Return true only when the selected Airglow site profile is calibrated.
    pub const fn is_airglow_site_calibrated(&self) -> bool {
        self.site_profile.is_site_calibrated()
    }
}
