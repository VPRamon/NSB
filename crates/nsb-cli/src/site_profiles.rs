//! Application-layer scientific site-profile presets.
//!
//! Observatory coordinates live in `data/observatories.toml`. Scientific
//! atmospheric/profile assumptions live here so the core `nsb` crate stays free
//! of observatory catalogs. Adding MAGIC, H.E.S.S., or a caller-defined profile
//! means extending this module (or loading equivalent data), not changing
//! `crates/nsb`.

use nsb::site::{AtmosphericConditions, SiteProfile, SiteProfileTag};
use nsb::{GenericClearSky, NsbModelConfig};
use siderust::qtty::{Hectopascals, Kilometers};

/// CLI / data-layer name for the generic clear-sky profile.
pub const GENERIC_CLEAR_SKY: &str = "generic-clear-sky";
/// CLI name for the CTAO-North planning profile.
pub const CTA_NORTH: &str = "cta-north";
/// CLI name for the CTAO-South planning profile.
pub const CTA_SOUTH: &str = "cta-south";

/// Type-level identity for the CTAO-North application planning preset.
pub struct CtaNorth;

impl SiteProfileTag for CtaNorth {
    const NAME: &'static str = "ctao-north-planning";
}

/// Type-level identity for the CTAO-South application planning preset.
pub struct CtaSouth;

impl SiteProfileTag for CtaSouth {
    const NAME: &'static str = "ctao-south-planning";
}

/// CTAO-North planning assumptions (not an observatory identity).
///
/// Preserves the historical core-library planning pressure (770 hPa),
/// representative 2.2 km altitude, Paranal-like clear-sky Mie parameters, and
/// `PlanningPreset` maturity.
pub fn ctao_north_planning() -> SiteProfile<CtaNorth> {
    SiteProfile::<CtaNorth>::planning(
        Kilometers::new(2.2),
        AtmosphericConditions::clear_sky_with_pressure(Hectopascals::new(770.0))
            .expect("CTAO-North pressure is physical"),
        concat!(
            "CTAO-North planning preset: representative ORM/La Palma ",
            "altitude, fixed planning pressure, Siderust default ",
            "Rayleigh scale height, and bundled Paranal-like clear-sky ",
            "Mie parameterization. This is not yet a validated CTA-N ",
            "aerosol calibration and does not identify the observer as ORM."
        ),
    )
    .expect("built-in CTAO-North profile is valid")
}

/// CTAO-South planning assumptions (not an observatory identity).
///
/// Preserves the historical Paranal-like `EL_PARANAL` atmospheric alias,
/// representative 2.1 km altitude, and `PlanningPreset` maturity.
pub fn ctao_south_planning() -> SiteProfile<CtaSouth> {
    SiteProfile::<CtaSouth>::planning(
        Kilometers::new(2.1),
        AtmosphericConditions::paranal_average(),
        concat!(
            "CTAO-South planning preset: Paranal-like atmosphere from ",
            "Siderust AtmosphereProfile::EL_PARANAL used as a planning ",
            "assumption. This is not yet a dedicated CTA-S aerosol ",
            "calibration and does not identify the observer as Paranal."
        ),
    )
    .expect("built-in CTAO-South profile is valid")
}

/// Apply a runtime CLI token by constructing the corresponding typed profile.
pub fn apply(config: NsbModelConfig, name: &str) -> Option<NsbModelConfig> {
    match name {
        GENERIC_CLEAR_SKY | "generic" => {
            Some(config.with_site_profile(SiteProfile::<GenericClearSky>::generic_clear_sky()))
        }
        CTA_NORTH | "ctao-north" | "ctao-north-planning" => {
            Some(config.with_site_profile(ctao_north_planning()))
        }
        CTA_SOUTH | "ctao-south" | "ctao-south-planning" => {
            Some(config.with_site_profile(ctao_south_planning()))
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use nsb::site::CalibrationStatus;

    #[test]
    fn ctao_presets_preserve_planning_assumptions() {
        let north = ctao_north_planning();
        let south = ctao_south_planning();

        assert_eq!(north.name(), "ctao-north-planning");
        assert_eq!(south.name(), "ctao-south-planning");
        assert_eq!(
            north.calibration_status(),
            CalibrationStatus::PlanningPreset
        );
        assert_eq!(
            south.calibration_status(),
            CalibrationStatus::PlanningPreset
        );
        assert!(north
            .atmosphere_provenance()
            .contains("fixed planning pressure"));
        assert!(south.atmosphere_provenance().contains("EL_PARANAL"));
    }

    #[test]
    fn observatory_identity_is_independent_of_profile_resolution() {
        assert_eq!(
            apply(NsbModelConfig::generic_clear_sky(), "cta-north")
                .unwrap()
                .site_profile_name(),
            "ctao-north-planning"
        );
        assert_eq!(
            apply(NsbModelConfig::generic_clear_sky(), "cta-south")
                .unwrap()
                .site_profile_name(),
            "ctao-south-planning"
        );
        assert_eq!(
            apply(NsbModelConfig::generic_clear_sky(), "generic-clear-sky")
                .unwrap()
                .site_profile_name(),
            "generic-clear-sky"
        );
        assert!(apply(NsbModelConfig::generic_clear_sky(), "not-a-real-profile").is_none());
    }
}
