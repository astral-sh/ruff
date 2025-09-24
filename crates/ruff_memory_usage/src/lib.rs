use std::sync::{LazyLock, Mutex};

use get_size2::{GetSize, StandardTracker};
use ordermap::{OrderMap, OrderSet};

/// Returns the memory usage of the provided object, using a global tracker to avoid
/// double-counting shared objects.
pub fn heap_size<T: GetSize>(value: &T) -> usize {
    static TRACKER: LazyLock<Mutex<StandardTracker>> =
        LazyLock::new(|| Mutex::new(StandardTracker::new()));

    value
        .get_heap_size_with_tracker(&mut *TRACKER.lock().unwrap())
        .0
}

/// An implementation of [`GetSize::get_heap_size`] for [`OrderSet`].
pub fn order_set_heap_size<T: GetSize, S>(set: &OrderSet<T, S>) -> usize {
    let size = set.iter().map(heap_size::<T>).sum::<usize>();
    size + (set.capacity() * T::get_stack_size())
}

/// An implementation of [`GetSize::get_heap_size`] for [`OrderSet`].
pub fn order_map_heap_size<K: GetSize, V: GetSize, S>(set: &OrderMap<K, V, S>) -> usize {
    let size = set
        .iter()
        .map(|(key, val)| heap_size::<K>(key) + heap_size::<V>(val))
        .sum::<usize>();

    size + (set.capacity() * <(K, V)>::get_stack_size())
}
