use std::cmp::Ordering;
use std::ops::Deref;

use crate::vec::IndexVec;
use crate::Idx;

/// Stores one or more _association lists_, which are linked lists of key/value pairs. We
/// additionally guarantee that the elements of an association list are sorted (by their keys), and
/// that they do not contain any entries with duplicate keys.
///
/// Association lists have fallen out of favor in recent decades, since you often need operations
/// that are inefficient on them. In particular, looking up a random element by index is O(n), just
/// like a linked list; and looking up an element by key is also O(n), since you must do a linear
/// scan of the list to find the matching element. The typical implementation also suffers from
/// poor cache locality and high memory allocation overhead, since individual list cells are
/// typically allocated separately from the heap.
///
/// We solve that last problem by storing the cells of an association list in an [`IndexVec`]
/// arena. You provide the index type (`I`) that you want to use with this arena. That means that
/// an individual association list is represented by an `Option<I>`, with `None` representing an
/// empty list.
///
/// We exploit structural sharing where possible, reusing cells across multiple lists when we can.
/// That said, we don't guarantee that lists are canonical — it's entirely possible for two lists
/// with identical contents to use different list cells and have different identifiers.
///
/// Given all of this, association lists have the following benefits:
///
/// - Lists can be represented by a single 32-bit integer (the index into the arena of the head of
///   the list).
/// - Lists can be cloned in constant time, since the underlying cells are immutable.
/// - Lists can be combined quickly (for both intersection and union), especially when you already
///   have to zip through both input lists to combine each key's values in some way.
///
/// There is one remaining caveat:
///
/// - You should construct lists in key order; doing lets you insert each value in constant time.
///   Inserting entries in reverse order results in _quadratic_ overall time to construct the list.
///
/// This type provides read-only access to the lists.  Use a [`ListBuilder`] to create lists.
#[derive(Debug, Eq, PartialEq)]
pub struct ListStorage<I, K, V = ()> {
    cells: IndexVec<I, ListCell<I, K, V>>,
}

/// Each association list is represented by a sequence of snoc cells. A snoc cell is like the more
/// familiar cons cell `(a : (b : (c : nil)))`, but in reverse `(((nil : a) : b) : c)`.
///
/// **Terminology**: The elements of a cons cell are usually called `head` and `tail` (assuming
/// you're not in Lisp-land, where they're called `car` and `cdr`).  The elements of a snoc cell
/// are usually called `rest` and `last`.
///
/// We use a tuple struct instead of named fields because we always unpack a cell into local
/// variables:
///
/// ```no_run
/// let ListCell(rest, last_key, last_value) = /* ... */;
/// ```
#[derive(Debug, Eq, PartialEq)]
struct ListCell<I, K, V>(Option<I>, K, V);

impl<I: Idx, K, V> ListStorage<I, K, V> {
    /// Iterates through the entries in a list _in reverse order by key_.
    pub fn iter_reverse(&self, list: Option<I>) -> ListReverseIterator<'_, I, K, V> {
        ListReverseIterator {
            storage: self,
            curr: list,
        }
    }

    /// Finds the entry in a list with a given key, and returns its value.
    ///
    /// **Performance**: Note that lookups are O(n), since we use a linked-list representation!
    pub fn get(&self, list: Option<I>, key: &K) -> Option<&V>
    where
        K: Ord,
    {
        self.iter_reverse(list)
            .find(|(k, _)| *k == key)
            .map(|(_, v)| v)
    }
}

pub struct ListReverseIterator<'a, I, K, V> {
    storage: &'a ListStorage<I, K, V>,
    curr: Option<I>,
}

impl<'a, I: Idx, K, V> Iterator for ListReverseIterator<'a, I, K, V> {
    type Item = (&'a K, &'a V);

    fn next(&mut self) -> Option<Self::Item> {
        let ListCell(rest, key, value) = &self.storage.cells[self.curr?];
        self.curr = *rest;
        Some((key, value))
    }
}

/// Constructs one or more association lists.
#[derive(Debug, Eq, PartialEq)]
pub struct ListBuilder<I, K, V = ()> {
    storage: ListStorage<I, K, V>,

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
    /// beginning of whatever result list we are creating. For our fix-up step, we can consume a
    /// Vec in reverse order by `pop`ping the elements off one by one.
    scratch: Vec<(K, V)>,
}

