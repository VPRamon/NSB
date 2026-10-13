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
const CAPACITY: usize = 131_072;
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
        if let Some(value) = self.0.read().expect("astronomy cache lock").get(&key) {
            return *value;
        }
        // Ephemerides run outside the lock; concurrent misses may duplicate
        // work but always compute the same authoritative state.
        let value = compute();
        let mut cache = self.0.write().expect("astronomy cache lock");
        if cache.len() < CAPACITY {
            cache.entry(key).or_insert(value);
        }
        value
    }
}
#[derive(Debug, Default)]
pub(crate) struct AstronomyCache {
    solar: Cache<Radians>,
    moon: Cache<MoonlightSky>,
}
impl AstronomyCache {
    pub fn solar(&self, time: Time<UTC>) -> Radians {
        self.solar.get(time, || {
            Sun::ecliptic_longitude_geocentric(time.to::<TT>().to::<JD>())
        })
    }
    pub fn moon(&self, time: Time<UTC>, observer: Observer) -> MoonlightSky {
        self.moon.get(time, || MoonlightSky::at(time, observer))
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
}
