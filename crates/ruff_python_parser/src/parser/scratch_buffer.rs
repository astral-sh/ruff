use std::mem::MaybeUninit;

use drop_bomb::DebugDropBomb;
use thin_vec::ThinVec;

/// Reusable scratch storage that preserves entries belonging to outer parser frames.
#[derive(Debug)]
pub(super) struct ScratchBuffer<T> {
    buffer: Vec<T>,
}

impl<T> ScratchBuffer<T> {
    pub(super) fn new() -> Self {
        Self { buffer: Vec::new() }
    }

    pub(super) fn with_capacity(capacity: usize) -> Self {
        Self {
            buffer: Vec::with_capacity(capacity),
        }
    }

    #[inline]
    pub(super) fn push(&mut self, value: T) {
        self.buffer.push(value);
    }

    #[inline]
    pub(super) fn is_empty(&self, snapshot: &ScratchSnapshot) -> bool {
        debug_assert!(
            self.buffer.len() >= snapshot.len,
            "Scratch buffer snapshots must be restored in reverse order of creation."
        );
        self.buffer.len() == snapshot.len
    }

    #[inline]
    pub(super) fn snapshot(&self) -> ScratchSnapshot {
        ScratchSnapshot {
            len: self.buffer.len(),
            bomb: DebugDropBomb::new("Scratch buffer snapshots must be restored."),
        }
    }

    #[inline]
    pub(super) fn take<C: From<Vec<T>>>(&mut self, mut snapshot: ScratchSnapshot) -> C {
        debug_assert!(
            self.buffer.len() >= snapshot.len,
            "Scratch buffer snapshots must be restored in reverse order of creation."
        );
        snapshot.bomb.defuse();
        self.buffer.split_off(snapshot.len).into()
    }

    #[inline]
    pub(super) fn take_thin_vec(&mut self, mut snapshot: ScratchSnapshot) -> ThinVec<T> {
        if self.is_empty(&snapshot) {
            snapshot.bomb.defuse();
            return ThinVec::new();
        }

        snapshot.bomb.defuse();
        let source = &self.buffer[snapshot.len..];
        let len = source.len();
        let mut result = ThinVec::<MaybeUninit<T>>::with_capacity(len);

        // SAFETY: `MaybeUninit<T>` permits uninitialized elements. Setting the length
        // first gives the slice returned by `DerefMut` access to all `len` slots.
        // `source` is initialized, and the allocations are disjoint (or the copy is
        // empty for zero-sized elements). Shortening the source transfers ownership.
        // `ThinVec` is `repr(C)` over a header pointer and `PhantomData`, and
        // `MaybeUninit<T>` has T's size and alignment. All copied entries are now
        // initialized, so converting the owner preserves its allocation and layout.
        #[expect(
            unsafe_code,
            reason = "moves initialized AST entries without per-element iteration"
        )]
        unsafe {
            result.set_len(len);
            std::ptr::copy_nonoverlapping(source.as_ptr(), result.as_mut_ptr().cast(), len);
            self.buffer.set_len(snapshot.len);
            std::mem::transmute::<ThinVec<MaybeUninit<T>>, ThinVec<T>>(result)
        }
    }
}

impl<T> Drop for ScratchBuffer<T> {
    fn drop(&mut self) {
        debug_assert!(
            self.buffer.is_empty() || std::thread::panicking(),
            "Scratch buffers must be empty when dropped."
        );
    }
}

pub(super) struct ScratchSnapshot {
    len: usize,
    bomb: DebugDropBomb,
}

#[cfg(test)]
mod tests {
    use super::ScratchBuffer;

    #[test]
    #[cfg(debug_assertions)]
    #[should_panic(expected = "Scratch buffers must be empty when dropped.")]
    fn buffer_must_be_empty_when_dropped() {
        let mut buffer = ScratchBuffer::new();
        buffer.push(1);
    }