impl<I: Idx, K, V> Default for ListBuilder<I, K, V> {
    fn default() -> Self {
        ListBuilder {
            storage: ListStorage {
                cells: IndexVec::default(),
            },
            scratch: Vec::default(),
        }
    }
}

impl<I, K, V> Deref for ListBuilder<I, K, V> {
    type Target = ListStorage<I, K, V>;
    fn deref(&self) -> &ListStorage<I, K, V> {
        &self.storage
    }
}

impl<I: Idx, K, V> ListBuilder<I, K, V> {
    /// Finalizes a `ListBuilder`. After calling this, you cannot create any new lists managed by
    /// this storage.
    pub fn build(mut self) -> ListStorage<I, K, V> {
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
    fn add_cell(&mut self, rest: Option<I>, key: K, value: V) -> Option<I> {
        Some(self.storage.cells.push(ListCell(rest, key, value)))
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
    pub fn entry(&mut self, list: Option<I>, key: K) -> ListEntry<I, K, V>
    where
        K: Clone + Ord,
        V: Clone,
    {
        self.scratch.clear();

        // Iterate through the input list, looking for the position where the key should be
        // inserted. We will need to create new list cells for any elements that appear before the
        // new key. Stash those away in our scratch accumulator as we step through the input. The
        // result of the loop is that "tail" of the result list, which we will stitch the new key
        // (and any preceding keys) onto.
        let mut curr = list;
        while let Some(curr_id) = curr {
            let ListCell(rest, curr_key, curr_value) = &self.storage.cells[curr_id];
            match key.cmp(curr_key) {
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
                // for this entry into the result list, so add its contents to the scratch
                // accumulator.
                Ordering::Less => {
                    let new_key = curr_key.clone();
                    let new_value = curr_value.clone();
                    self.scratch.push((new_key, new_value));
                    curr = *rest;
                }
            }
        }

        // We made it all the way through the list without finding the desired key.
        ListEntry {
            builder: self,
            list,
            key,
            rest: ListTail::Beginning,
        }
    }
}

/// A view into a list, indicating where a key would be inserted.
pub struct ListEntry<'a, I, K, V> {
    builder: &'a mut ListBuilder<I, K, V>,
    list: Option<I>,
    key: K,
    /// Points at the element that already contains `key`, if there is one, or the element
    /// immediately before where it would go, if not.
    rest: ListTail<I>,
}

enum ListTail<I> {
    /// The list does not already contain `key`, and it would go at the beginning of the list.
    Beginning,
    /// The list already contains `key`
    Occupied(I),
    /// The list does not already contain key
    Vacant(I),
}

impl<I: Idx, K, V> ListEntry<'_, I, K, V>
where
    K: Clone + Ord,
    V: Clone,
{
    fn stitch_up(self, rest: Option<I>, value: V) -> Option<I> {
        let mut result = rest;
        result = self.builder.add_cell(result, self.key, value);
        while let Some((key, value)) = self.builder.scratch.pop() {
            result = self.builder.add_cell(result, key, value);
        }
        result
    }

    /// Inserts a new key/value into the list if the key is not already present. If the list
    /// already contains `key`, we return the original list as-is, and do not invoke your closure.
    pub fn or_insert_with<F>(self, f: F) -> Option<I>
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

    /// Inserts a new key/value into the list if the key is not already present.
    pub fn or_insert(self, value: V) -> Option<I> {
        self.or_insert_with(|| value)
    }

    /// Inserts a new key and the default value into the list if the key is not already present.
    pub fn or_insert_default(self) -> Option<I>
    where
        V: Default,
    {
        self.or_insert_with(V::default)
    }

    /// Sets the value of the entry, returning the resulting list. Overwrites any existing entry
    /// with the same key
    pub fn replace(self, value: V) -> Option<I>
    where
        V: Eq,
    {
        // If the list already contains `key`, skip past its entry before we add its replacement.
        let rest = match self.rest {
            ListTail::Beginning => None,
            ListTail::Occupied(index) => {
                let ListCell(rest, _, existing_value) = &self.builder.cells[index];
                if value == *existing_value {
                    // As an optimization, if value isn't changed, there's no need to stitch up a
                    // new list.
                    return self.list;
                }
                *rest
            }
            ListTail::Vacant(index) => Some(index),
        };
        self.stitch_up(rest, value)
    }

    /// Sets the value of the entry to the default value, returning the resulting list. Overwrites
    /// any existing entry with the same key
    pub fn replace_with_default(self) -> Option<I>
    where
        V: Default + Eq,
    {
        self.replace(V::default())
    }
}

impl<I: Idx, K, V> ListBuilder<I, K, V> {
    /// Returns the intersection of two lists. The result will contain an entry for any key that
    /// appears in both lists. The corresponding values will be combined using the `combine`
    /// function.
    pub fn intersect_with<F>(
        &mut self,
        mut a: Option<I>,
        mut b: Option<I>,
        mut combine: F,
    ) -> Option<I>
    where
        K: Clone + Ord,
        V: Clone,
        F: FnMut(&V, &V) -> V,
    {
        self.scratch.clear();

        // Zip through the lists, building up the keys/values of the new entries into our scratch
        // vector. Continue until we run out of elements in either list. (Any remaining elements in
        // the other list cannot possibly be in the intersection.)
        while let (Some(a_id), Some(b_id)) = (a, b) {
            let ListCell(a_rest, a_key, a_value) = &self.storage.cells[a_id];
            let ListCell(b_rest, b_key, b_value) = &self.storage.cells[b_id];
            match a_key.cmp(b_key) {
                // Both lists contain this key; combine their values
                Ordering::Equal => {
                    let new_key = a_key.clone();
                    let new_value = combine(a_value, b_value);
                    self.scratch.push((new_key, new_value));
                    a = *a_rest;
                    b = *b_rest;
                }
                // a's key is only present in a, so it's not included in the result.
                Ordering::Greater => a = *a_rest,
                // b's key is only present in b, so it's not included in the result.
                Ordering::Less => b = *b_rest,
            }
        }

        // Once the iteration loop terminates, we stitch the new entries back together into proper
        // alist cells.
        let mut result = None;
        while let Some((key, value)) = self.scratch.pop() {
            result = self.add_cell(result, key, value);
        }
        result
    }

