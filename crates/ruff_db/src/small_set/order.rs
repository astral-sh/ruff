//! An order-sensitive set with inline storage for a small number of elements.

use std::fmt;
use std::hash::{Hash, Hasher};
use std::ops::Index;

use get_size2::{GetSize, GetSizeTracker};
use indexmap::Equivalent;

use super::indexed::{IntoIter, Iter, SmallIndexSet};

/// An insertion-ordered set whose equality and hashing depend on insertion order.
///
/// Storage and lookup are provided by [`SmallIndexSet`]. Unlike `SmallIndexSet`, two
/// `SmallOrderSet`s containing the same values in different orders compare unequal.
#[repr(transparent)]
pub struct SmallOrderSet<T, const N: usize>(SmallIndexSet<T, N>);

impl<T, const N: usize> SmallOrderSet<T, N> {
    /// Creates an empty inline set.
    pub const fn new() -> Self {
        Self(SmallIndexSet::new())
    }

    /// Returns the number of elements in the set.
    pub fn len(&self) -> usize {
        self.0.len()
    }

    /// Returns `true` if the set contains no elements.
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// Returns an iterator in insertion order.
    pub fn iter(&self) -> Iter<'_, T> {
        self.0.iter()
    }

    /// Returns the first element in insertion order, if any.
    pub fn first(&self) -> Option<&T> {
        self.0.first()
    }

    /// Returns `true` if `value` is in the set.
    pub fn contains<Q>(&self, value: &Q) -> bool
    where
        Q: ?Sized + Hash + Equivalent<T>,
        T: Hash + Eq,
    {
        self.0.contains(value)
    }

    /// Inserts `value`, preserving insertion order.
    ///
    /// Returns `true` if the value was newly inserted.
    pub fn insert(&mut self, value: T) -> bool
    where
        T: Hash + Eq,
    {
        self.0.insert(value)
    }

    /// Transforms all elements, preserving insertion order.
    ///
    /// If multiple elements map to the same value, only the first is retained.
    pub fn map(&self, map: impl FnMut(&T) -> T) -> Self
    where
        T: Hash + Eq,
    {
        Self(self.0.map(map))
    }

    /// Transforms all elements, returning `None` if the transformation fails.
    ///
    /// If multiple elements map to the same value, only the first is retained.
    pub fn try_map(&self, map: impl FnMut(&T) -> Option<T>) -> Option<Self>
    where
        T: Hash + Eq,
    {
        self.0.try_map(map).map(Self)
    }

    /// Removes `value` by swapping the last element into its place.
    ///
    /// Returns `true` if the value was present.
    pub fn swap_remove<Q>(&mut self, value: &Q) -> bool
    where
        Q: ?Sized + Hash + Equivalent<T>,
        T: Hash + Eq,
    {
        self.0.swap_remove(value)
    }

    /// Removes and returns the element at `index`, swapping the last element into its place.
    pub fn swap_remove_index(&mut self, index: usize) -> Option<T> {
        self.0.swap_remove_index(index)
    }

    /// Retains only elements for which `keep` returns `true`.
    pub fn retain(&mut self, keep: impl FnMut(&T) -> bool) {
        self.0.retain(keep);
    }

    /// Shrinks the spilled representation's heap allocations as much as possible.
    ///
    /// An empty spilled set returns to inline storage.
    pub fn shrink_to_fit(&mut self) {
        self.0.shrink_to_fit();
    }
}

impl<T, const N: usize> Default for SmallOrderSet<T, N> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T, const N: usize> Clone for SmallOrderSet<T, N>
where
    T: Clone,
{
    fn clone(&self) -> Self {
        Self(self.0.clone())
    }
}

impl<T, const N: usize> fmt::Debug for SmallOrderSet<T, N>
where
    T: fmt::Debug,
{
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(formatter)
    }
}

impl<T, const N: usize> PartialEq for SmallOrderSet<T, N>
where
    T: PartialEq,
{
    fn eq(&self, other: &Self) -> bool {
        self.len() == other.len() && self.iter().eq(other)
    }
}

impl<T, const N: usize> Eq for SmallOrderSet<T, N> where T: Eq {}

impl<T, const N: usize> Hash for SmallOrderSet<T, N>
where
    T: Hash,
{
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.len().hash(state);
        for value in self {
            value.hash(state);
        }
    }
}

impl<T, const N: usize> Index<usize> for SmallOrderSet<T, N> {
    type Output = T;

    fn index(&self, index: usize) -> &Self::Output {
        self.0
            .get_index(index)
            .expect("SmallOrderSet index out of bounds")
    }
}

