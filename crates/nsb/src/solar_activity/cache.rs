use super::{bundled_f107_store, resolve_f107, F107Store, SolarActivitySource};
use crate::components::airglow::units::SolarFluxUnits;
use chrono::{DateTime, NaiveDate};
use std::collections::{HashMap, HashSet};
use std::sync::{Arc, RwLock};
use tempoch::{Time, UTC};

/// Query-local scalar view of F10.7 evidence.
///
/// Resolution is stable within a UTC date unless a forecast/retrieval record
/// becomes time-valid during that date. Those transition dates deliberately
/// bypass the cache; all other dates reuse the first fully resolved value.
#[derive(Debug, Clone)]
pub(crate) struct SolarActivityValueCache {
    source: SolarActivitySource,
    values: Arc<RwLock<HashMap<NaiveDate, SolarFluxUnits>>>,
    volatile_dates: HashSet<NaiveDate>,
}

impl SolarActivityValueCache {
    pub(crate) fn new(source: &SolarActivitySource) -> crate::Result<Self> {
        let volatile_dates = match source {
            SolarActivitySource::Explicit(_) => HashSet::new(),
            SolarActivitySource::Dataset(store) => transition_dates(store),
            SolarActivitySource::Automatic => transition_dates(bundled_f107_store()?),
        };
        Ok(Self {
            source: source.clone(),
            values: Arc::new(RwLock::new(HashMap::new())),
            volatile_dates,
        })
    }

    pub(crate) fn value_at(&self, time: Time<UTC>) -> crate::Result<SolarFluxUnits> {
        let date = time
            .to_chrono()
            .expect("F10.7 resolution requires a chrono-representable UTC instant")
            .date_naive();
        if !self.volatile_dates.contains(&date) {
            if let Some(value) = self.values.read().expect("solar cache lock poisoned").get(&date) {
                return Ok(*value);
            }
        }
        let value = resolve_f107(time, &self.source)?.value;
        if !self.volatile_dates.contains(&date) {
            self.values
                .write()
                .expect("solar cache lock poisoned")
                .insert(date, value);
        }
        Ok(value)
    }
}

fn transition_dates(store: &F107Store) -> HashSet<NaiveDate> {
    store
        .records
        .iter()
        .flat_map(|record| {
            [
                record.forecast_issued_at_utc.as_deref(),
                record.retrieved_at_utc.as_deref(),
            ]
        })
        .flatten()
        .filter_map(|value| DateTime::parse_from_rfc3339(value).ok())
        .map(|date_time| date_time.date_naive())
        .collect()
}
