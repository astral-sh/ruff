//! Sorted, arena-allocated association lists
//!
//! An [_association list_][alist], which is a linked list of key/value pairs. We additionally
//! guarantee that the elements of an association list are sorted (by their keys), and that they do
//! not contain any entries with duplicate keys.
//!
//! Association lists have fallen out of favor in recent decades, since you often need operations
//! that are inefficient on them. In particular, looking up a random element by index is O(n), just
//! like a linked list; and looking up an element by key is also O(n), since you must do a linear
//! scan of the list to find the matching element. The typical implementation also suffers from
//! poor cache locality and high memory allocation overhead, since individual list cells are
//! typically allocated separately from the heap. We solve that last problem by storing the cells
//! of an association list in an [`IndexVec`] arena.
//!
//! We exploit structural sharing where possible, reusing cells across multiple lists when we can.
//! That said, we don't guarantee that lists are canonical — it's entirely possible for two lists
//! with identical contents to use different list cells and have different identifiers.
//!
//! Given all of this, association lists have the following benefits:
//!
//! - Lists can be represented by a single 32-bit integer (the index into the arena of the head of
//!   the list).
//! - Lists can be cloned in constant time, since the underlying cells are immutable.
//! - Lists can be combined quickly (for both intersection and union), especially when you already
//!   have to zip through both input lists to combine each key's values in some way.
//!
//! There is one remaining caveat:
//!
//! - You should construct lists in key order; doing this lets you insert each value in constant time.
//!   Inserting entries in reverse order results in _quadratic_ overall time to construct the list.
//!
//! Lists are created using a [`ListBuilder`], and once created are accessed via a [`ListStorage`].
//!
//! ## Tests
//!
//! This module contains quickcheck-based property tests.
//!
//! These tests are disabled by default, as they are non-deterministic and slow. You can run them
//! explicitly using:
//!
//! ```sh
//! cargo test -p ruff_index -- --ignored list::property_tests
//! ```
//!
//! The number of tests (default: 100) can be controlled by setting the `QUICKCHECK_TESTS`
//! environment variable. For example:
//!
//! ```sh
//! QUICKCHECK_TESTS=10000 cargo test …
//! ```
//!
//! If you want to run these tests for a longer period of time, it's advisable to run them in
//! release mode. As some tests are slower than others, it's advisable to run them in a loop until
//! they fail:
//!
//! ```sh
//! export QUICKCHECK_TESTS=100000
//! while cargo test --release -p ruff_index -- \
//!   --ignored list::property_tests; do :; done
//! ```
//!
//! [alist]: https://en.wikipedia.org/wiki/Association_list

use std::cmp::Ordering;
use std::marker::PhantomData;
use std::ops::Deref;

use ruff_index::{newtype_index, IndexVec};

/// A handle to an association list. Use [`ListStorage`] to access its elements, and
/// [`ListBuilder`] to construct other lists based on this one.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub(crate) struct List<K, V = ()> {
    last: Option<ListCellId>,
    _phantom: PhantomData<(K, V)>,
}

impl<K, V> List<K, V> {
    pub(crate) const fn empty() -> List<K, V> {
        List::new(None)
    }

    const fn new(last: Option<ListCellId>) -> List<K, V> {
        List {
            last,
            _phantom: PhantomData,
        }
    }
}

impl<K, V> Default for List<K, V> {
    fn default() -> Self {
        List::empty()
    }
}

#[newtype_index]
#[derive(PartialOrd, Ord)]
struct ListCellId;

/// Stores one or more association lists. This type provides read-only access to the lists.  Use a
/// [`ListBuilder`] to create lists.
#[derive(Debug, Eq, PartialEq)]
pub(crate) struct ListStorage<K, V = ()> {
    cells: IndexVec<ListCellId, ListCell<K, V>>,
}