impl<'a, T, const N: usize> IntoIterator for &'a SmallOrderSet<T, N> {
    type Item = &'a T;
    type IntoIter = Iter<'a, T>;

    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

impl<T, const N: usize> IntoIterator for SmallOrderSet<T, N> {
    type Item = T;
    type IntoIter = IntoIter<T, N>;

    fn into_iter(self) -> Self::IntoIter {
        self.0.into_iter()
    }
}

impl<T, const N: usize> Extend<T> for SmallOrderSet<T, N>
where
    T: Hash + Eq,
{
    fn extend<I: IntoIterator<Item = T>>(&mut self, iter: I) {
        self.0.extend(iter);
    }
}

impl<'a, T, const N: usize> Extend<&'a T> for SmallOrderSet<T, N>
where
    T: 'a + Copy + Hash + Eq,
{
    fn extend<I: IntoIterator<Item = &'a T>>(&mut self, iter: I) {
        self.0.extend(iter);
    }
}

impl<T, const N: usize> FromIterator<T> for SmallOrderSet<T, N>
where
    T: Hash + Eq,
{
    fn from_iter<I: IntoIterator<Item = T>>(iter: I) -> Self {
        Self(iter.into_iter().collect())
    }
}

impl<T, const N: usize> GetSize for SmallOrderSet<T, N>
where
    T: GetSize,
{
    fn get_heap_size_with_tracker<Tr: GetSizeTracker>(&self, tracker: Tr) -> (usize, Tr) {
        self.0.get_heap_size_with_tracker(tracker)
    }
}

// SAFETY: This matches Salsa's `OrderSet` implementation. Equality never dereferences stale
// database-owned values because `T: Update`; changed sets are rebuilt from `new_set`.
unsafe impl<T, const N: usize> salsa::Update for SmallOrderSet<T, N>
where
    T: salsa::Update + Hash + Eq,
{
    unsafe fn maybe_update(old_pointer: *mut Self, new_set: Self) -> bool {
        // SAFETY: The caller satisfies `Update::maybe_update`'s pointer requirements.
        let old_set = unsafe { &mut *old_pointer };
        if *old_set == new_set {
            false
        } else {
            old_set.0.clear();
            old_set.extend(new_set);
            true
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};

    use super::SmallOrderSet;

    #[test]
    fn equality_and_hashing_are_order_sensitive() {
        let left = [1, 2].into_iter().collect::<SmallOrderSet<_, 2>>();
        let reversed = [2, 1].into_iter().collect::<SmallOrderSet<_, 2>>();
        assert_ne!(left, reversed);

        fn hash(value: &impl Hash) -> u64 {
            let mut hasher = DefaultHasher::new();
            value.hash(&mut hasher);
            hasher.finish()
        }

        assert_ne!(hash(&left), hash(&reversed));
    }

    #[test]
    fn supports_borrowed_extend() {
        let mut set = SmallOrderSet::<u32, 3>::new();
        set.extend(&[2, 1, 2]);
        assert_eq!(set.iter().copied().collect::<Vec<_>>(), [2, 1]);
    }

    #[test]
    fn maps_elements() {
        let inline = [1, 2].into_iter().collect::<SmallOrderSet<_, 2>>();
        let mapped = inline.map(|_| 0);
        assert_eq!(mapped.iter().copied().collect::<Vec<_>>(), [0]);
        assert_eq!(
            inline.try_map(|value| (*value != 2).then_some(*value)),
            None
        );

        let spilled = [1, 2, 3].into_iter().collect::<SmallOrderSet<_, 2>>();
        let mapped = spilled.map(|value| value % 2);
        assert_eq!(mapped.iter().copied().collect::<Vec<_>>(), [1, 0]);
    }

    #[test]
    fn swap_removes_from_both_representations() {
        let mut inline = [1, 2, 3].into_iter().collect::<SmallOrderSet<_, 3>>();
        assert!(inline.swap_remove(&2));
        assert_eq!(inline.iter().copied().collect::<Vec<_>>(), [1, 3]);

        let mut spilled = [1, 2, 3].into_iter().collect::<SmallOrderSet<_, 2>>();
        assert_eq!(spilled.swap_remove_index(0), Some(1));
        assert_eq!(spilled.iter().copied().collect::<Vec<_>>(), [3, 2]);
    }

    #[test]
    fn preserves_covariance() {
        fn shorten<'short>(set: SmallOrderSet<&'static str, 1>) -> SmallOrderSet<&'short str, 1> {
            set
        }

        let set = shorten(SmallOrderSet::new());
        assert!(set.is_empty());
    }
}
