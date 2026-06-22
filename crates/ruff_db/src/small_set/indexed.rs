//! An insertion-ordered set with inline storage for a small number of elements.

use std::fmt;
use std::hash::Hash;
use std::iter::FusedIterator;
use std::slice;

use arrayvec::ArrayVec;
use get_size2::{GetSize, GetSizeTracker};
use indexmap::{Equivalent, IndexSet};
use rustc_hash::FxBuildHasher;

type FxIndexSet<T> = IndexSet<T, FxBuildHasher>;
type Inline<T, const N: usize> = ArrayVec<T, N>;

/// An insertion-ordered set that stores up to `N` elements inline.
///
/// `SmallIndexSet<T, N>` uses linear lookup while it contains at most `N` elements. Inserting a
/// new element once the inline storage is full moves the elements into an Fx-hashed [`IndexSet`].
/// A spilled set remains spilled even if elements are later removed, unless [`Self::shrink_to_fit`]
/// returns an empty set to inline storage.
///
/// Long-lived sets should use the largest useful `N` that doesn't make the set larger or more
/// aligned than `IndexSet<T>` in optimized builds. Profiling may justify a larger capacity for
/// temporary sets.
#[derive(Clone)]
pub struct SmallIndexSet<T, const N: usize> {
    storage: Storage<T, N>,
}

impl<T, const N: usize> SmallIndexSet<T, N> {
    /// Creates an empty inline set.
    pub const fn new() -> Self {
        Self {
            storage: Storage::Inline(Inline::new_const()),
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

    /// Returns an iterator in insertion order.
    pub fn iter(&self) -> Iter<'_, T> {
        let inner = match &self.storage {
            Storage::Inline(inline) => IterInner::Inline(inline.iter()),
            Storage::Spilled(spilled) => IterInner::Spilled(spilled.iter()),
        };
        Iter { inner }
    }

    /// Returns the first element in insertion order, if any.
    pub(super) fn first(&self) -> Option<&T> {
        self.get_index(0)
    }

    /// Returns `true` if `value` is in the set.
    pub fn contains<Q>(&self, value: &Q) -> bool
    where
        Q: ?Sized + Hash + Equivalent<T>,
        T: Hash + Eq,
    {
        self.get(value).is_some()
    }

    /// Inserts `value`, preserving insertion order.
    ///
    /// Returns `true` if the value was newly inserted.
    #[inline]
    pub fn insert(&mut self, value: T) -> bool
    where
        T: Hash + Eq,
    {
        match &mut self.storage {
            Storage::Spilled(spilled) => spilled.insert(value),
            Storage::Inline(inline) => {
                if inline.contains(&value) {
                    return false;
                }
                if !inline.is_full() {
                    inline.push(value);
                    return true;
                }

                self.storage = Storage::Spilled(Self::spill_and_insert(inline, value));
                true
            }
        }
    }

    pub(super) fn map(&self, mut map: impl FnMut(&T) -> T) -> Self
    where
        T: Hash + Eq,
    {
        let storage = match &self.storage {
            Storage::Inline(inline) => {
                let mut mapped = Inline::new();
                for value in inline {
                    let value = map(value);
                    if !mapped.contains(&value) {
                        mapped.push(value);
                    }
                }
                Storage::Inline(mapped)
            }
            Storage::Spilled(spilled) => Storage::Spilled(spilled.iter().map(map).collect()),
        };
        Self { storage }
    }

    pub(super) fn try_map(&self, mut map: impl FnMut(&T) -> Option<T>) -> Option<Self>
    where
        T: Hash + Eq,
    {
        let storage = match &self.storage {
            Storage::Inline(inline) => {
                let mut mapped = Inline::new();
                for value in inline {
                    let value = map(value)?;
                    if !mapped.contains(&value) {
                        mapped.push(value);
                    }
                }
                Storage::Inline(mapped)
            }
            Storage::Spilled(spilled) => {
                Storage::Spilled(spilled.iter().map(map).collect::<Option<_>>()?)
            }
        };
        Some(Self { storage })
    }

    /// Removes `value` by swapping the last element into its place.
    ///
    /// Returns `true` if the value was present.
    pub(super) fn swap_remove<Q>(&mut self, value: &Q) -> bool
    where
        Q: ?Sized + Hash + Equivalent<T>,
        T: Hash + Eq,
    {
        match &mut self.storage {
            Storage::Inline(inline) => {
                let Some(index) = inline.iter().position(|item| value.equivalent(item)) else {
                    return false;
                };
                inline.swap_remove(index);
                true
            }
            Storage::Spilled(spilled) => spilled.swap_remove(value),
        }
    }

    /// Removes and returns the element at `index`, swapping the last element into its place.
    pub(super) fn swap_remove_index(&mut self, index: usize) -> Option<T> {
        match &mut self.storage {
            Storage::Inline(inline) => inline.swap_pop(index),
            Storage::Spilled(spilled) => spilled.swap_remove_index(index),
        }
    }

    /// Retains only elements for which `keep` returns `true`.
    pub fn retain(&mut self, mut keep: impl FnMut(&T) -> bool) {
        match &mut self.storage {
            Storage::Inline(inline) => inline.retain(|value| keep(value)),
            Storage::Spilled(spilled) => spilled.retain(keep),
        }
    }

    /// Shrinks the spilled representation's heap allocations as much as possible.
    ///
    /// An empty spilled set returns to inline storage.
    pub fn shrink_to_fit(&mut self) {
        if let Storage::Spilled(spilled) = &mut self.storage {
            if spilled.is_empty() {
                self.storage = Storage::Inline(Inline::new());
            } else {
                spilled.shrink_to_fit();
            }
        }
    }

    /// Returns the element at `index` in insertion order, if any.
    pub(super) fn get_index(&self, index: usize) -> Option<&T> {
        match &self.storage {
            Storage::Inline(inline) => inline.get(index),
            Storage::Spilled(spilled) => spilled.get_index(index),
        }
    }

    /// Removes all elements without changing a spilled set back to inline storage.
    pub(super) fn clear(&mut self) {
        match &mut self.storage {
            Storage::Inline(inline) => inline.clear(),
            Storage::Spilled(spilled) => spilled.clear(),
        }
    }

    /// Returns `true` if the set has moved to its hashed representation.
    #[cfg(test)]
    fn is_spilled(&self) -> bool {
        matches!(self.storage, Storage::Spilled(_))
    }

    /// Returns the element equivalent to `value`, if present.
    fn get<Q>(&self, value: &Q) -> Option<&T>
    where
        Q: ?Sized + Hash + Equivalent<T>,
        T: Hash + Eq,
    {
        match &self.storage {
            Storage::Inline(inline) => inline.iter().find(|item| value.equivalent(*item)),
            Storage::Spilled(spilled) => spilled.get(value),
        }
    }

    #[cold]
    fn spill_and_insert(inline: &mut Inline<T, N>, value: T) -> FxIndexSet<T>
    where
        T: Hash + Eq,
    {
        // Allocate before moving the inline elements so that an allocation failure leaves the set
        // unchanged.
        let mut spilled = FxIndexSet::with_capacity_and_hasher(N.saturating_add(1), FxBuildHasher);

        // Empty the inline storage first so that the set remains valid if hashing an inline
        // element panics. `extend` owns and drops elements not yet transferred.
        spilled.extend(inline.take());

        let inserted = spilled.insert(value);
        debug_assert!(inserted, "new value was checked before spilling");
        spilled
    }
}

impl<T, const N: usize> Default for SmallIndexSet<T, N> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T, const N: usize> fmt::Debug for SmallIndexSet<T, N>
where
    T: fmt::Debug,
{
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.debug_set().entries(self).finish()
    }
}

