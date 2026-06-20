//! An insertion-ordered set with inline storage for a small number of elements.

use std::fmt;
use std::hash::Hash;
use std::iter::FusedIterator;
use std::mem::{self, MaybeUninit};
use std::ops::Range;
use std::slice;

use get_size2::{GetSize, GetSizeTracker};
use indexmap::{Equivalent, IndexSet};
use rustc_hash::FxBuildHasher;

type FxIndexSet<T> = IndexSet<T, FxBuildHasher>;

mod private {
    pub trait Sealed {}

    impl<T, const N: usize> Sealed for [T; N] {}
}

/// Backing array for [`SmallIndexSet`].
///
/// This trait is sealed and is implemented for all array types `[T; N]`.
pub trait SmallIndexArray: private::Sealed {
    /// The set's element type.
    type Item;

    /// The number of elements that fit inline.
    const CAPACITY: usize;
}

impl<T, const N: usize> SmallIndexArray for [T; N] {
    type Item = T;

    const CAPACITY: usize = N;
}

#[repr(C)]
struct Inline<A: SmallIndexArray> {
    len: usize,
    items: MaybeUninit<A>,
}

impl<A: SmallIndexArray> Inline<A> {
    const fn new() -> Self {
        Self {
            len: 0,
            items: MaybeUninit::uninit(),
        }
    }

    fn len(&self) -> usize {
        self.len
    }

    fn set_len(&mut self, len: usize) {
        debug_assert!(len <= A::CAPACITY);
        self.len = len;
    }

    fn as_ptr(&self) -> *const A::Item {
        self.items.as_ptr().cast()
    }

    fn as_mut_ptr(&mut self) -> *mut A::Item {
        self.items.as_mut_ptr().cast()
    }

    fn as_slice(&self) -> &[A::Item] {
        // SAFETY: The first `len` array elements are initialized by the `Inline` invariant.
        unsafe { slice::from_raw_parts(self.as_ptr(), self.len()) }
    }

    fn push(&mut self, value: A::Item) {
        let len = self.len();
        debug_assert!(len < A::CAPACITY);
        // SAFETY: `len` points to the first uninitialized array element.
        unsafe { self.as_mut_ptr().add(len).write(value) };
        self.set_len(len + 1);
    }

    fn pop(&mut self) -> A::Item {
        let new_len = self.len() - 1;
        self.set_len(new_len);
        // SAFETY: The old last element was initialized and is no longer covered by `len`.
        unsafe { self.as_mut_ptr().add(new_len).read() }
    }

    fn shift_remove_index(&mut self, index: usize) -> A::Item {
        let len = self.len();
        debug_assert!(index < len);

        let pointer = self.as_mut_ptr();
        // SAFETY: `index` is initialized. Decreasing `len` first prevents double-drop if moving or
        // dropping the returned value unwinds. `copy` supports the overlapping shifted ranges.
        let removed = unsafe { pointer.add(index).read() };
        self.set_len(len - 1);
        unsafe {
            pointer
                .add(index + 1)
                .copy_to(pointer.add(index), len - index - 1)
        };
        removed
    }

    fn swap_remove_index(&mut self, index: usize) -> A::Item {
        let len = self.len();
        debug_assert!(index < len);

        let pointer = self.as_mut_ptr();
        // SAFETY: `index` and the old last element are initialized. Decreasing `len` first
        // prevents double-drop if dropping the returned value unwinds. If they are distinct,
        // moving the last element fills the removed element's slot.
        let removed = unsafe { pointer.add(index).read() };
        self.set_len(len - 1);
        if index != len - 1 {
            unsafe { pointer.add(index).write(pointer.add(len - 1).read()) };
        }
        removed
    }

    fn clear(&mut self) {
        let len = self.len();
        self.set_len(0);
        // SAFETY: The old first `len` elements were initialized. Setting the length to zero first
        // prevents a second drop if an element destructor unwinds.
        unsafe {
            std::ptr::drop_in_place(std::ptr::slice_from_raw_parts_mut(self.as_mut_ptr(), len))
        };
    }

    fn into_iter(mut self) -> InlineIntoIter<A> {
        let remaining = 0..self.len();
        self.set_len(0);
        let items = mem::replace(&mut self.items, MaybeUninit::uninit());
        InlineIntoIter { items, remaining }
    }
}

impl<A: SmallIndexArray> Drop for Inline<A> {
    fn drop(&mut self) {
        self.clear();
    }
}

