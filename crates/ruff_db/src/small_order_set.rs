//! An order-sensitive set with inline storage for a small number of elements.

use std::fmt;
use std::hash::{Hash, Hasher};
use std::ops::Index;

use get_size2::{GetSize, GetSizeTracker};
use indexmap::Equivalent;

use crate::small_index_set::{IntoIter, Iter, SmallIndexArray, SmallIndexSet};

/// An insertion-ordered set whose equality and hashing depend on insertion order.
///
/// Storage and lookup are provided by [`SmallIndexSet`]. Unlike `SmallIndexSet`, two
/// `SmallOrderSet`s containing the same values in different orders compare unequal.
#[repr(transparent)]
pub struct SmallOrderSet<A, T = <A as SmallIndexArray>::Item>(SmallIndexSet<A, T>)
where
    A: SmallIndexArray<Item = T>;

impl<A: SmallIndexArray> SmallOrderSet<A> {
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
    pub fn iter(&self) -> Iter<'_, A> {
        self.0.iter()
    }

    /// Returns the first element in insertion order, if any.
    pub fn first(&self) -> Option<&A::Item> {
        self.0.first()
    }

    /// Returns `true` if `value` is in the set.
    pub fn contains<Q>(&self, value: &Q) -> bool
    where
        Q: ?Sized + Hash + Equivalent<A::Item>,
        A::Item: Hash + Eq,
    {
        self.0.contains(value)
    }

    /// Inserts `value`, preserving insertion order.
    ///
    /// Returns `true` if the value was newly inserted.
    pub fn insert(&mut self, value: A::Item) -> bool
    where
        A::Item: Hash + Eq,
    {
        self.0.insert(value)
    }

    /// Removes `value` by swapping the last element into its place.
    ///
    /// Returns `true` if the value was present.
    pub fn swap_remove<Q>(&mut self, value: &Q) -> bool
    where
        Q: ?Sized + Hash + Equivalent<A::Item>,
        A::Item: Hash + Eq,
    {
        self.0.swap_remove(value)
    }

    /// Removes and returns the element at `index`, swapping the last element into its place.
    pub fn swap_remove_index(&mut self, index: usize) -> Option<A::Item> {
        self.0.swap_remove_index(index)
    }

    /// Retains only elements for which `keep` returns `true`.
    pub fn retain(&mut self, keep: impl FnMut(&A::Item) -> bool) {
        self.0.retain(keep);
    }

    /// Shrinks the spilled representation's heap allocations as much as possible.
    pub fn shrink_to_fit(&mut self) {
        self.0.shrink_to_fit();
    }
}

impl<A: SmallIndexArray> Default for SmallOrderSet<A> {
    fn default() -> Self {
        Self::new()
    }
}

impl<A> Clone for SmallOrderSet<A>
where
    A: SmallIndexArray,
    A::Item: Clone,
{
    fn clone(&self) -> Self {
        Self(self.0.clone())
    }
}

impl<A> fmt::Debug for SmallOrderSet<A>
where
    A: SmallIndexArray,
    A::Item: fmt::Debug,
{
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(formatter)
    }
}

impl<A> PartialEq for SmallOrderSet<A>
where
    A: SmallIndexArray,
    A::Item: PartialEq,
{
    fn eq(&self, other: &Self) -> bool {
        self.len() == other.len() && self.iter().eq(other)
    }
}

impl<A> Eq for SmallOrderSet<A>
where
    A: SmallIndexArray,
    A::Item: Eq,
{
}

impl<A> Hash for SmallOrderSet<A>
where
    A: SmallIndexArray,
    A::Item: Hash,
{
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.len().hash(state);
        for value in self {
            value.hash(state);
        }
    }
}

impl<A> Extend<A::Item> for SmallOrderSet<A>
where
    A: SmallIndexArray,
    A::Item: Hash + Eq,
{
    fn extend<I: IntoIterator<Item = A::Item>>(&mut self, iter: I) {
        self.0.extend(iter);
    }
}

impl<'a, T, const N: usize> Extend<&'a T> for SmallOrderSet<[T; N]>
where
    T: 'a + Copy + Hash + Eq,
{
    fn extend<I: IntoIterator<Item = &'a T>>(&mut self, iter: I) {
        self.0.extend(iter);
    }
}

impl<A> FromIterator<A::Item> for SmallOrderSet<A>
where
    A: SmallIndexArray,
    A::Item: Hash + Eq,
{
    fn from_iter<I: IntoIterator<Item = A::Item>>(iter: I) -> Self {
        Self(iter.into_iter().collect())
    }
}

impl<A> GetSize for SmallOrderSet<A>
where
    A: SmallIndexArray,
    A::Item: GetSize,
{
    fn get_heap_size_with_tracker<Tr: GetSizeTracker>(&self, tracker: Tr) -> (usize, Tr) {
        self.0.get_heap_size_with_tracker(tracker)
    }
}

// SAFETY: This matches Salsa's `OrderSet` implementation. Equality never dereferences stale
// database-owned values because `A::Item: Update`; changed sets are rebuilt from `new_set`.
unsafe impl<A> salsa::Update for SmallOrderSet<A>
where
    A: SmallIndexArray,
    A::Item: salsa::Update + Hash + Eq,
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

impl<'a, A: SmallIndexArray> IntoIterator for &'a SmallOrderSet<A> {
    type Item = &'a A::Item;
    type IntoIter = Iter<'a, A>;

    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

impl<A: SmallIndexArray> IntoIterator for SmallOrderSet<A> {
    type Item = A::Item;
    type IntoIter = IntoIter<A>;

    fn into_iter(self) -> Self::IntoIter {
        self.0.into_iter()
    }
}

impl<A: SmallIndexArray> Index<usize> for SmallOrderSet<A> {
    type Output = A::Item;

    fn index(&self, index: usize) -> &Self::Output {
        self.0
            .get_index(index)
            .expect("SmallOrderSet index out of bounds")
    }
}

#[cfg(test)]
mod tests {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};

    use super::SmallOrderSet;

    #[test]
    fn equality_and_hashing_are_order_sensitive() {
        let left = [1, 2].into_iter().collect::<SmallOrderSet<[_; 2]>>();
        let reversed = [2, 1].into_iter().collect::<SmallOrderSet<[_; 2]>>();
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
        let mut set = SmallOrderSet::<[u32; 3]>::new();
        set.extend(&[2, 1, 2]);
        assert_eq!(set.iter().copied().collect::<Vec<_>>(), [2, 1]);
    }

    #[test]
    fn swap_removes_from_both_representations() {
        let mut inline = [1, 2, 3].into_iter().collect::<SmallOrderSet<[_; 3]>>();
        assert!(inline.swap_remove(&2));
        assert_eq!(inline.iter().copied().collect::<Vec<_>>(), [1, 3]);

        let mut spilled = [1, 2, 3].into_iter().collect::<SmallOrderSet<[_; 2]>>();
        assert_eq!(spilled.swap_remove_index(0), Some(1));
        assert_eq!(spilled.iter().copied().collect::<Vec<_>>(), [3, 2]);
    }

    #[test]
    fn backing_array_preserves_covariance() {
        fn shorten<'short>(
            set: SmallOrderSet<[&'static str; 1]>,
        ) -> SmallOrderSet<[&'short str; 1]> {
            set
        }

        let set = shorten(SmallOrderSet::new());
        assert!(set.is_empty());
    }
}
