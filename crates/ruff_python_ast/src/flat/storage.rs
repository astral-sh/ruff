//! The row and range columns occupy disjoint regions of one allocation.

use std::alloc::{Layout, alloc, dealloc, handle_alloc_error};
use std::ptr::{self, NonNull};
use std::slice;

use ruff_text_size::TextRange;

use super::Row;

const BYTES_PER_SLOT: usize = size_of::<Row>() + size_of::<TextRange>();
const _: () = {
    assert!(align_of::<Row>() >= align_of::<TextRange>());
    assert!(BYTES_PER_SLOT == 24);
};

/// Both initialized prefixes have `len` elements. The ranges begin just past the
/// last reserved row, even when the row column has spare capacity.
#[derive(Debug)]
pub(super) struct NodeStorage {
    rows: NonNull<Row>,
    ranges: NonNull<TextRange>,
    len: usize,
    capacity: usize,
}

// SAFETY: Storage exclusively owns the allocation. Both element types are Send and Sync.
// Reads through shared storage are immutable, and every write requires exclusive access.
#[expect(unsafe_code)]
unsafe impl Send for NodeStorage {}
#[expect(unsafe_code)]
unsafe impl Sync for NodeStorage {}

impl Default for NodeStorage {
    fn default() -> Self {
        Self::empty()
    }
}

impl Clone for NodeStorage {
    fn clone(&self) -> Self {
        self.copy_to_capacity(self.len)
    }
}

impl Drop for NodeStorage {
    fn drop(&mut self) {
        if self.capacity != 0 {
            // SAFETY: This is the base and exact layout returned by with_capacity.
            // Row and TextRange are Copy and have no destructors.
            #[expect(unsafe_code)]
            unsafe {
                dealloc(self.rows.as_ptr().cast(), Self::layout(self.capacity));
            };
        }
    }
}

#[cfg(feature = "get-size")]
impl get_size2::GetSize for NodeStorage {
    fn get_heap_size_with_tracker<T: get_size2::GetSizeTracker>(&self, tracker: T) -> (usize, T) {
        (self.capacity * BYTES_PER_SLOT, tracker)
    }
}

impl NodeStorage {
    pub(super) const fn empty() -> Self {
        Self {
            rows: NonNull::dangling(),
            ranges: NonNull::dangling(),
            len: 0,
            capacity: 0,
        }
    }

    fn layout(capacity: usize) -> Layout {
        capacity
            .checked_mul(BYTES_PER_SLOT)
            .and_then(|bytes| Layout::from_size_align(bytes, align_of::<Row>()).ok())
            .expect("AST row storage exceeds the supported allocation size")
    }

    pub(super) fn with_capacity(capacity: usize) -> Self {
        if capacity == 0 {
            return Self::default();
        }
        let layout = Self::layout(capacity);
        // SAFETY: The layout has nonzero size and the maximum alignment of both columns.
        #[expect(unsafe_code)]
        let allocation = unsafe { alloc(layout) };
        let rows =
            NonNull::new(allocation.cast::<Row>()).unwrap_or_else(|| handle_alloc_error(layout));
        // SAFETY: The column starts at capacity * size_of::<Row>() bytes in this allocation.
        // Row alignment also satisfies TextRange alignment.
        #[expect(unsafe_code)]
        let ranges = unsafe { rows.add(capacity).cast::<TextRange>() };
        Self {
            rows,
            ranges,
            len: 0,
            capacity,
        }
    }

    #[inline]
    pub(super) fn len(&self) -> usize {
        self.len
    }

    pub(super) fn capacity(&self) -> usize {
        self.capacity
    }

    #[inline]
    pub(super) fn rows(&self) -> &[Row] {
        // SAFETY: Both columns have len initialized elements.
        #[expect(unsafe_code)]
        unsafe {
            slice::from_raw_parts(self.rows.as_ptr(), self.len)
        }
    }

    #[inline]
    pub(super) fn rows_mut(&mut self) -> &mut [Row] {
        // SAFETY: The exclusive borrow covers the initialized prefix of the row column.
        #[expect(unsafe_code)]
        unsafe {
            slice::from_raw_parts_mut(self.rows.as_ptr(), self.len)
        }
    }

    #[inline]
    pub(super) fn ranges(&self) -> &[TextRange] {
        // SAFETY: Both columns have len initialized elements.
        #[expect(unsafe_code)]
        unsafe {
            slice::from_raw_parts(self.ranges.as_ptr(), self.len)
        }
    }

    pub(super) fn rows_and_ranges_mut(&mut self) -> (&[Row], &mut [TextRange]) {
        // SAFETY: These initialized prefixes are in disjoint regions of the allocation.
        #[expect(unsafe_code)]
        unsafe {
            (
                slice::from_raw_parts(self.rows.as_ptr(), self.len),
                slice::from_raw_parts_mut(self.ranges.as_ptr(), self.len),
            )
        }
    }

