//! Snapshot management for TSP.
//!
//! The snapshot is a monotonically increasing number that represents the
//! "type world state". It is incremented whenever any type information
//! could become stale (document changes, config changes, etc.).

use std::sync::atomic::{AtomicU64, Ordering};

/// Manages the snapshot state for TSP requests.
///
/// The snapshot is used to track the validity of type information.
/// When a client makes a TSP request, it includes the snapshot number
/// to ensure the response is based on consistent state.
#[derive(Debug)]
pub struct SnapshotManager {
    /// The current snapshot number.
    current: AtomicU64,
}

impl Default for SnapshotManager {
    fn default() -> Self {
        Self::new()
    }
}

impl SnapshotManager {
    /// Create a new snapshot manager with an initial snapshot of 1.
    pub fn new() -> Self {
        Self {
            current: AtomicU64::new(1),
        }
    }

    /// Get the current snapshot number.
    pub fn current(&self) -> u64 {
        self.current.load(Ordering::SeqCst)
    }

    /// Increment the snapshot and return the new value.
    ///
    /// This should be called whenever type information could become stale:
    /// - Document opened/changed/closed
    /// - Configuration changed
    /// - Workspace folders changed
    pub fn increment(&self) -> u64 {
        self.current.fetch_add(1, Ordering::SeqCst) + 1
    }

    /// Check if a snapshot is still valid (matches the current snapshot).
    pub fn is_valid(&self, snapshot: u64) -> bool {
        self.current() == snapshot
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_initial_snapshot() {
        let manager = SnapshotManager::new();
        assert_eq!(manager.current(), 1);
    }

    #[test]
    fn test_increment() {
        let manager = SnapshotManager::new();
        assert_eq!(manager.current(), 1);
        assert_eq!(manager.increment(), 2);
        assert_eq!(manager.current(), 2);
        assert_eq!(manager.increment(), 3);
        assert_eq!(manager.current(), 3);
    }

    #[test]
    fn test_is_valid() {
        let manager = SnapshotManager::new();
        assert!(manager.is_valid(1));
        assert!(!manager.is_valid(2));

        manager.increment();
        assert!(!manager.is_valid(1));
        assert!(manager.is_valid(2));
    }
}
