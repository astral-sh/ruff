use std::vec::Drain;

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

    pub(super) fn is_empty(&self) -> bool {
        self.buffer.is_empty()
    }

    #[inline]
    pub(super) fn is_empty_since(&self, snapshot: &ScratchSnapshot) -> bool {
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
    pub(super) fn take<C: FromIterator<T>>(&mut self, snapshot: ScratchSnapshot) -> C {
        self.drain_snapshot(snapshot).collect()
    }

    #[inline]
    pub(super) fn take_thin_vec(&mut self, mut snapshot: ScratchSnapshot) -> ThinVec<T> {
        if self.is_empty_since(&snapshot) {
            snapshot.bomb.defuse();
            return ThinVec::new();
        }

        let drain = self.drain_snapshot(snapshot);
        let mut result = ThinVec::with_capacity(drain.len());
        result.extend(drain);
        result
    }

    #[inline]
    fn drain_snapshot(&mut self, mut snapshot: ScratchSnapshot) -> Drain<'_, T> {
        debug_assert!(
            self.buffer.len() >= snapshot.len,
            "Scratch buffer snapshots must be restored in reverse order of creation."
        );
        snapshot.bomb.defuse();
        self.buffer.drain(snapshot.len..)
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
        buffer.push(1);
        let snapshot = buffer.snapshot();
        assert!(buffer.is_empty_since(&snapshot));

        buffer.push(2);
        assert!(!buffer.is_empty_since(&snapshot));

        let _: Vec<_> = buffer.take(snapshot);
    }
}
