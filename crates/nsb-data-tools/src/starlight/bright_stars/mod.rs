//! Optional very-bright-star supplement for Starlight (#182).
//!
//! Scientific design: `docs/nsb_components/starlight/bright-stars/design-v1.md`.
//!
//! This module provides deterministic ingest, Gaia crossmatch classification,
//! band integration helpers, and a versioned external artifact contract. It
//! does **not** embed Hipparcos/Tycho/XHIP catalogue bytes (CC BY-NC / CDS
//! origin constraints; human redistribution gate #103). Callers supply an
//! external, checksum-pinned artifact.

mod artifact;
mod build_run;
mod builder;
mod catalogue;
mod crossmatch;
mod photometry;
mod policy;
mod reconstruction;

pub use artifact::{
    load_bright_star_artifact, BrightStarArtifact, BrightStarInputProvenance, BrightStarInputRole,
    BrightStarPixel, BrightStarSourceRecord, CorrelatedUncertainty,
    BRIGHT_STAR_ARTIFACT_SCHEMA_VERSION, BRIGHT_STAR_MODEL_ID, BRIGHT_STAR_PRODUCT_BAND_ID,
};
pub use build_run::{run_experimental_build, BrightStarBuildRunConfig, BrightStarBuildRunManifest};
pub use builder::{
    build_experimental_artifact, BrightStarBuildDiagnostics, BrightStarSourceDiagnostic,
    ProperMotionDiagnostic, SpectralEstimate,
};
pub use catalogue::{
    ingest_gaia_quality_extract, ingest_hip_gaia_crossmatch, ingest_hipparcos2, ingest_tycho2,
    ingest_xhip, HipGaiaIdentityMatch, Hipparcos2Record, PinnedCatalogueInput, Tycho2Ingestion,
    Tycho2Photometry, XhipRecord,
};
pub use crossmatch::{
    classify_match, positional_match_candidates, propagate_hipparcos_3d_to_j2016,
    propagate_hipparcos_to_j2016, propagation_2d_3d_difference_arcsec, CrossmatchDecision,
    GaiaMatchRow, HipparcosAstrometry, MatchCandidate, PropagatedPosition,
};
pub use photometry::{
    integrate_template_photon_flux, integrate_template_through_response,
    magnitude_to_band_photon_flux, magnitude_to_f_lambda_si, reconstruct_template_band_flux,
    BandZeroPoint, PhotometricBandCalibration, PhotometricBandResponse, SpectralTemplate,
};
pub use policy::{
    BrightStarPopulationPolicy, BrightStarPrecedencePolicy, SupplementClass,
    POPULATION_POLICY_ID_V1, PRECEDENCE_POLICY_ID_V1,
};
pub use reconstruction::{
    load_spectral_reconstruction_model, reconstruct_spectral_estimates,
    SpectralReconstructionModel, TemplateAssignment, SPECTRAL_RECONSTRUCTION_MODEL_ID_V1,
};
