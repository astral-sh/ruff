//! Least Recently Used (LRU) eviction policy.
//!
//! This policy tracks the most recently accessed items and evicts
//! the least recently used ones when the cache exceeds its capacity.

use std::num::NonZeroUsize;

use crate::Id;
use crate::hash::FxLinkedHashSet;
use crate::sync::Mutex;

use super::{EvictionPolicy, HasCapacity};

/// Least Recently Used eviction policy.
///
/// When the number of memoized values exceeds the configured capacity,
/// the least recently accessed values are evicted at the start of each
/// new revision.
pub struct Lru {
    capacity: Option<NonZeroUsize>,
    set: Mutex<FxLinkedHashSet<Id>>,
}

impl Lru {
    #[inline(never)]
    fn insert(&self, id: Id) {
        self.set.lock().insert(id);
    }
}

impl EvictionPolicy for Lru {
    fn new(cap: usize) -> Self {
        Self {
            capacity: NonZeroUsize::new(cap),
            set: Mutex::default(),
        }
    }

    #[inline(always)]
    fn record_use(&self, id: Id) {
        if self.capacity.is_some() {
            self.insert(id);
        }
    }

    fn set_capacity(&mut self, capacity: usize) {
        self.capacity = NonZeroUsize::new(capacity);
        if self.capacity.is_none() {
            self.set.get_mut().clear();
        }
    }

    fn for_each_evicted(&mut self, mut cb: impl FnMut(Id)) {
        let Some(cap) = self.capacity else {
            return;
        };
        let set = self.set.get_mut();
        while set.len() > cap.get() {
            if let Some(id) = set.pop_front() {
                cb(id);
            }
        }
    }
}

impl HasCapacity for Lru {}