impl<T, const N: usize> PartialEq for SmallIndexSet<T, N>
where
    T: Hash + Eq,
{
    fn eq(&self, other: &Self) -> bool {
        self.len() == other.len() && self.iter().all(|item| other.contains(item))
    }
}

impl<T, const N: usize> Eq for SmallIndexSet<T, N> where T: Hash + Eq {}

impl<T, const N: usize> Extend<T> for SmallIndexSet<T, N>
where
    T: Hash + Eq,
{
    fn extend<I: IntoIterator<Item = T>>(&mut self, iter: I) {
        for value in iter {
            self.insert(value);
        }
    }
}

impl<'a, T, const N: usize> Extend<&'a T> for SmallIndexSet<T, N>
where
    T: 'a + Copy + Hash + Eq,
{
    fn extend<I: IntoIterator<Item = &'a T>>(&mut self, iter: I) {
        self.extend(iter.into_iter().copied());
    }
}

impl<T, const N: usize> FromIterator<T> for SmallIndexSet<T, N>
where
    T: Hash + Eq,
{
    fn from_iter<I: IntoIterator<Item = T>>(iter: I) -> Self {
        let mut set = Self::new();
        set.extend(iter);
        set
    }
}

impl<T, const N: usize> GetSize for SmallIndexSet<T, N>
where
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
// database-owned values because `T: Update`; changed sets are rebuilt from `new_set`.
unsafe impl<T, const N: usize> salsa::Update for SmallIndexSet<T, N>
where
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

impl<'a, T, const N: usize> IntoIterator for &'a SmallIndexSet<T, N> {
    type Item = &'a T;
    type IntoIter = Iter<'a, T>;

    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

impl<T, const N: usize> IntoIterator for SmallIndexSet<T, N> {
    type Item = T;
    type IntoIter = IntoIter<T, N>;

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
pub struct Iter<'a, T> {
    inner: IterInner<'a, T>,
}

impl<'a, T> Iterator for Iter<'a, T> {
    type Item = &'a T;

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

impl<T> DoubleEndedIterator for Iter<'_, T> {
    fn next_back(&mut self) -> Option<Self::Item> {
        match &mut self.inner {
            IterInner::Inline(iter) => iter.next_back(),
            IterInner::Spilled(iter) => iter.next_back(),
        }
    }
}

impl<T> ExactSizeIterator for Iter<'_, T> {
    fn len(&self) -> usize {
        match &self.inner {
            IterInner::Inline(iter) => iter.len(),
            IterInner::Spilled(iter) => iter.len(),
        }
    }
}

