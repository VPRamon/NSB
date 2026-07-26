//! Shared implementation for NSB maintainer data-product tools.

// `gaia::acquisition::storage_preflight` needs `libc::statvfs` for USB/NVMe capacity gates.
#![deny(unsafe_code)]

extern crate self as nsb_data_tools;

pub mod cli;
pub mod gaia;
pub mod platform;
pub mod starlight;

use clap::Parser;
use std::cell::RefCell;
use std::ffi::OsString;

thread_local! {
    static COMMAND_ARGUMENTS: RefCell<Option<Vec<OsString>>> = const { RefCell::new(None) };
}

/// Compatibility aliases for the pre-`tools` module paths used by bulk binaries.
pub mod gaia_bulk {
    pub use crate::gaia::acquisition::bulk::*;
}
pub mod gaia_bulk_service {
    pub use crate::gaia::acquisition::bulk_service::*;
}
pub mod gaia_usb_cache {
    pub use crate::gaia::acquisition::usb_cache::*;
}
pub mod gaia_usb_cache_rotator {
    pub use crate::gaia::acquisition::usb_cache_rotator::*;
}
pub mod gaia_storage_preflight {
    pub use crate::gaia::acquisition::storage_preflight::*;
}
pub mod gaia_xp {
    pub use crate::gaia::xp::sampled::*;
}
pub mod gaia_xp_continuous {
    pub use crate::gaia::xp::continuous::*;
}
pub mod gaia_xp_continuous_calibrate {
    pub use crate::gaia::xp::calibrate::*;
}
pub mod gaia_xp_continuous_canonical {
    pub use crate::gaia::xp::canonical::*;
}
pub mod gaia_xp_continuous_bulk_index {
    pub use crate::gaia::xp::bulk_index::*;
}
pub mod gaia_xp_continuous_healpix {
    pub use crate::gaia::xp::healpix::*;
}
pub mod gaia_xp_continuous_pilot_io {
    pub use crate::gaia::xp::pilot_io::*;
}
pub mod gaia_xp_continuous_bulk_schema {
    pub use crate::gaia::xp::bulk_schema::*;
}
pub mod gaia_xp_continuous_bulk_healpix_merge {
    pub use crate::gaia::xp::bulk_healpix_merge::*;
}
pub mod gaia_xp_continuous_bulk_reconciliation {
    pub use crate::starlight::xp_continuous::bulk_reconciliation::*;
}
pub mod gaia_xp_continuous_tool_launch {
    pub use crate::starlight::acquisition::tool_launch::*;
}
pub mod checksum_io {
    pub use crate::platform::checksum_io::*;
}
pub mod artifact_io {
    pub use crate::platform::artifact_io::*;
}

/// Parse the arguments supplied by the hierarchical `nsb-data` command.
pub fn parse_command_args<T: Parser>() -> T {
    COMMAND_ARGUMENTS.with(|stored| match stored.borrow().as_ref() {
        Some(arguments) => T::parse_from(arguments),
        None => T::parse(),
    })
}

/// Run one action with its command-local argument vector.
pub fn with_command_args<T>(arguments: Vec<OsString>, run: impl FnOnce() -> T) -> T {
    COMMAND_ARGUMENTS.with(|stored| {
        assert!(stored.borrow().is_none(), "nested nsb-data action dispatch");
        *stored.borrow_mut() = Some(arguments);
        let result = run();
        *stored.borrow_mut() = None;
        result
    })
}