    #[test]
    #[cfg(debug_assertions)]
    #[should_panic(expected = "Scratch buffer snapshots must be restored.")]
    fn snapshot_must_be_restored() {
        let buffer = ScratchBuffer::<u8>::new();
        let _snapshot = buffer.snapshot();
    }

    #[test]
    #[cfg(debug_assertions)]
    #[should_panic(
        expected = "Scratch buffer snapshots must be restored in reverse order of creation."
    )]
    fn snapshot_must_be_restored_in_reverse_order() {
        let mut buffer = ScratchBuffer::new();
        buffer.push(1);
        let snapshot = buffer.snapshot();
        buffer.buffer.clear();

        let _: Vec<_> = buffer.take(snapshot);
    }

    #[test]
    fn snapshot_is_empty_relative_to_its_buffer() {
        let mut buffer = ScratchBuffer::new();
        let outer_snapshot = buffer.snapshot();
        buffer.push(1);
        let snapshot = buffer.snapshot();
        assert!(buffer.is_empty(&snapshot));

        buffer.push(2);
        assert!(!buffer.is_empty(&snapshot));

        let _: Vec<_> = buffer.take(snapshot);
        let _: Vec<_> = buffer.take(outer_snapshot);
    }

    #[test]
    fn nested_values_are_dropped_once() {
        use std::cell::Cell;
        use std::rc::Rc;

        struct Value {
            text: String,
            boxed: Box<u8>,
            drops: Rc<Cell<usize>>,
        }

        impl Drop for Value {
            fn drop(&mut self) {
                self.drops.set(self.drops.get() + 1);
            }
        }

        let drops = Rc::new(Cell::new(0));
        let value = |text: &str| Value {
            text: text.to_owned(),
            boxed: Box::new(7),
            drops: Rc::clone(&drops),
        };
        let mut buffer = ScratchBuffer::new();
        let outer = buffer.snapshot();
        buffer.push(value("outer"));
        let inner = buffer.snapshot();
        buffer.push(value("first"));
        buffer.push(value("second"));
        let capacity = buffer.buffer.capacity();

        let inner = buffer.take_thin_vec(inner);
        assert_eq!(inner[0].text, "first");
        assert_eq!(inner[1].text, "second");
        assert_eq!(*inner[0].boxed, 7);
        let outer: Vec<_> = buffer.take(outer);
        assert_eq!(outer[0].text, "outer");
        assert_eq!(buffer.buffer.capacity(), capacity);
        assert_eq!(drops.get(), 0);
        drop(inner);
        assert_eq!(drops.get(), 2);
        drop(outer);
        assert_eq!(drops.get(), 3);
    }

    #[test]
    fn zero_sized_values_and_empty_snapshots() {
        use std::sync::atomic::{AtomicUsize, Ordering};

        static DROPS: AtomicUsize = AtomicUsize::new(0);
        struct Value;
        impl Drop for Value {
            fn drop(&mut self) {
                DROPS.fetch_add(1, Ordering::Relaxed);
            }
        }

        DROPS.store(0, Ordering::Relaxed);
        let mut buffer = ScratchBuffer::new();
        let snapshot = buffer.snapshot();
        assert!(buffer.take_thin_vec(snapshot).is_empty());
        let snapshot = buffer.snapshot();
        buffer.push(Value);
        buffer.push(Value);
        let values = buffer.take_thin_vec(snapshot);
        assert_eq!(values.len(), 2);
        drop(values);
        drop(buffer);
        assert_eq!(DROPS.load(Ordering::Relaxed), 2);
    }

    #[test]
    #[cfg_attr(
        debug_assertions,
        should_panic(
            expected = "Scratch buffer snapshots must be restored in reverse order of creation."
        )
    )]
    #[cfg_attr(not(debug_assertions), should_panic(expected = "range start index"))]
    fn invalid_snapshot_panics_before_copying() {
        let mut buffer = ScratchBuffer::<String>::new();
        let mut snapshot = buffer.snapshot();
        snapshot.len = 1;
        let _ = buffer.take_thin_vec(snapshot);
    }
}
