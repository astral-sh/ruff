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
        ScratchSnapshot(self.len())
    }

    #[inline]
    fn take<C: FromIterator<T>>(&mut self, snapshot: ScratchSnapshot) -> C {
        self.drain(snapshot.0..).collect()
    }

    #[inline]
    fn take_thin_vec(&mut self, snapshot: ScratchSnapshot) -> ThinVec<T> {
        let len = self.len() - snapshot.0;
        if len == 0 {
            return ThinVec::new();
        }

        let mut result = ThinVec::with_capacity(len);
        result.extend(self.drain(snapshot.0..));
        result
    }
}

#[derive(Clone, Copy)]
pub(super) struct ScratchSnapshot(usize);
