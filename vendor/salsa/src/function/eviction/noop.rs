//! No-op eviction policy - cache grows unbounded.
//!
//! This is the default eviction policy when no LRU capacity is specified.

use crate::{Id, function::EvictionPolicy};

/// No eviction - cache grows unbounded.
pub struct NoopEviction;

impl EvictionPolicy for NoopEviction {
    fn new(_cap: usize) -> Self {
        Self
    }

    #[inline(always)]
    fn record_use(&self, _id: Id) {}

    #[inline(always)]
    fn set_capacity(&mut self, _capacity: usize) {}

    #[inline(always)]
    fn for_each_evicted(&mut self, _cb: impl FnMut(Id)) {}
}
