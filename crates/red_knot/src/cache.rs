use std::fmt::Formatter;
use std::hash::Hash;
use std::sync::atomic::{AtomicUsize, Ordering};

use dashmap::mapref::entry::Entry;

use crate::FxDashMap;

pub trait Cache<K, V> {
    fn try_get(&self, key: &K) -> Option<V>;

    fn get<F>(&self, key: &K, compute: F) -> V
    where
        F: FnOnce(&K) -> V;

    fn set(&mut self, key: K, value: V);

    fn remove(&mut self, key: &K) -> Option<V>;

    fn clear(&mut self);

    fn statistics(&self) -> Option<Statistics>;
}

pub struct MapCache<K, V> {
    map: FxDashMap<K, V>,
    statistics: CacheStatistics,
}

impl<K, V> Cache<K, V> for MapCache<K, V>
where
    K: Eq + Hash + Clone,
    V: Clone,
{
    fn try_get(&self, key: &K) -> Option<V> {
        if let Some(existing) = self.map.get(key) {
            self.statistics.hit();
            Some(existing.clone())
        } else {
            self.statistics.hit();
            None
        }
    }

    fn get<F>(&self, key: &K, compute: F) -> V
    where
        F: FnOnce(&K) -> V,
    {
        match self.map.entry(key.clone()) {
            Entry::Occupied(cached) => {
                self.statistics.hit();

                cached.get().clone()
            }
            Entry::Vacant(vacant) => {
                self.statistics.miss();

                let value = compute(key);
                vacant.insert(value.clone());
                value
            }
        }
    }

    fn set(&mut self, key: K, value: V) {
        self.map.insert(key, value);
    }

    fn remove(&mut self, key: &K) -> Option<V> {
        self.map.remove(key).map(|(_, value)| value)
    }

    fn clear(&mut self) {
        self.map.clear()
    }

    fn statistics(&self) -> Option<Statistics> {
        self.statistics.to_statistics()
    }
}

impl<K, V> Default for MapCache<K, V>
where
    K: Eq + Hash,
    V: Clone,
{
    fn default() -> Self {
        Self {
            map: FxDashMap::default(),
            statistics: CacheStatistics::default(),
        }
    }
}

impl<K, V> std::fmt::Debug for MapCache<K, V>
where
    K: std::fmt::Debug + Eq + Hash,
    V: std::fmt::Debug,
{
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        let mut debug = f.debug_map();

        for entry in self.map.iter() {
            debug.entry(&entry.value(), &entry.key());
        }

        debug.finish()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Statistics {
    pub hits: usize,
    pub misses: usize,
}

impl Statistics {
    pub fn hit_rate(&self) -> Option<f64> {
        if self.hits + self.misses == 0 {
            return None;
        }

        Some((self.hits as f64) / (self.hits + self.misses) as f64)
    }
}

#[cfg(debug_assertions)]
pub type CacheStatistics = DebugStatistics;

#[cfg(not(debug_assertions))]
pub type CacheStatistics = ReleaseStatistics;

#[derive(Debug, Default)]
pub struct DebugStatistics {
    hits: AtomicUsize,
    misses: AtomicUsize,
}

impl DebugStatistics {
    // TODO figure out appropriate Ordering
    pub fn hit(&self) {
        self.hits.fetch_add(1, Ordering::SeqCst);
    }

    pub fn miss(&self) {
        self.misses.fetch_add(1, Ordering::SeqCst);
    }

    pub fn to_statistics(&self) -> Option<Statistics> {
        let hits = self.hits.load(Ordering::SeqCst);
        let misses = self.misses.load(Ordering::SeqCst);

        Some(Statistics { hits, misses })
    }
}

#[derive(Debug, Default)]
pub struct ReleaseStatistics;

impl ReleaseStatistics {
    #[inline(always)]
    pub fn to_statistics(&self) -> Option<Statistics> {
        None
    }
}