enum Storage<A, T>
where
    A: SmallIndexArray<Item = T>,
{
    Inline(Inline<A>),
    Spilled(FxIndexSet<T>),
}

/// An insertion-ordered set that stores up to `N` elements inline.
///
/// `SmallIndexSet<[T; N]>` uses linear lookup while it contains at most `N` elements. Inserting a
/// new element once the inline storage is full moves the elements into an Fx-hashed [`IndexSet`].
/// A spilled set remains spilled even if elements are later removed.
///
/// The spilled `IndexSet` is stored directly in the enum. The separate, defaulted `T` parameter
/// preserves covariance by avoiding an associated-type projection in that variant while keeping
/// the `SmallIndexSet<[T; N]>` syntax. Long-lived callers should choose the largest useful `N` for
/// which `SmallIndexSet<[T; N]>` is no larger or more aligned than `IndexSet<T>` in the optimized
/// build. Temporary callers may prefer a larger capacity when profiling supports it.
pub struct SmallIndexSet<A, T = <A as SmallIndexArray>::Item>
where
    A: SmallIndexArray<Item = T>,
{
    storage: Storage<A, T>,
}

impl<A, T> SmallIndexSet<A, T>
where
    A: SmallIndexArray<Item = T>,
{
    /// Creates an empty inline set.
    pub const fn new() -> Self {
        Self {
            storage: Storage::Inline(Inline::new()),
        }
    }

    /// Returns the number of elements in the set.
    pub fn len(&self) -> usize {
        match &self.storage {
            Storage::Inline(inline) => inline.len(),
            Storage::Spilled(spilled) => spilled.len(),
        }
    }

    /// Returns `true` if the set contains no elements.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Returns `true` if the set has moved to its hashed representation.
    #[cfg(test)]
    fn is_spilled(&self) -> bool {
        matches!(self.storage, Storage::Spilled(_))
    }

    /// Returns an iterator in insertion order.
    pub fn iter(&self) -> Iter<'_, A> {
        let inner = match &self.storage {
            Storage::Inline(inline) => IterInner::Inline(inline.as_slice().iter()),
            Storage::Spilled(spilled) => IterInner::Spilled(spilled.iter()),
        };
        Iter { inner }
    }

    /// Returns the first element in insertion order, if any.
    pub fn first(&self) -> Option<&T> {
        self.get_index(0)
    }

    /// Returns the element at `index` in insertion order, if any.
    pub(super) fn get_index(&self, index: usize) -> Option<&T> {
        match &self.storage {
            Storage::Inline(inline) => inline.as_slice().get(index),
            Storage::Spilled(spilled) => spilled.get_index(index),
        }
    }

    /// Returns `true` if `value` is in the set.
    pub fn contains<Q>(&self, value: &Q) -> bool
    where
        Q: ?Sized + Hash + Equivalent<T>,
        T: Hash + Eq,
    {
        self.get(value).is_some()
    }

    /// Returns the element equivalent to `value`, if present.
    fn get<Q>(&self, value: &Q) -> Option<&T>
    where
        Q: ?Sized + Hash + Equivalent<T>,
        T: Hash + Eq,
    {
        match &self.storage {
            Storage::Inline(inline) => inline
                .as_slice()
                .iter()
                .find(|item| value.equivalent(*item)),
            Storage::Spilled(spilled) => spilled.get(value),
        }
    }

    /// Inserts `value`, preserving insertion order.
    ///
    /// Returns `true` if the value was newly inserted.
    pub fn insert(&mut self, value: T) -> bool
    where
        T: Hash + Eq,
    {
        match &mut self.storage {
            Storage::Spilled(spilled) => return spilled.insert(value),
            Storage::Inline(inline) => {
                if inline.as_slice().contains(&value) {
                    return false;
                }
                if inline.len() < A::CAPACITY {
                    inline.push(value);
                    return true;
                }
            }
        }

        self.spill_and_insert(value)
    }

    /// Removes and returns the last element.
    pub fn pop(&mut self) -> Option<T> {
        match &mut self.storage {
            Storage::Inline(inline) => (inline.len() > 0).then(|| inline.pop()),
            Storage::Spilled(spilled) => spilled.pop(),
        }
    }

    /// Removes `value` by swapping the last element into its place.
    ///
    /// Returns `true` if the value was present.
    pub fn swap_remove<Q>(&mut self, value: &Q) -> bool
    where
        Q: ?Sized + Hash + Equivalent<T>,
        T: Hash + Eq,
    {
        match &mut self.storage {
            Storage::Inline(inline) => {
                if let Some(index) = inline
                    .as_slice()
                    .iter()
                    .position(|item| value.equivalent(item))
                {
                    drop(inline.swap_remove_index(index));
                    true
                } else {
                    false
                }
            }
            Storage::Spilled(spilled) => spilled.swap_remove(value),
        }
    }

    /// Removes and returns the element at `index`, swapping the last element into its place.
    pub fn swap_remove_index(&mut self, index: usize) -> Option<T> {
        match &mut self.storage {
            Storage::Inline(inline) => {
                (index < inline.len()).then(|| inline.swap_remove_index(index))
            }
            Storage::Spilled(spilled) => spilled.swap_remove_index(index),
        }
    }

    /// Retains only elements for which `keep` returns `true`.
    pub fn retain(&mut self, mut keep: impl FnMut(&T) -> bool) {
        match &mut self.storage {
            Storage::Inline(inline) => {
                let mut index = 0;
                while index < inline.len() {
                    if keep(&inline.as_slice()[index]) {
                        index += 1;
                    } else {
                        drop(inline.shift_remove_index(index));
                    }
                }
            }
            Storage::Spilled(spilled) => spilled.retain(keep),
        }
    }

    /// Removes all elements without changing a spilled set back to inline storage.
    pub(super) fn clear(&mut self) {
        match &mut self.storage {
            Storage::Inline(inline) => inline.clear(),
            Storage::Spilled(spilled) => spilled.clear(),
        }
    }

    /// Shrinks the spilled representation's heap allocations as much as possible.
    pub fn shrink_to_fit(&mut self) {
        if let Storage::Spilled(spilled) = &mut self.storage {
            spilled.shrink_to_fit();
        }
    }

    fn spill_and_insert(&mut self, value: T) -> bool
    where
        T: Hash + Eq,
    {
        // Allocate the IndexSet's internal tables before moving inline elements.
        let mut spilled =
            FxIndexSet::with_capacity_and_hasher(A::CAPACITY.saturating_add(1), FxBuildHasher);

        // Replace the representation first so that `self` remains valid if hashing an inline
        // element panics. The inline iterator owns and drops elements not yet transferred.
        match mem::replace(&mut self.storage, Storage::Inline(Inline::new())) {
            Storage::Inline(inline) => {
                for value in inline.into_iter() {
                    let inserted = spilled.insert(value);
                    debug_assert!(inserted, "inline values must be unique");
                }
            }
            Storage::Spilled(mut existing) => {
                let inserted = existing.insert(value);
                self.storage = Storage::Spilled(existing);
                return inserted;
            }
        }

        let inserted = spilled.insert(value);
        debug_assert!(inserted, "new value was checked before spilling");
        self.storage = Storage::Spilled(spilled);
        inserted
    }
}

