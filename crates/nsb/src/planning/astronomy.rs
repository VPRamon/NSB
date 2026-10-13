// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Vallés Puig, Ramon
//! Exact site-wide states shared by targets. No interpolation or time rounding.
use crate::components::moonlight::MoonlightSky;
use crate::{units::angular::Radians, Observer};
use siderust::bodies::Sun;
use std::{collections::HashMap, sync::RwLock};
use tempoch::{Time, JD, TT, UTC};

type Key = (u64, u64);
// Each context owns at most this many entries per selected component. Once
// full, new instants compute normally; eviction cannot affect the radiance.
const CAPACITY: usize = 100_000;
#[derive(Debug)]
struct Cache<T>(RwLock<HashMap<Key, T>>);
impl<T> Default for Cache<T> {
    fn default() -> Self {
        Self(RwLock::new(HashMap::new()))
    }
}
impl<T: Copy> Cache<T> {
    fn get(&self, time: Time<UTC>, compute: impl FnOnce() -> T) -> T {
        let (a, b) = time.to::<TT>().to::<JD>().raw_seconds_pair();
        let key = (a.value().to_bits(), b.value().to_bits());
        if let Ok(cache) = self.0.read() {
            if let Some(value) = cache.get(&key) {
                return *value;
            }
        }
        // Ephemerides run outside the lock; concurrent misses may duplicate
        // work but always compute the same authoritative state.
        let value = compute();
        // The cache is optional: a poisoned lock bypasses reuse, never physics.
        if let Ok(mut cache) = self.0.write() {
            if cache.len() < CAPACITY {
                cache.entry(key).or_insert(value);
            }
        }
        value
    }
}
#[derive(Debug)]
pub(crate) struct AstronomyCache {
    observer: Observer,
    time_data: tempoch::TimeDataStatus,
    solar: Cache<Radians>,
    moon: Cache<MoonlightSky>,
}
impl AstronomyCache {
    pub fn new(observer: Observer) -> Self {
        Self {
            observer,
            time_data: tempoch::time_data_status(),
            solar: Cache::default(),
            moon: Cache::default(),
        }
    }

    pub fn validate_time_data(&self) -> crate::Result<()> {
        if self.time_data != tempoch::time_data_status() {
            return Err(crate::NsbError::Unsupported("time-data bundle changed; prepare a new site window context; no partial schedule returned".into()));
        }
        Ok(())
    }

    pub fn solar(&self, time: Time<UTC>) -> Radians {
        self.solar.get(time, || {
            Sun::ecliptic_longitude_geocentric(time.to::<TT>().to::<JD>())
        })
    }
    pub fn moon(&self, time: Time<UTC>) -> MoonlightSky {
        self.moon
            .get(time, || MoonlightSky::at(time, self.observer))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use siderust::qtty::Seconds;

    #[test]
    fn keys_preserve_sub_microsecond_time_and_capacity_is_bounded() {
        let cache = Cache::default();
        let a = Time::<TT>::try_from_raw_j2000_seconds_split(
            Seconds::new(800_000_000.0),
            Seconds::new(0.0),
        )
        .unwrap()
        .to::<UTC>();
        let b = Time::<TT>::try_from_raw_j2000_seconds_split(
            Seconds::new(800_000_000.0),
            Seconds::new(1e-9),
        )
        .unwrap()
        .to::<UTC>();
        assert_eq!(cache.get(a, || 1), 1);
        assert_eq!(cache.get(b, || 2), 2);
        assert_eq!(cache.get(a, || panic!("exact state should be reused")), 1);
        for i in 0..=CAPACITY {
            cache.get(a + Seconds::new(i as f64 + 1.0), || 3);
        }
        assert_eq!(cache.0.read().unwrap().len(), CAPACITY);
        // Saturation computes normally; it never substitutes another instant.
        assert_eq!(cache.get(a + Seconds::new(CAPACITY as f64 + 2.0), || 4), 4);
    }
    #[test]
    fn poisoned_cache_bypasses_reuse_and_keeps_authoritative_computation() {
        let cache = Cache::default();
        let time = Time::<TT>::try_from_raw_j2000_seconds_split(
            siderust::qtty::Seconds::new(800_000_000.0),
            siderust::qtty::Seconds::new(0.0),
        )
        .unwrap()
        .to::<UTC>();
        assert_eq!(cache.get(time, || 1), 1);
        let _ = std::panic::catch_unwind(|| {
            let _guard = cache.0.write().unwrap();
            panic!("simulate a failed cache write");
        });
        assert_eq!(cache.get(time, || 2), 2);
        assert_eq!(cache.get(time, || 3), 3);
    }

    #[test]
    fn reused_context_rejects_a_changed_time_data_snapshot() {
        use crate::{ComponentMask, NsbError, NsbEvaluator, Target, ThresholdQuery, DEG};
        use tempoch::{ActiveTimeDataSource, Period};
        let evaluator = NsbEvaluator::new().unwrap();
        let start = Time::<UTC>::from_chrono(
            "2026-01-03T00:00:00Z"
                .parse::<chrono::DateTime<chrono::Utc>>()
                .unwrap(),
        );
        let observer = siderust::catalogs::observatories::EL_PARANAL.geodetic();
        let query = ThresholdQuery::new(
            observer,
            Target::new(83.6331 * DEG, 22.0145 * DEG),
            Period::new(start, start + Seconds::new(86400.0)),
            crate::units::radiometry::PhotonsPerSquareCentimeterNanosecondSteradian::new(0.25),
        )
        .with_components(ComponentMask::MOON);
        let mut context = evaluator.prepare_site_window_context(&query).unwrap();
        context.astronomy.moon(start);
        let snapshot = &mut std::sync::Arc::get_mut(&mut context.astronomy)
            .unwrap()
            .time_data;
        // Simulate the stale snapshot without mutating Tempoch's process-global
        // bundle and contaminating other parallel scientific tests.
        snapshot.source = if snapshot.source == ActiveTimeDataSource::Override {
            ActiveTimeDataSource::Bundled
        } else {
            ActiveTimeDataSource::Override
        };
        assert!(
            matches!(evaluator.periods_below_threshold_with_context(&context,&query),Err(NsbError::Unsupported(message)) if message.contains("time-data bundle changed"))
        );
        assert!(evaluator.periods_below_threshold(&query).is_ok());
    }
}
