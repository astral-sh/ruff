use drop_bomb::DebugDropBomb;

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
    pub(super) fn take_range(
        &mut self,
        snapshot: ScratchSnapshot,
        storage: &mut super::ParserStorage,
    ) -> ruff_python_ast::flat::ListId<T::Element>
    where
        T: ruff_python_ast::flat::ListItem + Copy,
    {
        self.take(snapshot, |items| storage.list(items.iter().copied()))
    }

    #[inline]
    pub(super) fn take<R>(
        &mut self,
        mut snapshot: ScratchSnapshot,
        read: impl FnOnce(&[T]) -> R,
    ) -> R {
        debug_assert!(
            self.buffer.len() >= snapshot.len,
            "Scratch buffer snapshots must be restored in reverse order of creation."
        );
        snapshot.bomb.defuse();
        let result = read(&self.buffer[snapshot.len..]);
        self.buffer.truncate(snapshot.len);
        result
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

        buffer.take(snapshot, |_| ());
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

        buffer.take(snapshot, |_| ());
        buffer.take(outer_snapshot, |_| ());
    }
}
