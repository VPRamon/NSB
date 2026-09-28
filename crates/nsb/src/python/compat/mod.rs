//! Temporary upstream interoperability adapters.
//!
//! Nothing in this module is an NSB-owned scientific abstraction. The public
//! Python facade may replace these adapters with reusable Siderust/tempoch
//! bindings once their dependency stack matches NSB.

mod siderust;
mod tempoch;

pub(in crate::python) use siderust::{PyDirection, PyObserver};
pub(in crate::python) use tempoch::{datetime_to_time, period_to_datetimes, time_to_datetime};
