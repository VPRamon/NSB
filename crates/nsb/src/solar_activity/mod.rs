//! Date-aware F10.7 solar-radio-flux resolution.
//!
//! Network acquisition belongs in `nsb-data-tools`. This module only loads
//! pinned/local stores and resolves values offline.

mod bundled;
mod monthly;
mod record;
mod resolve;
mod store;

pub use bundled::{bundled_f107_store, BUNDLED_F107_ASSET_PATH, BUNDLED_F107_RELATIVE_PATH};
pub use monthly::{
    days_in_month, is_finalized_monthly_observation, month_bounds_for, MonthlyCompleteness,
    MonthlyF107Evidence,
};
pub use record::{F107Kind, F107Record, F107ValidationError};
pub use resolve::{resolve_f107, utc_calendar_date, ResolvedSolarActivity, SolarActivitySource};
pub use store::{F107Store, F107StoreError, F107_STORE_SCHEMA_VERSION};

#[cfg(test)]
mod tests;