impl<A, T> Default for SmallIndexSet<A, T>
where
    A: SmallIndexArray<Item = T>,
{
    fn default() -> Self {
        Self::new()
    }
}

impl<A, T> Clone for SmallIndexSet<A, T>
where
    A: SmallIndexArray<Item = T>,
    T: Clone,
{
    fn clone(&self) -> Self {
        let storage = match &self.storage {
            Storage::Inline(inline) => {
                let mut cloned = Inline::new();
                for value in inline.as_slice() {
                    cloned.push(value.clone());
                }
                Storage::Inline(cloned)
            }
            Storage::Spilled(spilled) => Storage::Spilled(spilled.clone()),
        };
        Self { storage }
    }
}

impl<A, T> fmt::Debug for SmallIndexSet<A, T>
where
    A: SmallIndexArray<Item = T>,
    T: fmt::Debug,
{
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.debug_set().entries(self).finish()
    }
}

impl<A, T> PartialEq for SmallIndexSet<A, T>
where
    A: SmallIndexArray<Item = T>,
    T: Hash + Eq,
{
    fn eq(&self, other: &Self) -> bool {
        self.len() == other.len() && self.iter().all(|item| other.contains(item))
    }
}

impl<A, T> Eq for SmallIndexSet<A, T>
where
    A: SmallIndexArray<Item = T>,
    T: Hash + Eq,
{
}

impl<A, T> Extend<T> for SmallIndexSet<A, T>
where
    A: SmallIndexArray<Item = T>,
    T: Hash + Eq,
{
    fn extend<I: IntoIterator<Item = T>>(&mut self, iter: I) {
        for value in iter {
            self.insert(value);
        }
    }
}