/// Each association list is represented by a sequence of snoc cells. A snoc cell is like the more
/// familiar cons cell `(a : (b : (c : nil)))`, but in reverse `(((nil : a) : b) : c)`.
///
/// **Terminology**: The elements of a cons cell are usually called `head` and `tail` (assuming
/// you're not in Lisp-land, where they're called `car` and `cdr`).  The elements of a snoc cell
/// are usually called `rest` and `last`.
#[derive(Debug, Eq, PartialEq)]
struct ListCell<K, V> {
    rest: Option<ListCellId>,
    key: K,
    value: V,
}

/// Constructs one or more association lists.
#[derive(Debug, Eq, PartialEq)]
pub(crate) struct ListBuilder<K, V = ()> {
    storage: ListStorage<K, V>,

    /// Scratch space that lets us implement our list operations iteratively instead of
    /// recursively.
    ///
    /// The snoc-list representation that we use for alists is very common in functional
    /// programming, and the simplest implementations of most of the operations are defined
    /// recursively on that data structure. However, they are not _tail_ recursive, which means
    /// that the call stack grows linearly with the size of the input, which can be a problem for
    /// large lists.
    ///
    /// You can often rework those recursive implementations into iterative ones using an
    /// _accumulator_, but that comes at the cost of reversing the list. If we didn't care about
    /// ordering, that wouldn't be a problem. Since we want our lists to be sorted, we can't rely
    /// on that on its own.
    ///
    /// The next standard trick is to use an accumulator, and use a fix-up step at the end to
    /// reverse the (reversed) result in the accumulator, restoring the correct order.
    ///
    /// So, that's what we do! However, as one last optimization, we don't build up alist cells in
    /// our accumulator, since that would add wasteful cruft to our list storage. Instead, we use a
    /// normal Vec as our accumulator, holding the key/value pairs that should be stitched onto the
    /// end of whatever result list we are creating. For our fix-up step, we can consume a Vec in
    /// reverse order by `pop`ping the elements off one by one.
    scratch: Vec<(K, V)>,
}

impl<K, V> Default for ListBuilder<K, V> {
    fn default() -> Self {
        ListBuilder {
            storage: ListStorage {
                cells: IndexVec::default(),
            },
            scratch: Vec::default(),
        }
    }
}

impl<K, V> Deref for ListBuilder<K, V> {
    type Target = ListStorage<K, V>;
    fn deref(&self) -> &ListStorage<K, V> {
        &self.storage
    }
}

impl<K, V> ListBuilder<K, V> {
    /// Finalizes a `ListBuilder`. After calling this, you cannot create any new lists managed by
    /// this storage.
    pub(crate) fn build(mut self) -> ListStorage<K, V> {
        self.storage.cells.shrink_to_fit();
        self.storage
    }

    /// Adds a new cell to the list.
    ///
    /// Adding an element always returns a non-empty list, which means we could technically use `I`
    /// as our return type, since we never return `None`. However, for consistency with our other
    /// methods, we always use `Option<I>` as the return type for any method that can return a
    /// list.
    #[allow(clippy::unnecessary_wraps)]
    fn add_cell(&mut self, rest: Option<ListCellId>, key: K, value: V) -> Option<ListCellId> {
        Some(self.storage.cells.push(ListCell { rest, key, value }))
    }

