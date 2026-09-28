//! Local helpers for repository integration tests and benches.
//!
//! Intentionally not part of the `nsb` public API (#175).

use nsb::components::starlight::StarlightProvenance;
use nsb::site::{AtmosphericConditions, SiteProfileSpec};
use siderust::qtty::{Hectopascals, Kilometers};

/// CTAO-North planning assumptions (application-layer preset mirrored for tests).
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

/// CTAO-South planning assumptions (application-layer preset mirrored for tests).
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

/// Deterministic provenance for synthetic HEALPix fixtures.
pub fn starlight_test_provenance() -> StarlightProvenance {
    let mut provenance = StarlightProvenance::new(
        "NSB test fixture starlight map",
        "fixture",
        "2026-06-17",
        "synthetic unit-test fixture",
        "test-only",
        "test-only",
        "integrated 300-650 nm photon radiance",
        "HEALPix nside=1 ring 12 pixels",
        None::<String>,
    );
    provenance.source_catalogue_release = Some("test".to_string());
    provenance.photometry_model = Some("fixture".to_string());
    provenance.generated_by = Some("test".to_string());
    provenance.source_selection = Some("synthetic fixture".to_string());
    provenance.generation_command = Some("test fixture generation".to_string());
    provenance.validation_report = Some("test-only".to_string());
    provenance.calibration_status = Some("experimental".to_string());
    provenance
}
