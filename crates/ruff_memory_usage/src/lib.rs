use std::cell::RefCell;

use get_size2::{GetSize, StandardTracker};

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