    /// Returns the union of two lists. The result will contain an entry for any key that appears
    /// in either list. For keys that appear in both lists, the corresponding values will be
    /// combined using the `combine` function.
    pub fn union_with<F>(&mut self, mut a: Option<I>, mut b: Option<I>, mut combine: F) -> Option<I>
    where
        K: Clone + Ord,
        V: Clone,
        F: FnMut(&V, &V) -> V,
    {
        self.scratch.clear();

        // Zip through the lists, building up the keys/values of the new entries into our scratch
        // vector. Continue until we run out of elements in either list. (Any remaining elements in
        // the other list cannot possibly be in the intersection.)
        let mut result = loop {
            let (a_id, b_id) = match (a, b) {
                // If we run out of elements in one of the lists, the non-empty list will appear in
                // the output unchanged.
                (None, other) | (other, None) => break other,
                (Some(a_id), Some(b_id)) => (a_id, b_id),
            };

            let ListCell(a_rest, a_key, a_value) = &self.storage.cells[a_id];
            let ListCell(b_rest, b_key, b_value) = &self.storage.cells[b_id];
            match a_key.cmp(b_key) {
                // Both lists contain this key; combine their values
                Ordering::Equal => {
                    let new_key = a_key.clone();
                    let new_value = combine(a_value, b_value);
                    self.scratch.push((new_key, new_value));
                    a = *a_rest;
                    b = *b_rest;
                }
                // a's key is lower, so it goes into the result next
                Ordering::Greater => {
                    let new_key = a_key.clone();
                    let new_value = a_value.clone();
                    self.scratch.push((new_key, new_value));
                    a = *a_rest;
                }
                // b's key is lower, so it goes into the result next
                Ordering::Less => {
                    let new_key = b_key.clone();
                    let new_value = b_value.clone();
                    self.scratch.push((new_key, new_value));
                    b = *b_rest;
                }
            }
        };

        // Once the iteration loop terminates, we stitch the new entries back together into proper
        // alist cells.
        while let Some((key, value)) = self.scratch.pop() {
            result = self.add_cell(result, key, value);
        }
        result
    }
}

// ----
// Sets

impl<I: Idx, K> ListStorage<I, K, ()> {
    /// Iterates through the elements in a set _in reverse order_.
    pub fn iter_set_reverse(&self, set: Option<I>) -> ListSetReverseIterator<'_, I, K> {
        ListSetReverseIterator {
            storage: self,
            curr: set,
        }
    }
}