impl<T> FusedIterator for Iter<'_, T> {}

/// An owning iterator over a [`SmallIndexSet`].
pub struct IntoIter<T, const N: usize> {
    inner: IntoIterInner<T, N>,
}

impl<T, const N: usize> Iterator for IntoIter<T, N> {
    type Item = T;

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

impl<T, const N: usize> DoubleEndedIterator for IntoIter<T, N> {
    fn next_back(&mut self) -> Option<Self::Item> {
        match &mut self.inner {
            IntoIterInner::Inline(iter) => iter.next_back(),
            IntoIterInner::Spilled(iter) => iter.next_back(),
        }
    }
}

impl<T, const N: usize> ExactSizeIterator for IntoIter<T, N> {
    fn len(&self) -> usize {
        match &self.inner {
            IntoIterInner::Inline(iter) => iter.len(),
            IntoIterInner::Spilled(iter) => iter.len(),
        }
    }
}

impl<T, const N: usize> FusedIterator for IntoIter<T, N> {}

#[derive(Debug)]
enum IterInner<'a, T> {
    Inline(slice::Iter<'a, T>),
    Spilled(indexmap::set::Iter<'a, T>),
}

enum IntoIterInner<T, const N: usize> {
    Inline(arrayvec::IntoIter<T, N>),
    Spilled(indexmap::set::IntoIter<T>),
}

#[derive(Clone)]
enum Storage<T, const N: usize> {
    Inline(Inline<T, N>),
    Spilled(FxIndexSet<T>),
}

#[cfg(test)]
mod tests {
    use std::hash::{Hash, Hasher};
    #[cfg(target_pointer_width = "64")]
    use std::mem::{align_of, size_of};
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, Ordering};

    use super::{FxIndexSet, SmallIndexSet};

    #[test]
    fn inserts_inline_and_spills() {
        let mut set = SmallIndexSet::<u32, 2>::new();
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
        let mut set = SmallIndexSet::<u32, 1>::new();
        set.insert(1);
        assert!(!set.insert(1));
        assert!(!set.is_spilled());
    }

    #[test]
    fn supports_zero_inline_capacity() {
        let mut set = SmallIndexSet::<u32, 0>::new();
        assert!(set.insert(1));
        assert!(set.is_spilled());
        assert_eq!(set.iter().next(), Some(&1));
    }

    #[test]
    fn retains_inline() {
        let mut set = [1, 2, 3].into_iter().collect::<SmallIndexSet<_, 4>>();
        set.retain(|value| *value != 2);
        assert_eq!(set.iter().copied().collect::<Vec<_>>(), [1, 3]);
    }

    #[test]
    fn empty_spilled_set_returns_inline_when_shrunk() {
        let mut set = [1, 2].into_iter().collect::<SmallIndexSet<_, 1>>();
        assert!(set.is_spilled());

        set.shrink_to_fit();
        assert!(set.is_spilled());

        set.clear();
        assert!(set.is_spilled());
        assert!(set.is_empty());

        set.shrink_to_fit();
        assert!(!set.is_spilled());

        set.insert(3);
        assert_eq!(set.iter().next_back(), Some(&3));
    }

    #[test]
    fn equality_is_order_insensitive() {
        let left = [1, 2].into_iter().collect::<SmallIndexSet<_, 2>>();
        let reversed = [2, 1].into_iter().collect::<SmallIndexSet<_, 2>>();
        assert_eq!(left, reversed);
    }

    #[test]
    fn owning_iterator_preserves_order_for_both_representations() {
        let inline = [1, 2].into_iter().collect::<SmallIndexSet<_, 2>>();
        assert_eq!(inline.into_iter().collect::<Vec<_>>(), [1, 2]);

        let spilled = [1, 2].into_iter().collect::<SmallIndexSet<_, 1>>();
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
        let mut set = SmallIndexSet::<_, 1>::new();
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
    #[cfg(target_pointer_width = "64")]
    fn five_u64_values_fit_without_exceeding_index_set_layout() {
        assert_eq!(
            size_of::<SmallIndexSet<u64, 5>>(),
            size_of::<FxIndexSet<u64>>()
        );
        assert_eq!(
            align_of::<SmallIndexSet<u64, 5>>(),
            align_of::<FxIndexSet<u64>>()
        );
        assert!(size_of::<SmallIndexSet<u64, 6>>() > size_of::<FxIndexSet<u64>>());
    }
}
