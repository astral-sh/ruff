use drop_bomb::DebugDropBomb;
use thin_vec::ThinVec;

/// Operations for reusable scratch storage that preserve entries belonging to outer parser frames.
pub(super) trait ScratchBufferExt<T> {
    fn snapshot(&self) -> ScratchSnapshot;

    fn take<C: FromIterator<T>>(&mut self, snapshot: ScratchSnapshot) -> C;

    fn take_thin_vec(&mut self, snapshot: ScratchSnapshot) -> ThinVec<T>;
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
        let start = snapshot.restore(self.len());
        self.drain(start..).collect()
    }

    #[inline]
    fn take_thin_vec(&mut self, snapshot: ScratchSnapshot) -> ThinVec<T> {
        let start = snapshot.restore(self.len());
        let len = self.len() - start;
        if len == 0 {
            return ThinVec::new();
        }

        let mut result = ThinVec::with_capacity(len);
        result.extend(self.drain(start..));
        result
    }
}

pub(super) struct ScratchSnapshot {
    len: usize,
    bomb: DebugDropBomb,
}

impl ScratchSnapshot {
    fn restore(mut self, buffer_len: usize) -> usize {
        debug_assert!(
            buffer_len >= self.len,
            "Scratch buffers must not shrink before a snapshot is restored."
        );
        self.bomb.defuse();
        self.len
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
    #[should_panic(expected = "Scratch buffers must not shrink before a snapshot is restored.")]
    fn buffer_must_not_shrink_before_restore() {
        let mut buffer = vec![1];
        let snapshot = buffer.snapshot();
        buffer.clear();

        let _: Vec<_> = buffer.take(snapshot);
    }
}