pub struct ListSetReverseIterator<'a, I, K> {
    storage: &'a ListStorage<I, K, ()>,
    curr: Option<I>,
}

impl<'a, I: Idx, K> Iterator for ListSetReverseIterator<'a, I, K> {
    type Item = &'a K;

    fn next(&mut self) -> Option<Self::Item> {
        let ListCell(rest, key, ()) = &self.storage.cells[self.curr?];
        self.curr = *rest;
        Some(key)
    }
}

impl<I: Idx, K> ListBuilder<I, K, ()> {
    /// Adds an element to a set.
    pub fn insert(&mut self, set: Option<I>, element: K) -> Option<I>
    where
        K: Clone + Ord,
    {
        self.entry(set, element).or_insert_default()
    }

    /// Returns the intersection of two sets. The result will contain any value that appears in
    /// both sets.
    pub fn intersect(&mut self, a: Option<I>, b: Option<I>) -> Option<I>
    where
        K: Clone + Ord,
    {
        self.intersect_with(a, b, |(), ()| ())
    }

    /// Returns the intersection of two sets. The result will contain any value that appears in
    /// either set.
    pub fn union(&mut self, a: Option<I>, b: Option<I>) -> Option<I>
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

    use crate::newtype_index;

    // Allows the macro invocation below to work
    use crate as ruff_index;

    #[newtype_index]
    struct TestIndex;

    // ----
    // Sets

    impl<I, K> ListStorage<I, K>
    where
        I: Idx,
        K: Display,
    {
        fn display_set(&self, list: Option<I>) -> String {
            let elements: Vec<_> = self.iter_set_reverse(list).collect();
            let mut result = String::new();
            result.push('[');
            for element in elements.into_iter().rev() {
                if result.len() > 1 {
                    result.push_str(", ");
                }
                write!(&mut result, "{}", element).unwrap();
            }
            result.push(']');
            result
        }
    }

    #[test]
    fn can_insert_into_set() {
        let mut builder = ListBuilder::<TestIndex, u16>::default();

        // Build up the set in order
        let set1 = builder.insert(None, 1);
        let set12 = builder.insert(set1, 2);
        let set123 = builder.insert(set12, 3);
        let set1232 = builder.insert(set123, 2);
        assert_eq!(builder.display_set(None), "[]");
        assert_eq!(builder.display_set(set1), "[1]");
        assert_eq!(builder.display_set(set12), "[1, 2]");
        assert_eq!(builder.display_set(set123), "[1, 2, 3]");
        assert_eq!(builder.display_set(set1232), "[1, 2, 3]");

        // And in reverse order
        let set3 = builder.insert(None, 3);
        let set32 = builder.insert(set3, 2);
        let set321 = builder.insert(set32, 1);
        let set3212 = builder.insert(set321, 2);
        assert_eq!(builder.display_set(None), "[]");
        assert_eq!(builder.display_set(set3), "[3]");
        assert_eq!(builder.display_set(set32), "[2, 3]");
        assert_eq!(builder.display_set(set321), "[1, 2, 3]");
        assert_eq!(builder.display_set(set3212), "[1, 2, 3]");
    }

