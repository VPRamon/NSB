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
mod crossmatch;
mod photometry;
mod policy;

pub use artifact::{
    load_bright_star_artifact, BrightStarArtifact, BrightStarPixel, BrightStarSourceRecord,
    BRIGHT_STAR_ARTIFACT_SCHEMA_VERSION, BRIGHT_STAR_MODEL_ID,
};
pub use crossmatch::{classify_match, CrossmatchDecision, MatchCandidate};
pub use photometry::{
    integrate_template_photon_flux, magnitude_to_f_lambda_si, BandZeroPoint, SpectralTemplate,
};
pub use policy::{
    BrightStarPopulationPolicy, BrightStarPrecedencePolicy, SupplementClass,
    POPULATION_POLICY_ID_V1, PRECEDENCE_POLICY_ID_V1,
};