    /// Returns an entry pointing at where `key` would be inserted into a list.
    ///
    /// Note that when we add a new element to a list, we might have to clone the keys and values
    /// of some existing elements. This is because list cells are immutable once created, since
    /// they might be shared across multiple lists. We must therefore create new cells for every
    /// element that appears after the new element.
    ///
    /// That means that you should construct lists in key order, since that means that there are no
    /// entries to duplicate for each insertion. If you construct the list in reverse order, we
    /// will have to duplicate O(n) entries for each insertion, making it _quadratic_ to construct
    /// the entire list.
    pub(crate) fn entry(&mut self, list: List<K, V>, key: K) -> ListEntry<K, V>
    where
        K: Clone + Ord,
        V: Clone,
    {
        self.scratch.clear();

        // Iterate through the input list, looking for the position where the key should be
        // inserted. We will need to create new list cells for any elements that appear after the
        // new key. Stash those away in our scratch accumulator as we step through the input. The
        // result of the loop is that "rest" of the result list, which we will stitch the new key
        // (and any succeeding keys) onto.
        let mut curr = list.last;
        while let Some(curr_id) = curr {
            let cell = &self.storage.cells[curr_id];
            match key.cmp(&cell.key) {
                // We found an existing entry in the input list with the desired key.
                Ordering::Equal => {
                    return ListEntry {
                        builder: self,
                        list,
                        key,
                        rest: ListTail::Occupied(curr_id),
                    };
                }
                // The input list does not already contain this key, and this is where we should
                // add it.
                Ordering::Greater => {
                    return ListEntry {
                        builder: self,
                        list,
                        key,
                        rest: ListTail::Vacant(curr_id),
                    };
                }
                // If this key is in the list, it's further along. We'll need to create a new cell
                // for this entry in the result list, so add its contents to the scratch
                // accumulator.
                Ordering::Less => {
                    let new_key = cell.key.clone();
                    let new_value = cell.value.clone();
                    self.scratch.push((new_key, new_value));
                    curr = cell.rest;
                }
            }
        }

        // We made it all the way through the list without finding the desired key, so it belongs
        // at the beginning. (And we will unfortunately have to duplicate every existing cell if
        // the caller proceeds with inserting the new key!)
        ListEntry {
            builder: self,
            list,
            key,
            rest: ListTail::Beginning,
        }
    }
}

/// A view into a list, indicating where a key would be inserted.
pub(crate) struct ListEntry<'a, K, V = ()> {
    builder: &'a mut ListBuilder<K, V>,
    list: List<K, V>,
    key: K,
    /// Points at the element that already contains `key`, if there is one, or the element
    /// immediately before where it would go, if not.
    rest: ListTail<ListCellId>,
}

enum ListTail<I> {
    /// The list does not already contain `key`, and it would go at the beginning of the list.
    Beginning,
    /// The list already contains `key`
    Occupied(I),
    /// The list does not already contain key, and it would go immediately after the given element
    Vacant(I),
}

impl<K, V> ListEntry<'_, K, V>
where
    K: Clone,
    V: Clone,
{
    fn stitch_up(self, rest: Option<ListCellId>, value: V) -> List<K, V> {
        let mut last = rest;
        last = self.builder.add_cell(last, self.key, value);
        while let Some((key, value)) = self.builder.scratch.pop() {
            last = self.builder.add_cell(last, key, value);
        }
        List::new(last)
    }

    /// Inserts a new key/value into the list if the key is not already present. If the list
    /// already contains `key`, we return the original list as-is, and do not invoke your closure.
    pub(crate) fn or_insert_with<F>(self, f: F) -> List<K, V>
    where
        F: FnOnce() -> V,
    {
        let rest = match self.rest {
            // If the list already contains `key`, we don't need to replace anything, and can
            // return the original list unmodified.
            ListTail::Occupied(_) => return self.list,
            // Otherwise we have to create a new entry and stitch it onto the list.
            ListTail::Beginning => None,
            ListTail::Vacant(index) => Some(index),
        };
        self.stitch_up(rest, f())
    }

    /// Inserts a new key and the default value into the list if the key is not already present. If
    /// the list already contains `key`, we return the original list as-is.
    pub(crate) fn or_insert_default(self) -> List<K, V>
    where
        V: Default,
    {
        self.or_insert_with(V::default)
    }
}