    #[test]
    fn can_intersect_sets() {
        let mut builder = ListBuilder::<TestIndex, u16>::default();

        let set1 = builder.entry(None, 1).or_insert_default();
        let set12 = builder.entry(set1, 2).or_insert_default();
        let set123 = builder.entry(set12, 3).or_insert_default();
        let set1234 = builder.entry(set123, 4).or_insert_default();

        let set2 = builder.entry(None, 2).or_insert_default();
        let set24 = builder.entry(set2, 4).or_insert_default();
        let set245 = builder.entry(set24, 5).or_insert_default();
        let set2457 = builder.entry(set245, 7).or_insert_default();

        let intersection = builder.intersect(None, None);
        assert_eq!(builder.display_set(intersection), "[]");
        let intersection = builder.intersect(None, set1234);
        assert_eq!(builder.display_set(intersection), "[]");
        let intersection = builder.intersect(None, set2457);
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

    #[test]
    fn can_union_sets() {
        let mut builder = ListBuilder::<TestIndex, u16>::default();

        let set1 = builder.entry(None, 1).or_insert_default();
        let set12 = builder.entry(set1, 2).or_insert_default();
        let set123 = builder.entry(set12, 3).or_insert_default();
        let set1234 = builder.entry(set123, 4).or_insert_default();

        let set2 = builder.entry(None, 2).or_insert_default();
        let set24 = builder.entry(set2, 4).or_insert_default();
        let set245 = builder.entry(set24, 5).or_insert_default();
        let set2457 = builder.entry(set245, 7).or_insert_default();

        let union = builder.union(None, None);
        assert_eq!(builder.display_set(union), "[]");
        let union = builder.union(None, set1234);
        assert_eq!(builder.display_set(union), "[1, 2, 3, 4]");
        let union = builder.union(None, set2457);
        assert_eq!(builder.display_set(union), "[2, 4, 5, 7]");
        let union = builder.union(set1, set1234);
        assert_eq!(builder.display_set(union), "[1, 2, 3, 4]");
        let union = builder.union(set1, set2457);
        assert_eq!(builder.display_set(union), "[1, 2, 4, 5, 7]");
        let union = builder.union(set2, set1234);
        assert_eq!(builder.display_set(union), "[1, 2, 3, 4]");
        let union = builder.union(set2, set2457);
        assert_eq!(builder.display_set(union), "[2, 4, 5, 7]");
        let union = builder.union(set1234, set2457);
        assert_eq!(builder.display_set(union), "[1, 2, 3, 4, 5, 7]");
    }

    // ----
    // Maps

    impl<I, K, V> ListStorage<I, K, V>
    where
        I: Idx,
        K: Display,
        V: Display,
    {
        fn display(&self, list: Option<I>) -> String {
            let entries: Vec<_> = self.iter_reverse(list).collect();
            let mut result = String::new();
            result.push('[');
            for (key, value) in entries.into_iter().rev() {
                if result.len() > 1 {
                    result.push_str(", ");
                }
                write!(&mut result, "{}:{}", key, value).unwrap();
            }
            result.push(']');
            result
        }
    }

    #[test]
    fn can_insert_into_map() {
        let mut builder = ListBuilder::<TestIndex, u16, u16>::default();

        // Build up the map in order
        let map1 = builder.entry(None, 1).replace(1);
        let map12 = builder.entry(map1, 2).replace(2);
        let map123 = builder.entry(map12, 3).replace(3);
        let map1232 = builder.entry(map123, 2).replace(4);
        assert_eq!(builder.display(None), "[]");
        assert_eq!(builder.display(map1), "[1:1]");
        assert_eq!(builder.display(map12), "[1:1, 2:2]");
        assert_eq!(builder.display(map123), "[1:1, 2:2, 3:3]");
        assert_eq!(builder.display(map1232), "[1:1, 2:4, 3:3]");

        // And in reverse order
        let map3 = builder.entry(None, 3).replace(3);
        let map32 = builder.entry(map3, 2).replace(2);
        let map321 = builder.entry(map32, 1).replace(1);
        let map3212 = builder.entry(map321, 2).replace(4);
        assert_eq!(builder.display(None), "[]");
        assert_eq!(builder.display(map3), "[3:3]");
        assert_eq!(builder.display(map32), "[2:2, 3:3]");
        assert_eq!(builder.display(map321), "[1:1, 2:2, 3:3]");
        assert_eq!(builder.display(map3212), "[1:1, 2:4, 3:3]");
    }

    #[test]
    fn can_insert_if_needed_into_map() {
        let mut builder = ListBuilder::<TestIndex, u16, u16>::default();

        // Build up the map in order
        let map1 = builder.entry(None, 1).or_insert(1);
        let map12 = builder.entry(map1, 2).or_insert(2);
        let map123 = builder.entry(map12, 3).or_insert(3);
        let map1232 = builder.entry(map123, 2).or_insert(4);
        assert_eq!(builder.display(None), "[]");
        assert_eq!(builder.display(map1), "[1:1]");
        assert_eq!(builder.display(map12), "[1:1, 2:2]");
        assert_eq!(builder.display(map123), "[1:1, 2:2, 3:3]");
        assert_eq!(builder.display(map1232), "[1:1, 2:2, 3:3]");

        // And in reverse order
        let map3 = builder.entry(None, 3).or_insert(3);
        let map32 = builder.entry(map3, 2).or_insert(2);
        let map321 = builder.entry(map32, 1).or_insert(1);
        let map3212 = builder.entry(map321, 2).or_insert(4);
        assert_eq!(builder.display(None), "[]");
        assert_eq!(builder.display(map3), "[3:3]");
        assert_eq!(builder.display(map32), "[2:2, 3:3]");
        assert_eq!(builder.display(map321), "[1:1, 2:2, 3:3]");
        assert_eq!(builder.display(map3212), "[1:1, 2:2, 3:3]");
    }

    #[test]
    fn can_intersect_maps() {
        let mut builder = ListBuilder::<TestIndex, u16, u16>::default();

        let map1 = builder.entry(None, 1).or_insert(1);
        let map12 = builder.entry(map1, 2).or_insert(2);
        let map123 = builder.entry(map12, 3).or_insert(3);
        let map1234 = builder.entry(map123, 4).or_insert(4);

        let map2 = builder.entry(None, 2).or_insert(20);
        let map24 = builder.entry(map2, 4).or_insert(40);
        let map245 = builder.entry(map24, 5).or_insert(50);
        let map2457 = builder.entry(map245, 7).or_insert(70);

        let intersection = builder.intersect_with(None, None, |a, b| a + b);
        assert_eq!(builder.display(intersection), "[]");
        let intersection = builder.intersect_with(None, map1234, |a, b| a + b);
        assert_eq!(builder.display(intersection), "[]");
        let intersection = builder.intersect_with(None, map2457, |a, b| a + b);
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

    #[test]
    fn can_union_maps() {
        let mut builder = ListBuilder::<TestIndex, u16, u16>::default();

        let map1 = builder.entry(None, 1).or_insert(1);
        let map12 = builder.entry(map1, 2).or_insert(2);
        let map123 = builder.entry(map12, 3).or_insert(3);
        let map1234 = builder.entry(map123, 4).or_insert(4);

        let map2 = builder.entry(None, 2).or_insert(20);
        let map24 = builder.entry(map2, 4).or_insert(40);
        let map245 = builder.entry(map24, 5).or_insert(50);
        let map2457 = builder.entry(map245, 7).or_insert(70);

        let union = builder.union_with(None, None, |a, b| a + b);
        assert_eq!(builder.display(union), "[]");
        let union = builder.union_with(None, map1234, |a, b| a + b);
        assert_eq!(builder.display(union), "[1:1, 2:2, 3:3, 4:4]");
        let union = builder.union_with(None, map2457, |a, b| a + b);
        assert_eq!(builder.display(union), "[2:20, 4:40, 5:50, 7:70]");
        let union = builder.union_with(map1, map1234, |a, b| a + b);
        assert_eq!(builder.display(union), "[1:2, 2:2, 3:3, 4:4]");
        let union = builder.union_with(map1, map2457, |a, b| a + b);
        assert_eq!(builder.display(union), "[1:1, 2:20, 4:40, 5:50, 7:70]");
        let union = builder.union_with(map2, map1234, |a, b| a + b);
        assert_eq!(builder.display(union), "[1:1, 2:22, 3:3, 4:4]");
        let union = builder.union_with(map2, map2457, |a, b| a + b);
        assert_eq!(builder.display(union), "[2:40, 4:40, 5:50, 7:70]");
        let union = builder.union_with(map1234, map2457, |a, b| a + b);
        assert_eq!(builder.display(union), "[1:1, 2:22, 3:3, 4:44, 5:50, 7:70]");
    }
}