impl<'a, T, const N: usize> Extend<&'a T> for SmallIndexSet<[T; N]>
where
    T: 'a + Copy + Hash + Eq,
{
    fn extend<I: IntoIterator<Item = &'a T>>(&mut self, iter: I) {
        self.extend(iter.into_iter().copied());
    }
}

impl<A, T> FromIterator<T> for SmallIndexSet<A, T>
where
    A: SmallIndexArray<Item = T>,
    T: Hash + Eq,
{
    fn from_iter<I: IntoIterator<Item = T>>(iter: I) -> Self {
        let mut set = Self::new();
        set.extend(iter);
        set
    }
}

impl<A, T> GetSize for SmallIndexSet<A, T>
where
    A: SmallIndexArray<Item = T>,
    T: GetSize,
{
    fn get_heap_size_with_tracker<Tr: GetSizeTracker>(&self, tracker: Tr) -> (usize, Tr) {
        let (elements_size, tracker) =
            self.iter().fold((0, tracker), |(size, tracker), element| {
                let (element_size, tracker) = T::get_heap_size_with_tracker(element, tracker);
                (size + element_size, tracker)
            });

        match &self.storage {
            Storage::Inline(_) => (elements_size, tracker),
            Storage::Spilled(spilled) => {
                let allocation_size = spilled.capacity() * T::get_stack_size();
                (elements_size + allocation_size, tracker)
            }
        }
    }
}

// SAFETY: This matches Salsa's `IndexSet` implementation. Equality never dereferences stale
// database-owned values because `A::Item: Update`; changed sets are rebuilt from `new_set`.
unsafe impl<A, T> salsa::Update for SmallIndexSet<A, T>
where
    A: SmallIndexArray<Item = T>,
    T: salsa::Update + Hash + Eq,
{
    unsafe fn maybe_update(old_pointer: *mut Self, new_set: Self) -> bool {
        // SAFETY: The caller satisfies `Update::maybe_update`'s pointer requirements.
        let old_set = unsafe { &mut *old_pointer };
        if *old_set == new_set {
            false
        } else {
            old_set.clear();
            old_set.extend(new_set);
            true
        }
    }
}

impl<'a, A, T> IntoIterator for &'a SmallIndexSet<A, T>
where
    A: SmallIndexArray<Item = T>,
{
    type Item = &'a T;
    type IntoIter = Iter<'a, A>;

    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

impl<A, T> IntoIterator for SmallIndexSet<A, T>
where
    A: SmallIndexArray<Item = T>,
{
    type Item = T;
    type IntoIter = IntoIter<A>;

    fn into_iter(self) -> Self::IntoIter {
        let inner = match self.storage {
            Storage::Inline(inline) => IntoIterInner::Inline(inline.into_iter()),
            Storage::Spilled(spilled) => IntoIterInner::Spilled(spilled.into_iter()),
        };
        IntoIter { inner }
    }
}

/// An iterator over references in a [`SmallIndexSet`].
#[derive(Debug)]
pub struct Iter<'a, A: SmallIndexArray> {
    inner: IterInner<'a, A::Item>,
}

#[derive(Debug)]
enum IterInner<'a, T> {
    Inline(slice::Iter<'a, T>),
    Spilled(indexmap::set::Iter<'a, T>),
}

impl<'a, A: SmallIndexArray> Iterator for Iter<'a, A> {
    type Item = &'a A::Item;

    fn next(&mut self) -> Option<Self::Item> {
        match &mut self.inner {
            IterInner::Inline(iter) => iter.next(),
            IterInner::Spilled(iter) => iter.next(),
        }
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        let len = self.len();
        (len, Some(len))
    }
}

impl<A: SmallIndexArray> DoubleEndedIterator for Iter<'_, A> {
    fn next_back(&mut self) -> Option<Self::Item> {
        match &mut self.inner {
            IterInner::Inline(iter) => iter.next_back(),
            IterInner::Spilled(iter) => iter.next_back(),
        }
    }
}

impl<A: SmallIndexArray> ExactSizeIterator for Iter<'_, A> {
    fn len(&self) -> usize {
        match &self.inner {
            IterInner::Inline(iter) => iter.len(),
            IterInner::Spilled(iter) => iter.len(),
        }
    }
}

impl<A: SmallIndexArray> FusedIterator for Iter<'_, A> {}

/// An owning iterator over a [`SmallIndexSet`].
pub struct IntoIter<A: SmallIndexArray> {
    inner: IntoIterInner<A>,
}

