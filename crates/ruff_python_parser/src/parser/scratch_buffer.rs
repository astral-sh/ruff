use std::vec::Drain;

use drop_bomb::DebugDropBomb;
use thin_vec::ThinVec;

/// Operations for reusable scratch storage that preserve entries belonging to outer parser frames.
pub(super) trait ScratchBufferExt<T> {
    fn snapshot(&self) -> ScratchSnapshot;

    fn take<C: FromIterator<T>>(&mut self, snapshot: ScratchSnapshot) -> C;

    fn take_thin_vec(&mut self, snapshot: ScratchSnapshot) -> ThinVec<T>;

    fn drain_snapshot(&mut self, snapshot: ScratchSnapshot) -> Drain<'_, T>;
}

impl<T> ScratchBufferExt<T> for Vec<T> {
    #[inline]
    fn snapshot(&self) -> ScratchSnapshot {
        ScratchSnapshot {
            len: self.len(),
            bomb: DebugDropBomb::new("Scratch buffer snapshots must be restored."),
        }
    }

    #[inline]
    fn take<C: FromIterator<T>>(&mut self, snapshot: ScratchSnapshot) -> C {
        self.drain_snapshot(snapshot).collect()
    }

    #[inline]
    fn take_thin_vec(&mut self, mut snapshot: ScratchSnapshot) -> ThinVec<T> {
        if snapshot.is_empty(self) {
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
            self.len() >= snapshot.len,
            "Scratch buffer snapshots must be restored in reverse order of creation."
        );
        snapshot.bomb.defuse();
        self.drain(snapshot.len..)
    }
}

pub(super) struct ScratchSnapshot {
    len: usize,
    bomb: DebugDropBomb,
}

impl ScratchSnapshot {
    pub(super) fn is_empty<T>(&self, buffer: &[T]) -> bool {
        debug_assert!(
            buffer.len() >= self.len,
            "Scratch buffer snapshots must be restored in reverse order of creation."
        );
        buffer.len() == self.len
    }
}

#[cfg(test)]
mod tests {
    use super::ScratchBufferExt;

    #[test]
    #[cfg(debug_assertions)]
    #[should_panic(expected = "Scratch buffer snapshots must be restored.")]
    fn snapshot_must_be_restored() {
        let buffer = Vec::<u8>::new();
        let _snapshot = buffer.snapshot();
    }

    #[test]
    #[cfg(debug_assertions)]
    #[should_panic(
        expected = "Scratch buffer snapshots must be restored in reverse order of creation."
    )]
    fn snapshot_must_be_restored_in_reverse_order() {
        let mut buffer = vec![1];
        let snapshot = buffer.snapshot();
        buffer.clear();

        let _: Vec<_> = buffer.take(snapshot);
    }

    #[test]
    fn snapshot_is_empty_relative_to_its_buffer() {
        let mut buffer = vec![1];
        let snapshot = buffer.snapshot();
        assert!(snapshot.is_empty(&buffer));

        buffer.push(2);
        assert!(!snapshot.is_empty(&buffer));

        let _: Vec<_> = buffer.take(snapshot);
    }
}
