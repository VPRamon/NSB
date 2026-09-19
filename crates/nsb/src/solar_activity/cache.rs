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
            if let Some(value) = self
                .values
                .read()
                .expect("solar cache lock poisoned")
                .get(&date)
            {
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
        .map(|date_time| date_time.with_timezone(&chrono::Utc).date_naive())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{Duration, TimeZone, Utc};

    #[test]
    fn transition_dates_are_normalized_to_utc() {
        let mut store = bundled_f107_store().unwrap().clone();
        let mut record = store
            .records
            .first()
            .expect("bundled F10.7 store must contain records")
            .clone();
        record.forecast_issued_at_utc = None;
        record.retrieved_at_utc = Some("2026-01-01T23:30:00-02:00".into());
        store.records = vec![record];

        let transitions = transition_dates(&store);
        let local_offset_date = NaiveDate::from_ymd_opt(2026, 1, 1).unwrap();
        let utc_date = NaiveDate::from_ymd_opt(2026, 1, 2).unwrap();

        assert!(!transitions.contains(&local_offset_date));
        assert!(transitions.contains(&utc_date));
    }

    #[test]
    fn cached_values_match_full_resolution_across_stable_and_transition_dates() {
        let source = SolarActivitySource::Automatic;
        let cache = SolarActivityValueCache::new(&source).unwrap();
        let transitions = transition_dates(bundled_f107_store().unwrap());
        assert!(!transitions.is_empty());
        let start = Utc.with_ymd_and_hms(2026, 1, 1, 0, 0, 0).single().unwrap();
        let mut checked_transition = false;
        let mut checked_stable = false;

        for hour in (0..365 * 24).step_by(6) {
            let time = Time::<UTC>::from_chrono(start + Duration::hours(hour));
            let date = time.to_chrono().unwrap().date_naive();
            let cached = cache.value_at(time).unwrap();
            let exact = resolve_f107(time, &source).unwrap().value;
            assert_eq!(cached, exact, "cache changed F10.7 at {date}");
            checked_transition |= transitions.contains(&date);
            checked_stable |= !transitions.contains(&date);
        }

        assert!(checked_transition);
        assert!(checked_stable);
    }
}
