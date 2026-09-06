//! Compatibility facade for site-calibration evidence.
//!
//! The canonical implementation lives in [`crate::site::calibration`]. This
//! module remains public so existing `nsb::site_calibration::*` consumers keep
//! compiling while site-owned code is organized under `site/`.

pub use crate::site::calibration::*;
