//! Shared implementation for NSB maintainer data-product tools.

// `gaia_storage_preflight` needs `libc::statvfs` for USB/NVMe capacity gates.
#![deny(unsafe_code)]

extern crate self as nsb_data_tools;

pub mod artifact_io;
pub mod checksum_io;
pub mod gaia_bulk;
#[deny(missing_docs)]
pub mod gaia_bulk_service;
pub mod gaia_datalink;
#[allow(unsafe_code)]
pub mod gaia_storage_preflight;
pub mod gaia_tap;
pub mod gaia_usb_cache;
pub mod gaia_usb_cache_rotator;
pub mod gaia_xp;
pub mod gaia_xp_continuous;
pub mod gaia_xp_continuous_bulk_healpix_merge;
pub mod gaia_xp_continuous_bulk_index;
pub mod gaia_xp_continuous_bulk_reconciliation;
pub mod gaia_xp_continuous_bulk_schema;
pub mod gaia_xp_continuous_calibrate;
pub mod gaia_xp_continuous_canonical;
pub mod gaia_xp_continuous_healpix;
pub mod gaia_xp_continuous_pilot_io;
pub mod gaia_xp_continuous_tool_launch;
#[deny(missing_docs)]
pub mod pipeline;
pub mod provenance;
pub mod scientific_contract;
pub mod starlight_approval;
pub mod starlight_integrated;
pub mod starlight_phase5;
pub mod starlight_phase5_holdout;
pub mod starlight_phase5_uncertainty;
pub mod starlight_sampling;
pub mod starlight_science;
#[deny(missing_docs)]
pub mod tool_logging;
pub mod tool_services;