enum IntoIterInner<A: SmallIndexArray> {
    Inline(InlineIntoIter<A>),
    Spilled(indexmap::set::IntoIter<A::Item>),
}

impl<A: SmallIndexArray> Iterator for IntoIter<A> {
    type Item = A::Item;

    fn next(&mut self) -> Option<Self::Item> {
        match &mut self.inner {
            IntoIterInner::Inline(iter) => iter.next(),
            IntoIterInner::Spilled(iter) => iter.next(),
        }
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        let len = self.len();
        (len, Some(len))
    }
}

impl<A: SmallIndexArray> DoubleEndedIterator for IntoIter<A> {
    fn next_back(&mut self) -> Option<Self::Item> {
        match &mut self.inner {
            IntoIterInner::Inline(iter) => iter.next_back(),
            IntoIterInner::Spilled(iter) => iter.next_back(),
        }
    }
}

impl<A: SmallIndexArray> ExactSizeIterator for IntoIter<A> {
    fn len(&self) -> usize {
        match &self.inner {
            IntoIterInner::Inline(iter) => iter.len(),
            IntoIterInner::Spilled(iter) => iter.len(),
        }
    }
}

impl<A: SmallIndexArray> FusedIterator for IntoIter<A> {}

struct InlineIntoIter<A: SmallIndexArray> {
    items: MaybeUninit<A>,
    remaining: Range<usize>,
}

impl<A: SmallIndexArray> InlineIntoIter<A> {
    fn as_mut_ptr(&mut self) -> *mut A::Item {
        self.items.as_mut_ptr().cast()
    }
}

impl<A: SmallIndexArray> Iterator for InlineIntoIter<A> {
    type Item = A::Item;

    fn next(&mut self) -> Option<Self::Item> {
        let index = self.remaining.next()?;
        // SAFETY: `remaining` only yields initialized, not-yet-moved indices.
        Some(unsafe { self.as_mut_ptr().add(index).read() })
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        let len = self.remaining.len();
        (len, Some(len))
    }
}

impl<A: SmallIndexArray> DoubleEndedIterator for InlineIntoIter<A> {
    fn next_back(&mut self) -> Option<Self::Item> {
        let index = self.remaining.next_back()?;
        // SAFETY: `remaining` only yields initialized, not-yet-moved indices.
        Some(unsafe { self.as_mut_ptr().add(index).read() })
    }
}

impl<A: SmallIndexArray> ExactSizeIterator for InlineIntoIter<A> {}
impl<A: SmallIndexArray> FusedIterator for InlineIntoIter<A> {}

impl<A: SmallIndexArray> Drop for InlineIntoIter<A> {
    fn drop(&mut self) {
        for value in self.by_ref() {
            drop(value);
        }
    }
}

