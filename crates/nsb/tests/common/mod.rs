//! Local starlight provenance helper for repository tests and benches.
//!
//! Intentionally not part of the `nsb` public API (#175).

use nsb::components::starlight::StarlightProvenance;

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