impl<K, V> ListBuilder<K, V> {
    /// Returns the intersection of two lists. The result will contain an entry for any key that
    /// appears in both lists. The corresponding values will be combined using the `combine`
    /// function that you provide.
    #[allow(clippy::needless_pass_by_value)]
    pub(crate) fn intersect_with<F>(
        &mut self,
        a: List<K, V>,
        b: List<K, V>,
        mut combine: F,
    ) -> List<K, V>
    where
        K: Clone + Ord,
        V: Clone,
        F: FnMut(&V, &V) -> V,
    {
        self.scratch.clear();

        // Zip through the lists, building up the keys/values of the new entries into our scratch
        // vector. Continue until we run out of elements in either list. (Any remaining elements in
        // the other list cannot possibly be in the intersection.)
        let mut a = a.last;
        let mut b = b.last;
        while let (Some(a_id), Some(b_id)) = (a, b) {
            let a_cell = &self.storage.cells[a_id];
            let b_cell = &self.storage.cells[b_id];
            match a_cell.key.cmp(&b_cell.key) {
                // Both lists contain this key; combine their values
                Ordering::Equal => {
                    let new_key = a_cell.key.clone();
                    let new_value = combine(&a_cell.value, &b_cell.value);
                    self.scratch.push((new_key, new_value));
                    a = a_cell.rest;
                    b = b_cell.rest;
                }
                // a's key is only present in a, so it's not included in the result.
                Ordering::Greater => a = a_cell.rest,
                // b's key is only present in b, so it's not included in the result.
                Ordering::Less => b = b_cell.rest,
            }
        }

        // Once the iteration loop terminates, we stitch the new entries back together into proper
        // alist cells.
        let mut last = None;
        while let Some((key, value)) = self.scratch.pop() {
            last = self.add_cell(last, key, value);
        }
        List::new(last)
    }
}

// ----
// Sets

impl<K> ListStorage<K, ()> {
    /// Iterates through the elements in a set _in reverse order_.
    #[allow(clippy::needless_pass_by_value)]
    pub(crate) fn iter_set_reverse(&self, set: List<K, ()>) -> ListSetReverseIterator<K> {
        ListSetReverseIterator {
            storage: self,
            curr: set.last,
        }
    }
}

pub(crate) struct ListSetReverseIterator<'a, K> {
    storage: &'a ListStorage<K, ()>,
    curr: Option<ListCellId>,
}

impl<'a, K> Iterator for ListSetReverseIterator<'a, K> {
    type Item = &'a K;

    fn next(&mut self) -> Option<Self::Item> {
        let cell = &self.storage.cells[self.curr?];
        self.curr = cell.rest;
        Some(&cell.key)
    }
}

impl<K> ListBuilder<K, ()> {
    /// Adds an element to a set.
    pub(crate) fn insert(&mut self, set: List<K, ()>, element: K) -> List<K, ()>
    where
        K: Clone + Ord,
    {
        self.entry(set, element).or_insert_default()
    }

    /// Returns the intersection of two sets. The result will contain any value that appears in
    /// both sets.
    pub(crate) fn intersect(&mut self, a: List<K, ()>, b: List<K, ()>) -> List<K, ()>
    where
        K: Clone + Ord,
    {
        self.intersect_with(a, b, |(), ()| ())
    }
}

// -----
// Tests

#[cfg(test)]
mod tests {
    use super::*;

    use std::fmt::Display;
    use std::fmt::Write;

    // ----
    // Sets

    impl<K> ListStorage<K>
    where
        K: Display,
    {
        fn display_set(&self, list: List<K, ()>) -> String {
            let elements: Vec<_> = self.iter_set_reverse(list).collect();
            let mut result = String::new();
            result.push('[');
            for element in elements.into_iter().rev() {
                if result.len() > 1 {
                    result.push_str(", ");
                }
                write!(&mut result, "{element}").unwrap();
            }
            result.push(']');
            result
        }
    }

