//! Handler for `typeServer/getSnapshot`.

use crate::SnapshotManager;

/// Handle the `typeServer/getSnapshot` request.
///
/// Returns the current snapshot number as a raw number (not wrapped in an object).
/// The protocol expects `number`, not `{ snapshot: number }`.
pub(crate) fn handle_get_snapshot(
    snapshot_manager: &SnapshotManager,
) -> Result<serde_json::Value, String> {
    // Return the snapshot as a plain number
    let snapshot = snapshot_manager.current();
    serde_json::to_value(snapshot).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_handle_get_snapshot() {
        let manager = SnapshotManager::new();
        let result = handle_get_snapshot(&manager).unwrap();
        let parsed: u64 = serde_json::from_value(result).unwrap();

        assert_eq!(parsed, 1);
    }

    #[test]
    fn test_handle_get_snapshot_after_increment() {
        let manager = SnapshotManager::new();
        manager.increment();
        manager.increment();

        let result = handle_get_snapshot(&manager).unwrap();
        let parsed: u64 = serde_json::from_value(result).unwrap();

        assert_eq!(parsed, 3);
    }

    #[test]
    fn test_snapshot_is_number() {
        let manager = SnapshotManager::new();
        let result = handle_get_snapshot(&manager).unwrap();
        assert!(result.is_number(), "Snapshot should be a raw number");
    }
}
