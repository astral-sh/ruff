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
//! typically allocated separately from the heap.
//!
//! We solve that last problem by storing the cells of an association list in an [`IndexVec`]
//! arena. You provide the index type (`I`) that you want to use with this arena. That means that
//! an individual association list is represented by an `Option<I>`, with `None` representing an
//! empty list.
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

use crate::newtype_index;
use crate::vec::IndexVec;

// Allows the macro invocation below to work
use crate as ruff_index;

/// A handle to an association list. Use [`ListStorage`] to access its elements, and
/// [`ListBuilder`] to construct other lists based on this one.
#[derive(Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct List<K, V = ()> {
    last: Option<ListCellId>,
    _phantom_key: PhantomData<K>,
    _phantom_value: PhantomData<V>,
}

impl<K, V> List<K, V> {
    pub const fn empty() -> List<K, V> {
        List::new(None)
    }

    const fn new(last: Option<ListCellId>) -> List<K, V> {
        List {
            last,
            _phantom_key: PhantomData,
            _phantom_value: PhantomData,
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
pub struct ListStorage<K, V = ()> {
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

impl<K, V> ListStorage<K, V> {
    /// Iterates through the entries in a list _in reverse order by key_.
    pub fn iter_reverse(&self, list: &List<K, V>) -> ListReverseIterator<'_, K, V> {
        ListReverseIterator {
            storage: self,
            curr: list.last,
        }
    }
}

pub struct ListReverseIterator<'a, K, V> {
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

/// Constructs one or more association lists.
#[derive(Debug, Eq, PartialEq)]
pub struct ListBuilder<K, V = ()> {
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
    pub fn build(mut self) -> ListStorage<K, V> {
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

    /// Clones an existing list.
    pub fn clone_list(&mut self, list: &List<K, V>) -> List<K, V> {
        List::new(list.last)
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
    pub fn entry(&mut self, list: List<K, V>, key: K) -> ListEntry<K, V>
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
        let mut prev = None;
        let mut curr = list.last;
        while let Some(curr_id) = curr {
            let cell = &self.storage.cells[curr_id];
            match key.cmp(&cell.key) {
                // We found an existing entry in the input list with the desired key.
                Ordering::Equal => {
                    let insertion_point = cell.rest;
                    return ListEntry {
                        builder: self,
                        list,
                        key,
                        existing_cell: curr,
                        insertion_point,
                        predecessor: prev,
                    };
                }
                // The input list does not already contain this key, and this is where we should
                // add it.
                Ordering::Greater => {
                    return ListEntry {
                        builder: self,
                        list,
                        key,
                        existing_cell: None,
                        insertion_point: curr,
                        predecessor: prev,
                    };
                }
                // If this key is in the list, it's further along.
                Ordering::Less => {
                    prev = curr;
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
            existing_cell: None,
            insertion_point: None,
            predecessor: prev,
        }
    }
}

/// A view into a list, indicating where a key would be inserted.
pub struct ListEntry<'a, K, V = ()> {
    builder: &'a mut ListBuilder<K, V>,
    list: List<K, V>,
    key: K,
    /// The cell of the element containing `key`, if there is one.
    existing_cell: Option<ListCellId>,
    /// The cell of the element immediately before where `key` would go in the list.
    insertion_point: Option<ListCellId>,
    /// The cell of the element immediately after where `key` would go in the list, or None if
    /// `key` would go at the end of the list.
    predecessor: Option<ListCellId>,
}

impl<K, V> ListEntry<'_, K, V>
where
    K: Clone,
    V: Clone,
{
    fn stitch_up(self, value: V) -> List<K, V> {
        // Make copies of the keys/values of any cells that will appear after the new element.
        self.builder.scratch.clear();
        if self.predecessor.is_some() {
            let mut curr = self.list.last;
            loop {
                let cell_id = curr.expect("cell should not be empty");
                let cell = &self.builder.cells[cell_id];
                let new_key = cell.key.clone();
                let new_value = cell.value.clone();
                let next = cell.rest;
                self.builder.scratch.push((new_key, new_value));
                if curr == self.predecessor {
                    break;
                } else {
                    curr = next;
                }
            }
        }

        let mut last = self.insertion_point;
        last = self.builder.add_cell(last, self.key, value);
        while let Some((key, value)) = self.builder.scratch.pop() {
            last = self.builder.add_cell(last, key, value);
        }
        List::new(last)
    }

    /// Inserts a new key/value into the list if the key is not already present. If the list
    /// already contains `key`, we return the original list as-is, and do not invoke your closure.
    pub fn or_insert_with<F>(self, f: F) -> List<K, V>
    where
        F: FnOnce() -> V,
    {
        // If the list already contains `key`, we don't need to replace anything, and can
        // return the original list unmodified.
        if self.existing_cell.is_some() {
            return self.list;
        }

        // Otherwise we have to create a new entry and stitch it onto the list.
        self.stitch_up(f())
    }

    /// Inserts a new key/value into the list if the key is not already present. If the list
    /// already contains `key`, we return the original list as-is.
    pub fn or_insert(self, value: V) -> List<K, V> {
        self.or_insert_with(|| value)
    }

    /// Inserts a new key and the default value into the list if the key is not already present. If
    /// the list already contains `key`, we return the original list as-is.
    pub fn or_insert_default(self) -> List<K, V>
    where
        V: Default,
    {
        self.or_insert_with(V::default)
    }

    /// Ensures that the list contains an entry mapping the key to `value`, returning the resulting
    /// list. Overwrites any existing entry with the same key. As an optimization, if the existing
    /// entry has an equal _value_, as well, we return the original list as-is.
    pub fn replace(self, value: V) -> List<K, V>
    where
        V: Eq,
    {
        // As an optimization, if value isn't changed, there's no need to stitch up a new list.
        if let Some(existing) = self.existing_cell {
            let cell = &self.builder.cells[existing];
            if value == cell.value {
                return self.list;
            }
        }

        // Otherwise we have to create a new entry and stitch it onto the list.
        self.stitch_up(value)
    }

    /// Ensures that the list contains an entry mapping the key to the default, returning the
    /// resulting list. Overwrites any existing entry with the same key. As an optimization, if the
    /// existing entry has an equal _value_, as well, we return the original list as-is.
    pub fn replace_with_default(self) -> List<K, V>
    where
        V: Default + Eq,
    {
        self.replace(V::default())
    }
}

impl<K, V> ListBuilder<K, V> {
    /// Returns the intersection of two lists. The result will contain an entry for any key that
    /// appears in both lists. The corresponding values will be combined using the `combine`
    /// function that you provide.
    #[allow(clippy::needless_pass_by_value)]
    pub fn intersect_with<F>(&mut self, a: List<K, V>, b: List<K, V>, mut combine: F) -> List<K, V>
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

    /// Returns the union of two lists. The result will contain an entry for any key that appears
    /// in either list. For keys that appear in both lists, the corresponding values will be
    /// combined using the `combine` function that you provide.
    #[allow(clippy::needless_pass_by_value)]
    pub fn union_with<F>(&mut self, a: List<K, V>, b: List<K, V>, mut combine: F) -> List<K, V>
    where
        K: Clone + Ord,
        V: Clone,
        F: FnMut(&V, &V) -> V,
    {
        self.scratch.clear();

        // Zip through the lists, building up the keys/values of the new entries into our scratch
        // vector. Continue until we run out of elements in either list. (Any remaining elements in
        // the other list will be added to the result, but won't need to be combined with
        // anything.)
        let mut a = a.last;
        let mut b = b.last;
        let mut last = loop {
            let (a_id, b_id) = match (a, b) {
                // If we run out of elements in one of the lists, the non-empty list will appear in
                // the output unchanged.
                (None, other) | (other, None) => break other,
                (Some(a_id), Some(b_id)) => (a_id, b_id),
            };

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
                // a's key goes into the result next
                Ordering::Greater => {
                    let new_key = a_cell.key.clone();
                    let new_value = a_cell.value.clone();
                    self.scratch.push((new_key, new_value));
                    a = a_cell.rest;
                }
                // b's key goes into the result next
                Ordering::Less => {
                    let new_key = b_cell.key.clone();
                    let new_value = b_cell.value.clone();
                    self.scratch.push((new_key, new_value));
                    b = b_cell.rest;
                }
            }
        };

        // Once the iteration loop terminates, we stitch the new entries back together into proper
        // alist cells.
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
    pub fn iter_set_reverse(&self, set: &List<K, ()>) -> ListSetReverseIterator<K> {
        ListSetReverseIterator {
            storage: self,
            curr: set.last,
        }
    }
}

pub struct ListSetReverseIterator<'a, K> {
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
    pub fn insert(&mut self, set: List<K, ()>, element: K) -> List<K, ()>
    where
        K: Clone + Ord,
    {
        self.entry(set, element).or_insert_default()
    }

    /// Returns the intersection of two sets. The result will contain any value that appears in
    /// both sets.
    pub fn intersect(&mut self, a: List<K, ()>, b: List<K, ()>) -> List<K, ()>
    where
        K: Clone + Ord,
    {
        self.intersect_with(a, b, |(), ()| ())
    }

    /// Returns the intersection of two sets. The result will contain any value that appears in
    /// either set.
    pub fn union(&mut self, a: List<K, ()>, b: List<K, ()>) -> List<K, ()>
    where
        K: Clone + Ord,
    {
        self.union_with(a, b, |(), ()| ())
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
        fn display_set(&self, list: &List<K, ()>) -> String {
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

    fn insert(builder: &mut ListBuilder<u16>, list: &List<u16>, element: u16) -> List<u16> {
        let list = builder.clone_list(list);
        builder.insert(list, element)
    }

    #[test]
    fn can_insert_into_set() {
        let mut builder = ListBuilder::<u16>::default();

        // Build up the set in order
        let empty = List::empty();
        let set1 = insert(&mut builder, &empty, 1);
        let set12 = insert(&mut builder, &set1, 2);
        let set123 = insert(&mut builder, &set12, 3);
        let set1232 = insert(&mut builder, &set123, 2);
        assert_eq!(builder.display_set(&empty), "[]");
        assert_eq!(builder.display_set(&set1), "[1]");
        assert_eq!(builder.display_set(&set12), "[1, 2]");
        assert_eq!(builder.display_set(&set123), "[1, 2, 3]");
        assert_eq!(builder.display_set(&set1232), "[1, 2, 3]");

        // And in reverse order
        let set3 = insert(&mut builder, &empty, 3);
        let set32 = insert(&mut builder, &set3, 2);
        let set321 = insert(&mut builder, &set32, 1);
        let set3212 = insert(&mut builder, &set321, 2);
        assert_eq!(builder.display_set(&empty), "[]");
        assert_eq!(builder.display_set(&set3), "[3]");
        assert_eq!(builder.display_set(&set32), "[2, 3]");
        assert_eq!(builder.display_set(&set321), "[1, 2, 3]");
        assert_eq!(builder.display_set(&set3212), "[1, 2, 3]");
    }

    #[test]
    fn can_intersect_sets() {
        let mut builder = ListBuilder::<u16>::default();

        let empty = List::empty();
        let set1 = insert(&mut builder, &empty, 1);
        let set12 = insert(&mut builder, &set1, 2);
        let set123 = insert(&mut builder, &set12, 3);
        let set1234 = insert(&mut builder, &set123, 4);

        let set2 = insert(&mut builder, &empty, 2);
        let set24 = insert(&mut builder, &set2, 4);
        let set245 = insert(&mut builder, &set24, 5);
        let set2457 = insert(&mut builder, &set245, 7);

        #[allow(clippy::items_after_statements)]
        fn intersect(builder: &mut ListBuilder<u16>, a: &List<u16>, b: &List<u16>) -> List<u16> {
            let a = builder.clone_list(a);
            let b = builder.clone_list(b);
            builder.intersect(a, b)
        }

        let result = intersect(&mut builder, &empty, &empty);
        assert_eq!(builder.display_set(&result), "[]");
        let result = intersect(&mut builder, &empty, &set1234);
        assert_eq!(builder.display_set(&result), "[]");
        let result = intersect(&mut builder, &empty, &set2457);
        assert_eq!(builder.display_set(&result), "[]");
        let result = intersect(&mut builder, &set1, &set1234);
        assert_eq!(builder.display_set(&result), "[1]");
        let result = intersect(&mut builder, &set1, &set2457);
        assert_eq!(builder.display_set(&result), "[]");
        let result = intersect(&mut builder, &set2, &set1234);
        assert_eq!(builder.display_set(&result), "[2]");
        let result = intersect(&mut builder, &set2, &set2457);
        assert_eq!(builder.display_set(&result), "[2]");
        let result = intersect(&mut builder, &set1234, &set2457);
        assert_eq!(builder.display_set(&result), "[2, 4]");
    }

    #[test]
    fn can_union_sets() {
        let mut builder = ListBuilder::<u16>::default();

        let empty = List::empty();
        let set1 = insert(&mut builder, &empty, 1);
        let set12 = insert(&mut builder, &set1, 2);
        let set123 = insert(&mut builder, &set12, 3);
        let set1234 = insert(&mut builder, &set123, 4);

        let set2 = insert(&mut builder, &empty, 2);
        let set24 = insert(&mut builder, &set2, 4);
        let set245 = insert(&mut builder, &set24, 5);
        let set2457 = insert(&mut builder, &set245, 7);

        #[allow(clippy::items_after_statements)]
        fn union(builder: &mut ListBuilder<u16>, a: &List<u16>, b: &List<u16>) -> List<u16> {
            let a = builder.clone_list(a);
            let b = builder.clone_list(b);
            builder.union(a, b)
        }

        let result = union(&mut builder, &empty, &empty);
        assert_eq!(builder.display_set(&result), "[]");
        let result = union(&mut builder, &empty, &set1234);
        assert_eq!(builder.display_set(&result), "[1, 2, 3, 4]");
        let result = union(&mut builder, &empty, &set2457);
        assert_eq!(builder.display_set(&result), "[2, 4, 5, 7]");
        let result = union(&mut builder, &set1, &set1234);
        assert_eq!(builder.display_set(&result), "[1, 2, 3, 4]");
        let result = union(&mut builder, &set1, &set2457);
        assert_eq!(builder.display_set(&result), "[1, 2, 4, 5, 7]");
        let result = union(&mut builder, &set2, &set1234);
        assert_eq!(builder.display_set(&result), "[1, 2, 3, 4]");
        let result = union(&mut builder, &set2, &set2457);
        assert_eq!(builder.display_set(&result), "[2, 4, 5, 7]");
        let result = union(&mut builder, &set1234, &set2457);
        assert_eq!(builder.display_set(&result), "[1, 2, 3, 4, 5, 7]");
    }

    // ----
    // Maps

    impl<K, V> ListStorage<K, V>
    where
        K: Display,
        V: Display,
    {
        fn display(&self, list: &List<K, V>) -> String {
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

    fn entry<'a>(
        builder: &'a mut ListBuilder<u16, u16>,
        list: &List<u16, u16>,
        key: u16,
    ) -> ListEntry<'a, u16, u16> {
        let list = builder.clone_list(list);
        builder.entry(list, key)
    }

    #[test]
    fn can_insert_into_map() {
        let mut builder = ListBuilder::<u16, u16>::default();

        // Build up the map in order
        let empty = List::empty();
        let map1 = entry(&mut builder, &empty, 1).replace(1);
        let map12 = entry(&mut builder, &map1, 2).replace(2);
        let map123 = entry(&mut builder, &map12, 3).replace(3);
        let map1232 = entry(&mut builder, &map123, 2).replace(4);
        assert_eq!(builder.display(&empty), "[]");
        assert_eq!(builder.display(&map1), "[1:1]");
        assert_eq!(builder.display(&map12), "[1:1, 2:2]");
        assert_eq!(builder.display(&map123), "[1:1, 2:2, 3:3]");
        assert_eq!(builder.display(&map1232), "[1:1, 2:4, 3:3]");

        // And in reverse order
        let map3 = entry(&mut builder, &empty, 3).replace(3);
        let map32 = entry(&mut builder, &map3, 2).replace(2);
        let map321 = entry(&mut builder, &map32, 1).replace(1);
        let map3212 = entry(&mut builder, &map321, 2).replace(4);
        assert_eq!(builder.display(&empty), "[]");
        assert_eq!(builder.display(&map3), "[3:3]");
        assert_eq!(builder.display(&map32), "[2:2, 3:3]");
        assert_eq!(builder.display(&map321), "[1:1, 2:2, 3:3]");
        assert_eq!(builder.display(&map3212), "[1:1, 2:4, 3:3]");
    }

    #[test]
    fn can_insert_if_needed_into_map() {
        let mut builder = ListBuilder::<u16, u16>::default();

        // Build up the map in order
        let empty = List::empty();
        let map1 = entry(&mut builder, &empty, 1).or_insert(1);
        let map12 = entry(&mut builder, &map1, 2).or_insert(2);
        let map123 = entry(&mut builder, &map12, 3).or_insert(3);
        let map1232 = entry(&mut builder, &map123, 2).or_insert(4);
        assert_eq!(builder.display(&empty), "[]");
        assert_eq!(builder.display(&map1), "[1:1]");
        assert_eq!(builder.display(&map12), "[1:1, 2:2]");
        assert_eq!(builder.display(&map123), "[1:1, 2:2, 3:3]");
        assert_eq!(builder.display(&map1232), "[1:1, 2:2, 3:3]");

        // And in reverse order
        let map3 = entry(&mut builder, &empty, 3).or_insert(3);
        let map32 = entry(&mut builder, &map3, 2).or_insert(2);
        let map321 = entry(&mut builder, &map32, 1).or_insert(1);
        let map3212 = entry(&mut builder, &map321, 2).or_insert(4);
        assert_eq!(builder.display(&empty), "[]");
        assert_eq!(builder.display(&map3), "[3:3]");
        assert_eq!(builder.display(&map32), "[2:2, 3:3]");
        assert_eq!(builder.display(&map321), "[1:1, 2:2, 3:3]");
        assert_eq!(builder.display(&map3212), "[1:1, 2:2, 3:3]");
    }

    #[test]
    fn can_intersect_maps() {
        let mut builder = ListBuilder::<u16, u16>::default();

        let empty = List::empty();
        let map1 = entry(&mut builder, &empty, 1).or_insert(1);
        let map12 = entry(&mut builder, &map1, 2).or_insert(2);
        let map123 = entry(&mut builder, &map12, 3).or_insert(3);
        let map1234 = entry(&mut builder, &map123, 4).or_insert(4);

        let map2 = entry(&mut builder, &empty, 2).or_insert(20);
        let map24 = entry(&mut builder, &map2, 4).or_insert(40);
        let map245 = entry(&mut builder, &map24, 5).or_insert(50);
        let map2457 = entry(&mut builder, &map245, 7).or_insert(70);

        #[allow(clippy::items_after_statements)]
        fn intersect(
            builder: &mut ListBuilder<u16, u16>,
            a: &List<u16, u16>,
            b: &List<u16, u16>,
        ) -> List<u16, u16> {
            let a = builder.clone_list(a);
            let b = builder.clone_list(b);
            builder.intersect_with(a, b, |a, b| a + b)
        }

        let result = intersect(&mut builder, &empty, &empty);
        assert_eq!(builder.display(&result), "[]");
        let result = intersect(&mut builder, &empty, &map1234);
        assert_eq!(builder.display(&result), "[]");
        let result = intersect(&mut builder, &empty, &map2457);
        assert_eq!(builder.display(&result), "[]");
        let result = intersect(&mut builder, &map1, &map1234);
        assert_eq!(builder.display(&result), "[1:2]");
        let result = intersect(&mut builder, &map1, &map2457);
        assert_eq!(builder.display(&result), "[]");
        let result = intersect(&mut builder, &map2, &map1234);
        assert_eq!(builder.display(&result), "[2:22]");
        let result = intersect(&mut builder, &map2, &map2457);
        assert_eq!(builder.display(&result), "[2:40]");
        let result = intersect(&mut builder, &map1234, &map2457);
        assert_eq!(builder.display(&result), "[2:22, 4:44]");
    }

    #[test]
    fn can_union_maps() {
        let mut builder = ListBuilder::<u16, u16>::default();

        let empty = List::empty();
        let map1 = entry(&mut builder, &empty, 1).or_insert(1);
        let map12 = entry(&mut builder, &map1, 2).or_insert(2);
        let map123 = entry(&mut builder, &map12, 3).or_insert(3);
        let map1234 = entry(&mut builder, &map123, 4).or_insert(4);

        let map2 = entry(&mut builder, &empty, 2).or_insert(20);
        let map24 = entry(&mut builder, &map2, 4).or_insert(40);
        let map245 = entry(&mut builder, &map24, 5).or_insert(50);
        let map2457 = entry(&mut builder, &map245, 7).or_insert(70);

        #[allow(clippy::items_after_statements)]
        fn union(
            builder: &mut ListBuilder<u16, u16>,
            a: &List<u16, u16>,
            b: &List<u16, u16>,
        ) -> List<u16, u16> {
            let a = builder.clone_list(a);
            let b = builder.clone_list(b);
            builder.union_with(a, b, |a, b| a + b)
        }

        let result = union(&mut builder, &empty, &empty);
        assert_eq!(builder.display(&result), "[]");
        let result = union(&mut builder, &empty, &map1234);
        assert_eq!(builder.display(&result), "[1:1, 2:2, 3:3, 4:4]");
        let result = union(&mut builder, &empty, &map2457);
        assert_eq!(builder.display(&result), "[2:20, 4:40, 5:50, 7:70]");
        let result = union(&mut builder, &map1, &map1234);
        assert_eq!(builder.display(&result), "[1:2, 2:2, 3:3, 4:4]");
        let result = union(&mut builder, &map1, &map2457);
        assert_eq!(builder.display(&result), "[1:1, 2:20, 4:40, 5:50, 7:70]");
        let result = union(&mut builder, &map2, &map1234);
        assert_eq!(builder.display(&result), "[1:1, 2:22, 3:3, 4:4]");
        let result = union(&mut builder, &map2, &map2457);
        assert_eq!(builder.display(&result), "[2:40, 4:40, 5:50, 7:70]");
        let result = union(&mut builder, &map1234, &map2457);
        assert_eq!(
            builder.display(&result),
            "[1:1, 2:22, 3:3, 4:44, 5:50, 7:70]"
        );
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
        fn append_from_elements<'a>(
            &mut self,
            rest: List<K>,
            elements: impl IntoIterator<Item = &'a K>,
        ) -> List<K>
        where
            K: 'a,
        {
            let mut set = rest;
            for element in elements {
                set = self.insert(set, element.clone());
            }
            set
        }

        fn set_from_elements<'a>(&mut self, elements: impl IntoIterator<Item = &'a K>) -> List<K>
        where
            K: 'a,
        {
            self.append_from_elements(List::empty(), elements)
        }
    }

    // For most of the tests below, we use a vec as our input, instead of a HashSet or BTreeSet,
    // since we want to test the behavior of adding duplicate elements to the set.

    #[quickcheck_macros::quickcheck]
    #[ignore]
    fn roundtrip_set_from_vec(elements: Vec<u16>) -> bool {
        let mut builder = ListBuilder::default();
        let set = builder.set_from_elements(&elements);
        let expected: BTreeSet<_> = elements.iter().copied().collect();
        let actual = builder.iter_set_reverse(&set).copied();
        actual.eq(expected.into_iter().rev())
    }

    #[quickcheck_macros::quickcheck]
    #[ignore]
    fn roundtrip_shared_sets(a_elements: Vec<u16>, b_elements: Vec<u16>) -> bool {
        // Create sets for `a` and `a ∪ b` in a way that induces structural sharing between the
        // two.
        let mut builder = ListBuilder::default();
        let a = builder.set_from_elements(&a_elements);
        let a_copy = builder.clone_list(&a);
        let union = builder.append_from_elements(a_copy, &b_elements);

        // Verify that the structural sharing did not change the contents of either set.
        let a_expected: BTreeSet<_> = a_elements.iter().copied().collect();
        let a_actual = builder.iter_set_reverse(&a).copied();
        let union_expected: BTreeSet<_> = a_elements
            .iter()
            .copied()
            .chain(b_elements.iter().copied())
            .collect();
        let union_actual = builder.iter_set_reverse(&union).copied();
        a_actual.eq(a_expected.into_iter().rev())
            && union_actual.eq(union_expected.into_iter().rev())
    }

    #[quickcheck_macros::quickcheck]
    #[ignore]
    fn roundtrip_set_intersection(a_elements: Vec<u16>, b_elements: Vec<u16>) -> bool {
        let mut builder = ListBuilder::default();
        let a = builder.set_from_elements(&a_elements);
        let b = builder.set_from_elements(&b_elements);
        let intersection = builder.intersect(a, b);
        let a_set: BTreeSet<_> = a_elements.iter().copied().collect();
        let b_set: BTreeSet<_> = b_elements.iter().copied().collect();
        let expected: Vec<_> = a_set.intersection(&b_set).copied().collect();
        let actual = builder.iter_set_reverse(&intersection).copied();
        actual.eq(expected.into_iter().rev())
    }

    #[quickcheck_macros::quickcheck]
    #[ignore]
    fn roundtrip_set_union(a_elements: Vec<u16>, b_elements: Vec<u16>) -> bool {
        let mut builder = ListBuilder::default();
        let a = builder.set_from_elements(&a_elements);
        let b = builder.set_from_elements(&b_elements);
        let union = builder.union(a, b);
        let a_set: BTreeSet<_> = a_elements.iter().copied().collect();
        let b_set: BTreeSet<_> = b_elements.iter().copied().collect();
        let expected: Vec<_> = a_set.union(&b_set).copied().collect();
        let actual = builder.iter_set_reverse(&union).copied();
        actual.eq(expected.into_iter().rev())
    }

    impl<K, V> ListBuilder<K, V>
    where
        K: Clone + Ord,
        V: Clone + Eq,
    {
        fn append_from_pairs<'a>(
            &mut self,
            rest: List<K, V>,
            pairs: impl IntoIterator<Item = &'a (K, V)>,
        ) -> List<K, V>
        where
            K: 'a,
            V: 'a,
        {
            let mut list = rest;
            for (key, value) in pairs {
                list = self.entry(list, key.clone()).replace(value.clone());
            }
            list
        }

        fn list_from_pairs<'a>(&mut self, pairs: impl IntoIterator<Item = &'a (K, V)>) -> List<K, V>
        where
            K: 'a,
            V: 'a,
        {
            self.append_from_pairs(List::empty(), pairs)
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
    fn roundtrip_list_from_vec(pairs: Vec<(u16, u16)>) -> bool {
        let mut builder = ListBuilder::default();
        let list = builder.list_from_pairs(&pairs);
        let expected: BTreeMap<_, _> = pairs.iter().copied().collect();
        let actual = builder.iter_reverse(&list).map(|(k, v)| (*k, *v));
        actual.eq(expected.into_iter().rev())
    }

    #[quickcheck_macros::quickcheck]
    #[ignore]
    fn roundtrip_shared_lists(a_pairs: Vec<(u16, u16)>, b_pairs: Vec<(u16, u16)>) -> bool {
        // Create lists for `a` and `a ∪ b` in a way that induces structural sharing between the
        // two.
        let mut builder = ListBuilder::default();
        let a = builder.list_from_pairs(&a_pairs);
        let a_copy = builder.clone_list(&a);
        let union = builder.append_from_pairs(a_copy, &b_pairs);

        // Verify that the structural sharing did not change the contents of either list.
        let a_expected: BTreeMap<_, _> = a_pairs.iter().copied().collect();
        let a_actual = builder.iter_reverse(&a).map(|(k, v)| (*k, *v));
        let union_expected: BTreeMap<_, _> = a_pairs
            .iter()
            .copied()
            .chain(b_pairs.iter().copied())
            .collect();
        let union_actual = builder.iter_reverse(&union).map(|(k, v)| (*k, *v));
        a_actual.eq(a_expected.into_iter().rev())
            && union_actual.eq(union_expected.into_iter().rev())
    }

    #[quickcheck_macros::quickcheck]
    #[ignore]
    fn roundtrip_list_intersection(
        a_elements: Vec<(u16, u16)>,
        b_elements: Vec<(u16, u16)>,
    ) -> bool {
        let mut builder = ListBuilder::default();
        let a = builder.list_from_pairs(&a_elements);
        let b = builder.list_from_pairs(&b_elements);
        let intersection = builder.intersect_with(a, b, |a, b| a + b);
        let a_map: BTreeMap<_, _> = a_elements.iter().copied().collect();
        let b_map: BTreeMap<_, _> = b_elements.iter().copied().collect();
        let intersection_map = join(&a_map, &b_map);
        let expected: Vec<_> = intersection_map
            .into_iter()
            .filter_map(|(k, (v1, v2))| Some((k, v1? + v2?)))
            .collect();
        let actual = builder.iter_reverse(&intersection).map(|(k, v)| (*k, *v));
        actual.eq(expected.into_iter().rev())
    }

    #[quickcheck_macros::quickcheck]
    #[ignore]
    fn roundtrip_list_union(a_elements: Vec<(u16, u16)>, b_elements: Vec<(u16, u16)>) -> bool {
        let mut builder = ListBuilder::default();
        let a = builder.list_from_pairs(&a_elements);
        let b = builder.list_from_pairs(&b_elements);
        let union = builder.union_with(a, b, |a, b| a + b);
        let a_map: BTreeMap<_, _> = a_elements.iter().copied().collect();
        let b_map: BTreeMap<_, _> = b_elements.iter().copied().collect();
        let union_map = join(&a_map, &b_map);
        let expected: Vec<_> = union_map
            .into_iter()
            .map(|(k, (v1, v2))| (k, v1.unwrap_or_default() + v2.unwrap_or_default()))
            .collect();
        let actual = builder.iter_reverse(&union).map(|(k, v)| (*k, *v));
        actual.eq(expected.into_iter().rev())
    }
}