    #[test]
    fn can_insert_into_set() {
        let mut builder = ListBuilder::<u16>::default();

        // Build up the set in order
        let empty = List::empty();
        let set1 = builder.insert(empty, 1);
        let set12 = builder.insert(set1, 2);
        let set123 = builder.insert(set12, 3);
        let set1232 = builder.insert(set123, 2);
        assert_eq!(builder.display_set(empty), "[]");
        assert_eq!(builder.display_set(set1), "[1]");
        assert_eq!(builder.display_set(set12), "[1, 2]");
        assert_eq!(builder.display_set(set123), "[1, 2, 3]");
        assert_eq!(builder.display_set(set1232), "[1, 2, 3]");

        // And in reverse order
        let set3 = builder.insert(empty, 3);
        let set32 = builder.insert(set3, 2);
        let set321 = builder.insert(set32, 1);
        let set3212 = builder.insert(set321, 2);
        assert_eq!(builder.display_set(empty), "[]");
        assert_eq!(builder.display_set(set3), "[3]");
        assert_eq!(builder.display_set(set32), "[2, 3]");
        assert_eq!(builder.display_set(set321), "[1, 2, 3]");
        assert_eq!(builder.display_set(set3212), "[1, 2, 3]");
    }

    #[test]
    fn can_intersect_sets() {
        let mut builder = ListBuilder::<u16>::default();

        let empty = List::empty();
        let set1 = builder.insert(empty, 1);
        let set12 = builder.insert(set1, 2);
        let set123 = builder.insert(set12, 3);
        let set1234 = builder.insert(set123, 4);

        let set2 = builder.insert(empty, 2);
        let set24 = builder.insert(set2, 4);
        let set245 = builder.insert(set24, 5);
        let set2457 = builder.insert(set245, 7);

        let intersection = builder.intersect(empty, empty);
        assert_eq!(builder.display_set(intersection), "[]");
        let intersection = builder.intersect(empty, set1234);
        assert_eq!(builder.display_set(intersection), "[]");
        let intersection = builder.intersect(empty, set2457);
        assert_eq!(builder.display_set(intersection), "[]");
        let intersection = builder.intersect(set1, set1234);
        assert_eq!(builder.display_set(intersection), "[1]");
        let intersection = builder.intersect(set1, set2457);
        assert_eq!(builder.display_set(intersection), "[]");
        let intersection = builder.intersect(set2, set1234);
        assert_eq!(builder.display_set(intersection), "[2]");
        let intersection = builder.intersect(set2, set2457);
        assert_eq!(builder.display_set(intersection), "[2]");
        let intersection = builder.intersect(set1234, set2457);
        assert_eq!(builder.display_set(intersection), "[2, 4]");
    }

    // ----
    // Maps

