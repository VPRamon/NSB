//! Solar-activity (F10.7) acquisition and local store maintenance.
//!
//! Network access lives here only. The `nsb` runtime resolves against pinned
//! local/bundled stores and never fetches online data.

mod providers;
mod update;

pub use providers::{
    parse_45_day_forecast_json, parse_daily_solar_indices, parse_predicted_solar_cycle,
};
pub use update::{
    import_store, resolve_against_store, status_report, update_store, verify_store, UpdateMode,
    UpdateReport,
};
