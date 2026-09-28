//! Application-layer scientific site-profile presets.
//!
//! Observatory coordinates live in `data/observatories.toml`. Scientific
//! atmospheric/profile assumptions live here so the core `nsb` crate stays free
//! of observatory catalogs. Adding MAGIC, H.E.S.S., or a caller-defined profile
//! means extending this module (or loading equivalent data), not changing
//! `crates/nsb`.

use nsb::site::{AtmosphericConditions, SiteProfileSpec};
use siderust::qtty::{Hectopascals, Kilometers};

/// CLI / data-layer name for the generic clear-sky profile.
pub const GENERIC_CLEAR_SKY: &str = "generic-clear-sky";
/// CLI name for the CTAO-North planning profile.
pub const CTA_NORTH: &str = "cta-north";
/// CLI name for the CTAO-South planning profile.
pub const CTA_SOUTH: &str = "cta-south";

/// CTAO-North planning assumptions (not an observatory identity).
///
/// Preserves the historical core-library planning pressure (770 hPa),
/// representative 2.2 km altitude, Paranal-like clear-sky Mie parameters, and
/// `PlanningPreset` maturity.
pub fn ctao_north_planning() -> SiteProfileSpec {
    SiteProfileSpec::planning(
        "ctao-north-planning",
        "ctao-north-planning",
        Kilometers::new(2.2),
        AtmosphericConditions::clear_sky_with_pressure(Hectopascals::new(770.0)),
        concat!(
            "CTAO-North planning preset: representative ORM/La Palma ",
            "altitude, fixed planning pressure, Siderust default ",
            "Rayleigh scale height, and bundled Paranal-like clear-sky ",
            "Mie parameterization. This is not yet a validated CTA-N ",
            "aerosol calibration and does not identify the observer as ORM."
        ),
    )
}

/// CTAO-South planning assumptions (not an observatory identity).
///
/// Preserves the historical Paranal-like `EL_PARANAL` atmospheric alias,
/// representative 2.1 km altitude, and `PlanningPreset` maturity.
pub fn ctao_south_planning() -> SiteProfileSpec {
    SiteProfileSpec::planning(
        "ctao-south-planning",
        "ctao-south-planning",
        Kilometers::new(2.1),
        AtmosphericConditions::paranal_average(),
        concat!(
            "CTAO-South planning preset: Paranal-like atmosphere from ",
            "Siderust AtmosphereProfile::EL_PARANAL used as a planning ",
            "assumption. This is not yet a dedicated CTA-S aerosol ",
            "calibration and does not identify the observer as Paranal."
        ),
    )
}

/// Resolve a CLI `--site-profile` value to a scientific profile specification.
pub fn resolve(name: &str) -> Option<SiteProfileSpec> {
    match name {
        GENERIC_CLEAR_SKY | "generic" => Some(SiteProfileSpec::generic_clear_sky()),
        CTA_NORTH | "ctao-north" | "ctao-north-planning" => Some(ctao_north_planning()),
        CTA_SOUTH | "ctao-south" | "ctao-south-planning" => Some(ctao_south_planning()),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use nsb::site::CalibrationStatus;
    use siderust::coordinates::centers::Geodetic;
    use siderust::coordinates::frames::ECEF;
    use siderust::qtty::{Degrees, Meters};

    fn observer(height_m: f64) -> Geodetic<ECEF> {
        Geodetic::new_raw(
            Degrees::new(-17.89),
            Degrees::new(28.76),
            Meters::new(height_m),
        )
    }

    #[test]
    fn ctao_presets_preserve_planning_assumptions() {
        let north = ctao_north_planning().resolve(observer(2_200.0));
        let south = ctao_south_planning().resolve(observer(2_100.0));

        assert_eq!(north.id.as_str(), "ctao-north-planning");
        assert_eq!(south.id.as_str(), "ctao-south-planning");
        assert_eq!(north.calibration_status, CalibrationStatus::PlanningPreset);
        assert_eq!(south.calibration_status, CalibrationStatus::PlanningPreset);
        assert_eq!(north.atmosphere.surface_pressure.value(), 770.0);
        assert_eq!(
            south.atmosphere.surface_pressure,
            AtmosphericConditions::paranal_average().surface_pressure
        );
        assert!(north.atmosphere_provenance.contains("CTAO-North"));
        assert!(south.atmosphere_provenance.contains("CTAO-South"));
    }

    #[test]
    fn observatory_identity_is_independent_of_profile_resolution() {
        assert_eq!(
            resolve("cta-north").unwrap().id().as_str(),
            "ctao-north-planning"
        );
        assert_eq!(
            resolve("cta-south").unwrap().id().as_str(),
            "ctao-south-planning"
        );
        assert_eq!(
            resolve("generic-clear-sky").unwrap().id().as_str(),
            "generic-clear-sky"
        );
        assert!(resolve("not-a-real-profile").is_none());
    }
}