    impl<K, V> ListStorage<K, V> {
        /// Iterates through the entries in a list _in reverse order by key_.
        #[allow(clippy::needless_pass_by_value)]
        pub(crate) fn iter_reverse(&self, list: List<K, V>) -> ListReverseIterator<'_, K, V> {
            ListReverseIterator {
                storage: self,
                curr: list.last,
            }
        }
    }

    pub(crate) struct ListReverseIterator<'a, K, V> {
        storage: &'a ListStorage<K, V>,
        curr: Option<ListCellId>,
    }

    impl<'a, K, V> Iterator for ListReverseIterator<'a, K, V> {
        type Item = (&'a K, &'a V);

        fn next(&mut self) -> Option<Self::Item> {
            let cell = &self.storage.cells[self.curr?];
            self.curr = cell.rest;
            Some((&cell.key, &cell.value))
        }
    }

    impl<K, V> ListStorage<K, V>
    where
        K: Display,
        V: Display,
    {
        fn display(&self, list: List<K, V>) -> String {
            let entries: Vec<_> = self.iter_reverse(list).collect();
            let mut result = String::new();
            result.push('[');
            for (key, value) in entries.into_iter().rev() {
                if result.len() > 1 {
                    result.push_str(", ");
                }
                write!(&mut result, "{key}:{value}").unwrap();
            }
            result.push(']');
            result
        }
    }

    #[test]
    fn can_insert_into_map() {
        let mut builder = ListBuilder::<u16, u16>::default();

        // Build up the map in order
        let empty = List::empty();
        let map1 = builder.entry(empty, 1).or_insert_with(|| 1);
        let map12 = builder.entry(map1, 2).or_insert_with(|| 2);
        let map123 = builder.entry(map12, 3).or_insert_with(|| 3);
        let map1232 = builder.entry(map123, 2).or_insert_with(|| 4);
        assert_eq!(builder.display(empty), "[]");
        assert_eq!(builder.display(map1), "[1:1]");
        assert_eq!(builder.display(map12), "[1:1, 2:2]");
        assert_eq!(builder.display(map123), "[1:1, 2:2, 3:3]");
        assert_eq!(builder.display(map1232), "[1:1, 2:2, 3:3]");

        // And in reverse order
        let map3 = builder.entry(empty, 3).or_insert_with(|| 3);
        let map32 = builder.entry(map3, 2).or_insert_with(|| 2);
        let map321 = builder.entry(map32, 1).or_insert_with(|| 1);
        let map3212 = builder.entry(map321, 2).or_insert_with(|| 4);
        assert_eq!(builder.display(empty), "[]");
        assert_eq!(builder.display(map3), "[3:3]");
        assert_eq!(builder.display(map32), "[2:2, 3:3]");
        assert_eq!(builder.display(map321), "[1:1, 2:2, 3:3]");
        assert_eq!(builder.display(map3212), "[1:1, 2:2, 3:3]");
    }

    #[test]
    fn can_intersect_maps() {
        let mut builder = ListBuilder::<u16, u16>::default();

        let empty = List::empty();
        let map1 = builder.entry(empty, 1).or_insert_with(|| 1);
        let map12 = builder.entry(map1, 2).or_insert_with(|| 2);
        let map123 = builder.entry(map12, 3).or_insert_with(|| 3);
        let map1234 = builder.entry(map123, 4).or_insert_with(|| 4);

        let map2 = builder.entry(empty, 2).or_insert_with(|| 20);
        let map24 = builder.entry(map2, 4).or_insert_with(|| 40);
        let map245 = builder.entry(map24, 5).or_insert_with(|| 50);
        let map2457 = builder.entry(map245, 7).or_insert_with(|| 70);

        let intersection = builder.intersect_with(empty, empty, |a, b| a + b);
        assert_eq!(builder.display(intersection), "[]");
        let intersection = builder.intersect_with(empty, map1234, |a, b| a + b);
        assert_eq!(builder.display(intersection), "[]");
        let intersection = builder.intersect_with(empty, map2457, |a, b| a + b);
        assert_eq!(builder.display(intersection), "[]");
        let intersection = builder.intersect_with(map1, map1234, |a, b| a + b);
        assert_eq!(builder.display(intersection), "[1:2]");
        let intersection = builder.intersect_with(map1, map2457, |a, b| a + b);
        assert_eq!(builder.display(intersection), "[]");
        let intersection = builder.intersect_with(map2, map1234, |a, b| a + b);
        assert_eq!(builder.display(intersection), "[2:22]");
        let intersection = builder.intersect_with(map2, map2457, |a, b| a + b);
        assert_eq!(builder.display(intersection), "[2:40]");
        let intersection = builder.intersect_with(map1234, map2457, |a, b| a + b);
        assert_eq!(builder.display(intersection), "[2:22, 4:44]");
    }
}

// --------------
// Property tests

#[cfg(test)]
mod property_tests {
    use super::*;

    use std::collections::{BTreeMap, BTreeSet};

