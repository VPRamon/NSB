//! Retained data-product command implementations exposed as library services.

/// Service implementation for `audit_gaia_starlight_exclusions`.
pub mod audit_gaia_starlight_exclusions;
/// Service implementation for `build_integrated_starlight_product`.
pub mod build_integrated_starlight_product;
/// Service implementation for `build_starlight_map`.
pub mod build_starlight_map;
/// Service implementation for `consolidate_gaia_starlight_samples`.
pub mod consolidate_gaia_starlight_samples;
/// Service implementation for `generate_gaia_starlight_release_inputs`.
pub mod generate_gaia_starlight_release_inputs;
/// Service implementation for `generate_starlight_sample_queries`.
pub mod generate_starlight_sample_queries;
/// Service implementation for `index_gaia_xp_continuous_bulk`.
pub mod index_gaia_xp_continuous_bulk;
/// Service implementation for `normalize_xp_continuous_coefficients`.
pub mod normalize_xp_continuous_coefficients;
/// Service implementation for `pack_starlight_asset`.
pub mod pack_starlight_asset;
/// Service implementation for `prepare_gaia_starlight_catalogue`.
pub mod prepare_gaia_starlight_catalogue;
/// Service implementation for `prepare_tycho_starlight_catalogue`.
pub mod prepare_tycho_starlight_catalogue;
/// Service implementation for `query_gaia_tap`.
pub mod query_gaia_tap;
/// Service implementation for `sweep_starlight_nside`.
pub mod sweep_starlight_nside;
/// Service implementation for `train_starlight_photometry_models`.
pub mod train_starlight_photometry_models;
/// Service implementation for `validate_starlight_map`.
pub mod validate_starlight_map;
/// Service implementation for `validate_xp_continuous_reconstruction`.
pub mod validate_xp_continuous_reconstruction;
/// Service implementation for `verify_assets`.
pub mod verify_assets;
