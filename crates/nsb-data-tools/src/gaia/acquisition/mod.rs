//! Gaia transport and official bulk-acquisition primitives.

pub mod bulk;
#[deny(missing_docs)]
pub mod bulk_service;
pub mod datalink;
pub mod storage_preflight;
pub mod tap;
pub mod usb_cache;
pub mod usb_cache_rotator;
