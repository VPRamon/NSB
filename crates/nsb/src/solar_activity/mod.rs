//! Date-aware F10.7 solar-radio-flux resolution.
//!
//! Network acquisition belongs in `nsb-data-tools`. This module only loads
//! pinned/local stores and resolves values offline.

mod bundled;
mod monthly;
mod record;
mod resolve;
mod store;

pub use bundled::bundled_f107_store;
pub use monthly::{MonthlyCompleteness, MonthlyF107Evidence};
pub use record::{F107Kind, F107Record, F107ValidationError};
pub use resolve::{resolve_f107, ResolvedSolarActivity, SolarActivitySource};
pub use store::{F107Store, F107StoreError, F107_STORE_SCHEMA_VERSION};

#[cfg(test)]
mod tests;