    impl<K> ListBuilder<K>
    where
        K: Clone + Ord,
    {
        fn set_from_elements<'a>(&mut self, elements: impl IntoIterator<Item = &'a K>) -> List<K>
        where
            K: 'a,
        {
            let mut set = List::empty();
            for element in elements {
                set = self.insert(set, element.clone());
            }
            set
        }
    }

    // For most of the tests below, we use a vec as our input, instead of a HashSet or BTreeSet,
    // since we want to test the behavior of adding duplicate elements to the set.

    #[quickcheck_macros::quickcheck]
    #[ignore]
    #[allow(clippy::needless_pass_by_value)]
    fn roundtrip_set_from_vec(elements: Vec<u16>) -> bool {
        let mut builder = ListBuilder::default();
        let set = builder.set_from_elements(&elements);
        let expected: BTreeSet<_> = elements.iter().copied().collect();
        let actual = builder.iter_set_reverse(set).copied();
        actual.eq(expected.into_iter().rev())
    }

    #[quickcheck_macros::quickcheck]
    #[ignore]
    #[allow(clippy::needless_pass_by_value)]
    fn roundtrip_set_intersection(a_elements: Vec<u16>, b_elements: Vec<u16>) -> bool {
        let mut builder = ListBuilder::default();
        let a = builder.set_from_elements(&a_elements);
        let b = builder.set_from_elements(&b_elements);
        let intersection = builder.intersect(a, b);
        let a_set: BTreeSet<_> = a_elements.iter().copied().collect();
        let b_set: BTreeSet<_> = b_elements.iter().copied().collect();
        let expected: Vec<_> = a_set.intersection(&b_set).copied().collect();
        let actual = builder.iter_set_reverse(intersection).copied();
        actual.eq(expected.into_iter().rev())
    }

    impl<K, V> ListBuilder<K, V>
    where
        K: Clone + Ord,
        V: Clone + Eq,
    {
        fn set_from_pairs<'a, I>(&mut self, pairs: I) -> List<K, V>
        where
            K: 'a,
            V: 'a,
            I: IntoIterator<Item = &'a (K, V)>,
            I::IntoIter: DoubleEndedIterator,
        {
            let mut list = List::empty();
            for (key, value) in pairs.into_iter().rev() {
                list = self
                    .entry(list, key.clone())
                    .or_insert_with(|| value.clone());
            }
            list
        }
    }

    fn join<K, V>(a: &BTreeMap<K, V>, b: &BTreeMap<K, V>) -> BTreeMap<K, (Option<V>, Option<V>)>
    where
        K: Clone + Ord,
        V: Clone + Ord,
    {
        let mut joined: BTreeMap<K, (Option<V>, Option<V>)> = BTreeMap::new();
        for (k, v) in a {
            joined.entry(k.clone()).or_default().0 = Some(v.clone());
        }
        for (k, v) in b {
            joined.entry(k.clone()).or_default().1 = Some(v.clone());
        }
        joined
    }

    #[quickcheck_macros::quickcheck]
    #[ignore]
    #[allow(clippy::needless_pass_by_value)]
    fn roundtrip_list_from_vec(pairs: Vec<(u16, u16)>) -> bool {
        let mut builder = ListBuilder::default();
        let list = builder.set_from_pairs(&pairs);
        let expected: BTreeMap<_, _> = pairs.iter().copied().collect();
        let actual = builder.iter_reverse(list).map(|(k, v)| (*k, *v));
        actual.eq(expected.into_iter().rev())
    }

    #[quickcheck_macros::quickcheck]
    #[ignore]
    #[allow(clippy::needless_pass_by_value)]
    fn roundtrip_list_intersection(
        a_elements: Vec<(u16, u16)>,
        b_elements: Vec<(u16, u16)>,
    ) -> bool {
        let mut builder = ListBuilder::default();
        let a = builder.set_from_pairs(&a_elements);
        let b = builder.set_from_pairs(&b_elements);
        let intersection = builder.intersect_with(a, b, |a, b| a + b);
        let a_map: BTreeMap<_, _> = a_elements.iter().copied().collect();
        let b_map: BTreeMap<_, _> = b_elements.iter().copied().collect();
        let intersection_map = join(&a_map, &b_map);
        let expected: Vec<_> = intersection_map
            .into_iter()
            .filter_map(|(k, (v1, v2))| Some((k, v1? + v2?)))
            .collect();
        let actual = builder.iter_reverse(intersection).map(|(k, v)| (*k, *v));
        actual.eq(expected.into_iter().rev())
    }
}
