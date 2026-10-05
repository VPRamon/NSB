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

// Single-line `pub use` statements keep declaration-only module files
// recognisable to the coverage-gate non-instrumentable classifier when LCOV
// omits pure re-export crates (multi-line brace continuations fail closed).
pub use artifact::artifact_compatible_with_product_band;
pub use artifact::is_combined_product_band;
pub use artifact::load_bright_star_artifact;
pub use artifact::BrightStarArtifact;
pub use artifact::BrightStarInputProvenance;
pub use artifact::BrightStarInputRole;
pub use artifact::BrightStarPixel;
pub use artifact::BrightStarSourceRecord;
pub use artifact::BrightStarSupplementProvenance;
pub use artifact::CorrelatedUncertainty;
pub use artifact::BRIGHT_STAR_ARTIFACT_SCHEMA_VERSION;
pub use artifact::BRIGHT_STAR_COMBINED_ARTIFACT_SCHEMA_VERSION;
pub use artifact::BRIGHT_STAR_COMBINED_MODEL_ID;
pub use artifact::BRIGHT_STAR_COMBINED_PRODUCT_BAND_ID;
pub use artifact::BRIGHT_STAR_MODEL_ID;
pub use artifact::BRIGHT_STAR_PRODUCT_BAND_ID;
pub use build_run::run_experimental_build;
pub use build_run::BrightStarBuildRunConfig;
pub use build_run::BrightStarBuildRunManifest;
pub use builder::build_experimental_artifact;
pub use builder::build_experimental_artifact_for_product_band;
pub use builder::BrightStarBuildDiagnostics;
pub use builder::BrightStarSourceDiagnostic;
pub use builder::ProperMotionDiagnostic;
pub use builder::SpectralEstimate;
pub use catalogue::ingest_gaia_quality_extract;
pub use catalogue::ingest_hip_gaia_crossmatch;
pub use catalogue::ingest_hipparcos2;
pub use catalogue::ingest_tycho2;
pub use catalogue::ingest_xhip;
pub use catalogue::HipGaiaIdentityMatch;
pub use catalogue::Hipparcos2Record;
pub use catalogue::PinnedCatalogueInput;
pub use catalogue::Tycho2Ingestion;
pub use catalogue::Tycho2Photometry;
pub use catalogue::XhipRecord;
pub use crossmatch::classify_match;
pub use crossmatch::positional_match_candidates;
pub use crossmatch::propagate_hipparcos_3d_to_j2016;
pub use crossmatch::propagate_hipparcos_to_j2016;
pub use crossmatch::propagation_2d_3d_difference_arcsec;
pub use crossmatch::CrossmatchDecision;
pub use crossmatch::GaiaMatchRow;
pub use crossmatch::HipparcosAstrometry;
pub use crossmatch::MatchCandidate;
pub use crossmatch::PropagatedPosition;
pub use photometry::integrate_template_photon_flux;
pub use photometry::integrate_template_through_response;
pub use photometry::magnitude_to_band_photon_flux;
pub use photometry::magnitude_to_f_lambda_si;
pub use photometry::reconstruct_template_band_flux;
pub use photometry::BandZeroPoint;
pub use photometry::PhotometricBandCalibration;
pub use photometry::PhotometricBandResponse;
pub use photometry::SpectralTemplate;
pub use policy::BrightStarPopulationPolicy;
pub use policy::BrightStarPrecedencePolicy;
pub use policy::SupplementClass;
pub use policy::POPULATION_POLICY_ID_V1;
pub use policy::PRECEDENCE_POLICY_ID_V1;
pub use reconstruction::load_spectral_reconstruction_model;
pub use reconstruction::reconstruct_spectral_estimates;
pub use reconstruction::SpectralReconstructionModel;
pub use reconstruction::TemplateAssignment;
pub use reconstruction::SPECTRAL_RECONSTRUCTION_MODEL_ID_V1;
pub use reconstruction::UV_COMPLETION_MODEL_ID_V1;