#[cfg(test)]
mod tests {
    use std::hash::{Hash, Hasher};
    use std::mem::{align_of, size_of};
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, Ordering};

    use super::{FxIndexSet, SmallIndexSet};

    #[test]
    fn inserts_inline_and_spills() {
        let mut set = SmallIndexSet::<[u32; 2]>::new();
        assert!(set.insert(2));
        assert!(set.insert(1));
        assert!(!set.insert(2));
        assert_eq!(set.iter().copied().collect::<Vec<_>>(), [2, 1]);
        assert!(!set.is_spilled());

        assert!(set.insert(3));
        assert!(set.is_spilled());
        assert_eq!(set.iter().copied().collect::<Vec<_>>(), [2, 1, 3]);
    }

    #[test]
    fn duplicate_does_not_spill_full_inline_set() {
        let mut set = SmallIndexSet::<[u32; 1]>::new();
        set.insert(1);
        assert!(!set.insert(1));
        assert!(!set.is_spilled());
    }

    #[test]
    fn supports_zero_inline_capacity() {
        let mut set = SmallIndexSet::<[u32; 0]>::new();
        assert!(set.insert(1));
        assert!(set.is_spilled());
        assert_eq!(set.iter().next(), Some(&1));
    }

    #[test]
    fn retains_and_pops_inline() {
        let mut set = [1, 2, 3].into_iter().collect::<SmallIndexSet<[_; 4]>>();
        set.retain(|value| *value != 2);
        assert_eq!(set.iter().copied().collect::<Vec<_>>(), [1, 3]);
        assert_eq!(set.pop(), Some(3));
        assert_eq!(set.pop(), Some(1));
        assert!(set.is_empty());
    }

    #[test]
    fn spilled_set_stays_spilled_when_cleared() {
        let mut set = [1, 2].into_iter().collect::<SmallIndexSet<[_; 1]>>();
        assert!(set.is_spilled());
        set.clear();
        assert!(set.is_spilled());
        assert!(set.is_empty());
        set.insert(3);
        assert_eq!(set.iter().next_back(), Some(&3));
    }

    #[test]
    fn equality_is_order_insensitive() {
        let left = [1, 2].into_iter().collect::<SmallIndexSet<[_; 2]>>();
        let right = [2, 1].into_iter().collect::<SmallIndexSet<[_; 1]>>();
        assert_eq!(left.iter().copied().collect::<Vec<_>>(), [1, 2]);
        assert_eq!(right.iter().copied().collect::<Vec<_>>(), [2, 1]);

        // Compare equal-capacity sets because `PartialEq` deliberately mirrors IndexSet's API.
        let reversed = [2, 1].into_iter().collect::<SmallIndexSet<[_; 2]>>();
        assert_eq!(left, reversed);
    }

    #[test]
    fn clone_preserves_representation() {
        let inline = [String::from("a")]
            .into_iter()
            .collect::<SmallIndexSet<[_; 1]>>();
        assert!(!inline.clone().is_spilled());

        let spilled = [String::from("a"), String::from("b")]
            .into_iter()
            .collect::<SmallIndexSet<[_; 1]>>();
        assert!(spilled.clone().is_spilled());
    }

    #[test]
    fn owning_iterator_drops_unconsumed_elements() {
        let first = Arc::new(());
        let second = Arc::new(());
        let set = [Arc::clone(&first), Arc::clone(&second)]
            .into_iter()
            .collect::<SmallIndexSet<[_; 2]>>();

        let mut iter = set.into_iter();
        drop(iter.next());
        drop(iter);
        assert_eq!(Arc::strong_count(&first), 1);
        assert_eq!(Arc::strong_count(&second), 1);
    }

    #[test]
    fn owning_iterator_preserves_order_for_both_representations() {
        let inline = [1, 2].into_iter().collect::<SmallIndexSet<[_; 2]>>();
        assert_eq!(inline.into_iter().collect::<Vec<_>>(), [1, 2]);

        let spilled = [1, 2].into_iter().collect::<SmallIndexSet<[_; 1]>>();
        assert_eq!(spilled.into_iter().collect::<Vec<_>>(), [1, 2]);
    }

    #[test]
    fn remains_valid_if_hashing_an_inline_value_panics_while_spilling() {
        struct HashBomb {
            value: u32,
            panic: Arc<AtomicBool>,
        }

        impl PartialEq for HashBomb {
            fn eq(&self, other: &Self) -> bool {
                self.value == other.value
            }
        }

        impl Eq for HashBomb {}

        impl Hash for HashBomb {
            fn hash<H: Hasher>(&self, state: &mut H) {
                assert!(!self.panic.load(Ordering::Relaxed), "hash bomb");
                self.value.hash(state);
            }
        }

        let panic = Arc::new(AtomicBool::new(false));
        let mut set = SmallIndexSet::<[_; 1]>::new();
        set.insert(HashBomb {
            value: 1,
            panic: Arc::clone(&panic),
        });

        panic.store(true, Ordering::Relaxed);
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            set.insert(HashBomb {
                value: 2,
                panic: Arc::clone(&panic),
            });
        }));
        assert!(result.is_err());

        panic.store(false, Ordering::Relaxed);
        assert!(set.is_empty());
        assert!(set.insert(HashBomb { value: 3, panic }));
    }

    #[test]
    fn backing_array_preserves_covariance() {
        fn shorten<'short>(
            set: SmallIndexSet<[&'static str; 1]>,
        ) -> SmallIndexSet<[&'short str; 1]> {
            set
        }

        let set = shorten(SmallIndexSet::new());
        assert!(set.is_empty());
    }

    #[test]
    fn five_u64_values_fit_without_exceeding_index_set_layout() {
        assert_eq!(
            size_of::<SmallIndexSet<[u64; 5]>>(),
            size_of::<FxIndexSet<u64>>()
        );
        assert_eq!(
            align_of::<SmallIndexSet<[u64; 5]>>(),
            align_of::<FxIndexSet<u64>>()
        );
        assert!(size_of::<SmallIndexSet<[u64; 6]>>() > size_of::<FxIndexSet<u64>>());
    }
}
