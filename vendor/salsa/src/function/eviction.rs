//! Pluggable cache eviction strategies for memoized function values.
//!
//! This module provides the [`EvictionPolicy`] trait that allows different
//! eviction strategies to be used for salsa tracked functions.

mod lru;
mod noop;

pub use lru::Lru;
pub use noop::NoopEviction;

use crate::Id;

/// Trait for cache eviction strategies.
///
/// Implementations control when memoized values are evicted from the cache.
/// The eviction policy is selected at compile time via the `Configuration` trait.
pub trait EvictionPolicy: Send + Sync {
    /// Create a new eviction policy with the given capacity.
    fn new(capacity: usize) -> Self;

    /// Record that an item was accessed.
    fn record_use(&self, id: Id);

    /// Set the maximum capacity.
    fn set_capacity(&mut self, capacity: usize);

    /// Iterate over items that should be evicted.
    ///
    /// Called once per revision during `reset_for_new_revision`.
    /// The callback `cb` should be invoked for each item to evict.
    fn for_each_evicted(&mut self, cb: impl FnMut(Id));
}

/// Marker trait for eviction policies that have a configurable capacity.
///
/// This trait is used to conditionally generate the `set_lru_capacity` method
/// on tracked functions. Only policies that implement this trait will expose
/// runtime capacity configuration.
pub trait HasCapacity: EvictionPolicy {}
