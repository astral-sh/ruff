use std::cell::RefCell;

use get_size2::{GetSize, StandardTracker};
use ordermap::{OrderMap, OrderSet};

thread_local! {
    pub static TRACKER: RefCell<Option<StandardTracker>>= const { RefCell::new(None) };
}

struct TrackerGuard(Option<StandardTracker>);

impl Drop for TrackerGuard {
    fn drop(&mut self) {
        TRACKER.set(self.0.take());
    }
}

pub fn attach_tracker<R>(tracker: StandardTracker, f: impl FnOnce() -> R) -> R {
    let prev = TRACKER.replace(Some(tracker));
    let _guard = TrackerGuard(prev);
    f()
}

fn with_tracker<F, R>(f: F) -> R
where
    F: FnOnce(Option<&mut StandardTracker>) -> R,
{
    TRACKER.with(|tracker| {
        let mut tracker = tracker.borrow_mut();
        f(tracker.as_mut())
    })
}

/// Returns the memory usage of the provided object, using a global tracker to avoid
/// double-counting shared objects.
pub fn heap_size<T: GetSize>(value: &T) -> usize {
    with_tracker(|tracker| {
        if let Some(tracker) = tracker {
            value.get_heap_size_with_tracker(tracker).0
        } else {
            value.get_heap_size()
        }
    })
}

/// An implementation of [`GetSize::get_heap_size`] for [`OrderSet`].
pub fn order_set_heap_size<T: GetSize, S>(set: &OrderSet<T, S>) -> usize {
    (set.capacity() * T::get_stack_size()) + set.iter().map(heap_size).sum::<usize>()
}

/// An implementation of [`GetSize::get_heap_size`] for [`OrderMap`].
pub fn order_map_heap_size<K: GetSize, V: GetSize, S>(map: &OrderMap<K, V, S>) -> usize {
    (map.capacity() * (K::get_stack_size() + V::get_stack_size()))
        + (map.iter())
            .map(|(k, v)| heap_size(k) + heap_size(v))
            .sum::<usize>()
}