    fn copy_to_capacity(&self, capacity: usize) -> Self {
        assert!(capacity >= self.len);
        let mut new = Self::with_capacity(capacity);
        if self.len > 0 {
            // SAFETY: Both allocations hold len elements of each Copy type. The ranges
            // are copied separately because growth moves their base relative to the rows.
            #[expect(unsafe_code)]
            unsafe {
                ptr::copy_nonoverlapping(self.rows.as_ptr(), new.rows.as_ptr(), self.len);
                ptr::copy_nonoverlapping(self.ranges.as_ptr(), new.ranges.as_ptr(), self.len);
            }
        }
        new.len = self.len;
        new
    }

    pub(super) fn reserve_exact(&mut self, additional: usize) {
        let capacity = self
            .len
            .checked_add(additional)
            .expect("AST row capacity overflow");
        if capacity > self.capacity {
            *self = self.copy_to_capacity(capacity);
        }
    }

    /// Returns false when the builder must reserve space using its source-density estimate.
    #[inline]
    pub(super) fn try_push(&mut self, row: Row, range: TextRange) -> bool {
        if self.len == self.capacity {
            return false;
        }
        // SAFETY: There is one spare slot in both columns. Publish the new length
        // after both writes so that every visible row has its matching range.
        #[expect(unsafe_code)]
        unsafe {
            self.rows.as_ptr().add(self.len).write(row);
            self.ranges.as_ptr().add(self.len).write(range);
        }
        self.len += 1;
        true
    }

    pub(super) fn truncate(&mut self, len: usize) {
        self.len = self.len.min(len);
    }

    pub(super) fn shrink_to_fit(&mut self) {
        if self.capacity != self.len {
            *self = self.copy_to_capacity(self.len);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{NodeStorage, Row, TextRange};
    use crate::NodeKind;
    #[cfg(feature = "get-size")]
    use get_size2::GetSize;

    fn row(id: u32) -> Row {
        let mut row = Row::new(NodeKind::Identifier);
        row.set_identifier_id(id);
        row
    }

    fn span(id: u32) -> TextRange {
        TextRange::at(id.into(), 2.into())
    }

    fn assert_storage(storage: &NodeStorage, expected: &[u32]) {
        assert_eq!(storage.len(), expected.len());
        assert_eq!(storage.rows().len(), expected.len());
        assert_eq!(storage.ranges().len(), expected.len());
        let base = storage.rows().as_ptr().addr();
        let ranges = storage.ranges().as_ptr().addr();
        assert_eq!(base % align_of::<Row>(), 0);
        assert_eq!(ranges % align_of::<TextRange>(), 0);
        if storage.capacity() > 0 {
            assert_eq!(ranges - base, storage.capacity() * size_of::<Row>());
        }
        #[cfg(feature = "get-size")]
        assert_eq!(storage.get_heap_size(), storage.capacity() * 24);
        for ((row, &range), &value) in storage.rows().iter().zip(storage.ranges()).zip(expected) {
            assert_eq!(row.kind(), NodeKind::Identifier);
            assert_eq!(row.identifier_id(), value);
            assert_eq!(range, span(value));
        }
    }

    #[test]
    fn shared_range_storage_lifecycle() {
        fn copy_send_sync<T: Copy + Send + Sync>() {}
        fn send_sync<T: Send + Sync>() {}
        copy_send_sync::<Row>();
        copy_send_sync::<TextRange>();
        send_sync::<NodeStorage>();
        let mut nodes = NodeStorage::default();
        let mut values = Vec::new();
        assert_storage(&nodes, &values);
        assert!(!nodes.try_push(row(99), span(99)));
        nodes.shrink_to_fit();
        assert_storage(&nodes.clone(), &[]);
        for id in 1..=64 {
            if !nodes.try_push(row(id), span(id)) {
                nodes.reserve_exact(7);
                assert!(nodes.try_push(row(id), span(id)));
            }
            values.push(id);
            assert_storage(&nodes, &values);
        }
        nodes.reserve_exact(113);
        assert_storage(&nodes, &values);
        nodes.truncate(13);
        values.truncate(13);
        for id in 65..=80 {
            assert!(nodes.try_push(row(id), span(id)));
            values.push(id);
        }
        nodes.truncate(usize::MAX);
        nodes.rows_mut()[0].set_identifier_id(170);
        let (rows, ranges) = nodes.rows_and_ranges_mut();
        ranges[0] = span(rows[0].identifier_id());
        values[0] = 170;
        assert_storage(&nodes, &values);
        let cloned = nodes.clone();
        assert_storage(&cloned, &values);
        assert_ne!(nodes.rows().as_ptr(), cloned.rows().as_ptr());
        nodes.shrink_to_fit();
        assert_eq!(nodes.capacity(), values.len());
        assert_storage(&nodes, &values);
        nodes.truncate(0);
        nodes.shrink_to_fit();
        assert_eq!(nodes.capacity(), 0);
        assert_storage(&nodes, &[]);
        assert_storage(&cloned, &values);
    }

    #[test]
    fn shared_range_storage_overflow_preserves_both_prefixes() {
        let mut nodes = NodeStorage::with_capacity(1);
        assert!(nodes.try_push(row(3), span(3)));
        assert!(
            std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                nodes.reserve_exact(usize::MAX);
            }))
            .is_err()
        );
        assert_storage(&nodes, &[3]);
        assert!(
            std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                nodes.reserve_exact(usize::MAX / 2);
            }))
            .is_err()
        );
        assert_storage(&nodes, &[3]);
    }
}
