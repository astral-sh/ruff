use std::hash::Hash;
use std::iter::ExactSizeIterator;
use std::mem::size_of;
use std::ops::Deref;

use get_size2::{GetSize, GetSizeTracker};
use salsa::Update;
use triomphe::{Arc, ThinArc};

/// An atomically reference-counted pointer for values that never need weak references.
///
/// This wrapper lets ty use [`triomphe::Arc`] while providing the foreign trait implementations
/// required by Salsa and ty's memory reporting.
#[derive(Debug, Eq, Hash, PartialEq)]
#[repr(transparent)]
pub struct NoWeakArc<T: ?Sized>(Arc<T>);

impl<T: ?Sized> Clone for NoWeakArc<T> {
    fn clone(&self) -> Self {
        Self(self.0.clone())
    }
}

impl<T> NoWeakArc<T> {
    pub fn new(value: T) -> Self {
        Self(Arc::new(value))
    }
}

impl<T: ?Sized> NoWeakArc<T> {
    pub fn ptr_eq(this: &Self, other: &Self) -> bool {
        Arc::ptr_eq(&this.0, &other.0)
    }
}

impl<T: ?Sized> Deref for NoWeakArc<T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<T> GetSize for NoWeakArc<T>
where
    T: GetSize,
{
    fn get_heap_size_with_tracker<Tracker: GetSizeTracker>(
        &self,
        mut tracker: Tracker,
    ) -> (usize, Tracker) {
        if tracker.track(self.0.heap_ptr()) {
            T::get_size_with_tracker(self, tracker)
        } else {
            (0, tracker)
        }
    }
}

// SAFETY: This has the same update behavior as Salsa's implementation for `std::sync::Arc`.
// It only updates the pointee in place when both allocations are uniquely owned; otherwise it
// replaces the old pointer with the new one.
#[expect(
    unsafe_code,
    reason = "implementing Salsa's unsafe Update contract requires dereferencing its raw pointer"
)]
unsafe impl<T> Update for NoWeakArc<T>
where
    T: Update,
{
    unsafe fn maybe_update(old_pointer: *mut Self, new_arc: Self) -> bool {
        let old_arc = unsafe { &mut *old_pointer };

        if Self::ptr_eq(old_arc, &new_arc) {
            return false;
        }

        if let Some(old_inner) = Arc::get_mut(&mut old_arc.0) {
            match Arc::try_unwrap(new_arc.0) {
                Ok(new_inner) => unsafe { T::maybe_update(old_inner, new_inner) },
                Err(new_arc) => {
                    old_arc.0 = new_arc;
                    true
                }
            }
        } else {
            old_arc.0 = new_arc.0;
            true
        }
    }
}

/// An atomically reference-counted header and trailing slice stored in one allocation.
#[derive(Debug, Eq, Hash, PartialEq)]
#[repr(transparent)]
pub struct ThinArcSlice<H, T>(ThinArc<H, T>);

impl<H, T> Clone for ThinArcSlice<H, T> {
    fn clone(&self) -> Self {
        Self(self.0.clone())
    }
}

impl<H, T> ThinArcSlice<H, T> {
    pub fn from_iter<I>(header: H, items: I) -> Self
    where
        I: IntoIterator<Item = T>,
        I::IntoIter: ExactSizeIterator,
    {
        Self(ThinArc::from_header_and_iter(header, items.into_iter()))
    }

    pub fn header(&self) -> &H {
        &self.0.header.header
    }

    pub fn as_slice(&self) -> &[T] {
        &self.0.slice
    }

    /// Mutates the contents in place when uniquely owned, cloning the allocation first when it is
    /// shared.
    pub fn with_make_mut<R>(&mut self, f: impl FnOnce(&mut [T]) -> R) -> R
    where
        H: Clone,
        T: Clone,
    {
        if ThinArc::strong_count(&self.0) > 1 {
            self.0 = ThinArc::from_header_and_iter(
                self.header().clone(),
                self.as_slice().iter().cloned(),
            );
        }

        self.0.with_arc_mut(|arc| {
            let value = Arc::get_mut(arc)
                .expect("a ThinArc without weak references and one strong reference is unique");
            f(value.slice_mut())
        })
    }
}

impl<H, T> Deref for ThinArcSlice<H, T> {
    type Target = [T];

    fn deref(&self) -> &Self::Target {
        self.as_slice()
    }
}

impl<H, T> GetSize for ThinArcSlice<H, T>
where
    H: GetSize,
    T: GetSize,
{
    fn get_heap_size_with_tracker<Tracker: GetSizeTracker>(
        &self,
        mut tracker: Tracker,
    ) -> (usize, Tracker) {
        if !tracker.track(self.0.heap_ptr()) {
            return (0, tracker);
        }

        let (header_size, tracker) = H::get_size_with_tracker(self.header(), tracker);
        let (slice_size, tracker) =
            self.as_slice()
                .iter()
                .fold((size_of::<usize>(), tracker), |(size, tracker), item| {
                    let (item_size, tracker) = T::get_size_with_tracker(item, tracker);
                    (size + item_size, tracker)
                });

        (header_size + slice_size, tracker)
    }
}

// SAFETY: Equal values contain only `Update`-safe headers and elements, so retaining an equal old
// allocation across revisions is safe. Unequal values replace the complete allocation.
#[expect(
    unsafe_code,
    reason = "implementing Salsa's unsafe Update contract requires dereferencing its raw pointer"
)]
unsafe impl<H, T> Update for ThinArcSlice<H, T>
where
    H: Eq + Update,
    T: Eq + Update,
{
    unsafe fn maybe_update(old_pointer: *mut Self, new_value: Self) -> bool {
        let old_value = unsafe { &mut *old_pointer };
        if *old_value == new_value {
            false
        } else {
            *old_value = new_value;
            true
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{NoWeakArc, ThinArcSlice};

    #[test]
    fn pointers_are_thin() {
        assert_eq!(size_of::<NoWeakArc<u64>>(), size_of::<usize>());
        assert_eq!(size_of::<ThinArcSlice<u8, u16>>(), size_of::<usize>());
    }

    #[test]
    fn thin_arc_slice_make_mut_uses_copy_on_write() {
        let mut left = ThinArcSlice::from_iter(1, [2, 3]);
        let right = left.clone();

        left.with_make_mut(|slice| {
            slice[0] = 5;
        });

        assert_eq!(*left.header(), 1);
        assert_eq!(left.as_slice(), [5, 3]);
        assert_eq!(*right.header(), 1);
        assert_eq!(right.as_slice(), [2, 3]);
    }
}
