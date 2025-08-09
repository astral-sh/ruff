use std::sync::{LazyLock, Mutex};

use get_size2::{GetSize, StandardTracker};

/// Returns the memory usage of the provided object, using a global tracker to avoid
/// double-counting shared objects.
pub fn heap_size<T: GetSize>(value: &T) -> usize {
    static TRACKER: LazyLock<Mutex<StandardTracker>> =
        LazyLock::new(|| Mutex::new(StandardTracker::new()));

    value
        .get_heap_size_with_tracker(&mut *TRACKER.lock().unwrap())
        .0
}
