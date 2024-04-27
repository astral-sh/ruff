use std::fmt::Formatter;
use std::hash::Hash;
use std::sync::atomic::{AtomicUsize, Ordering};

use dashmap::mapref::entry::Entry;

use crate::FxDashMap;

/// Simple key value cache that locks on a per-key level.
pub struct KeyValueCache<K, V, S = DefaultStatisticsRecorder>
where
    S: StatisticsRecorder,
{
    map: FxDashMap<K, V>,
    statistics: S,
}

impl<K, V, S> KeyValueCache<K, V, S>
where
    K: Eq + Hash + Clone,
    V: Clone,
    S: StatisticsRecorder,
{
    pub fn try_get(&self, key: &K) -> Option<V> {
        if let Some(existing) = self.map.get(key) {
            self.statistics.hit();
            Some(existing.clone())
        } else {
            self.statistics.miss();
            None
        }
    }

    pub fn get<F>(&self, key: &K, compute: F) -> V
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

    pub fn set(&mut self, key: K, value: V) {
        self.map.insert(key, value);
    }

    pub fn remove(&mut self, key: &K) -> Option<V> {
        self.map.remove(key).map(|(_, value)| value)
    }

    pub fn clear(&mut self) {
        self.map.clear();
        self.map.shrink_to_fit();
    }

    pub fn statistics(&self) -> Option<Statistics> {
        self.statistics.to_statistics()
    }
}

impl<K, V, S> Default for KeyValueCache<K, V, S>
where
    K: Eq + Hash,
    V: Clone,
    S: StatisticsRecorder,
{
    fn default() -> Self {
        Self {
            map: FxDashMap::default(),
            statistics: S::default(),
        }
    }
}

impl<K, V, S> std::fmt::Debug for KeyValueCache<K, V, S>
where
    K: std::fmt::Debug + Eq + Hash,
    V: std::fmt::Debug,
    S: StatisticsRecorder,
{
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        let mut debug = f.debug_map();

        for entry in &self.map {
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
    #[allow(clippy::cast_precision_loss)]
    pub fn hit_rate(&self) -> Option<f64> {
        if self.hits + self.misses == 0 {
            return None;
        }

        Some((self.hits as f64) / (self.hits + self.misses) as f64)
    }
}

#[cfg(debug_assertions)]
type DefaultStatisticsRecorder = DebugStatistics;

#[cfg(not(debug_assertions))]
type DefaultStatisticsRecorder = ReleaseStatistics;

pub trait StatisticsRecorder: Default {
    fn hit(&self);
    fn miss(&self);
    fn to_statistics(&self) -> Option<Statistics>;
}

#[derive(Debug, Default)]
pub struct DebugStatistics {
    hits: AtomicUsize,
    misses: AtomicUsize,
}

impl StatisticsRecorder for DebugStatistics {
    // TODO figure out appropriate Ordering
    fn hit(&self) {
        self.hits.fetch_add(1, Ordering::SeqCst);
    }

    fn miss(&self) {
        self.misses.fetch_add(1, Ordering::SeqCst);
    }

    fn to_statistics(&self) -> Option<Statistics> {
        let hits = self.hits.load(Ordering::SeqCst);
        let misses = self.misses.load(Ordering::SeqCst);

        Some(Statistics { hits, misses })
    }
}

#[derive(Debug, Default)]
pub struct ReleaseStatistics;

impl StatisticsRecorder for ReleaseStatistics {
    #[inline]
    fn hit(&self) {}

    #[inline]
    fn miss(&self) {}

    #[inline]
    fn to_statistics(&self) -> Option<Statistics> {
        None
    }
}
