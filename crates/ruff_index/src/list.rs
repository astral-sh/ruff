use std::cmp::Ordering;
use std::ops::Deref;

use crate::vec::IndexVec;
use crate::Idx;

#[derive(Debug, Eq, PartialEq)]
struct ListCell<I, K, V>(K, V, Option<I>);

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
/// This type provides read-only access to the lists.  Use a [`ListBuilder`] to create lists.
#[derive(Debug, Eq, PartialEq)]
pub struct ListStorage<I, K, V> {
    cells: IndexVec<I, ListCell<I, K, V>>,
}

impl<I: Idx, K, V> ListStorage<I, K, V> {
    /// Iterates through the entries in a list.
    pub fn iter(&self, list: Option<I>) -> ListIterator<'_, I, K, V> {
        ListIterator {
            storage: self,
            curr: list,
        }
    }
}

impl<I: Idx, K: Ord, V> ListStorage<I, K, V> {
    /// Finds the entry in a list with a given key, and returns its value.
    ///
    /// **Performance**: Note that lookups are O(n), since we use a linked-list representation!
    pub fn get(&self, list: Option<I>, key: &K) -> Option<&V> {
        self.iter(list).find(|(k, _)| *k == key).map(|(_, v)| v)
    }
}

#[derive(Debug)]
pub struct ListIterator<'a, I, K, V> {
    storage: &'a ListStorage<I, K, V>,
    curr: Option<I>,
}

impl<'a, I: Idx, K, V> Iterator for ListIterator<'a, I, K, V> {
    type Item = (&'a K, &'a V);

    fn next(&mut self) -> Option<Self::Item> {
        let ListCell(key, value, tail) = &self.storage.cells[self.curr?];
        self.curr = *tail;
        Some((key, value))
    }
}

/// Constructs one or more association lists.
#[derive(Debug, Eq, PartialEq)]
pub struct ListBuilder<I, K, V> {
    storage: ListStorage<I, K, V>,
}

impl<I: Idx, K, V> Default for ListBuilder<I, K, V> {
    fn default() -> Self {
        ListBuilder {
            storage: ListStorage {
                cells: IndexVec::default(),
            },
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

    #[allow(clippy::unnecessary_wraps)]
    fn add_cell(&mut self, key: K, value: V, tail: Option<I>) -> Option<I> {
        Some(self.storage.cells.push(ListCell(key, value, tail)))
    }
}

impl<I: Idx, K: Clone + Ord, V: Clone> ListBuilder<I, K, V> {
    /// Inserts a new key/value pair into an existing list.  If there is already an entry with an
    /// equal key, its value is overwritten.
    pub fn insert(&mut self, list: Option<I>, key: K, value: V) -> Option<I> {
        let Some(curr_id) = list else {
            // First entry in the list
            return self.add_cell(key, value, None);
        };

        let ListCell(curr_key, curr_value, tail) = &self.storage.cells[curr_id];
        match key.cmp(curr_key) {
            // Existing entry with equal key; overwrite the value
            Ordering::Equal => self.add_cell(key, value, *tail),
            // List does not already contain this key, and this is where we should add it.
            Ordering::Less => self.add_cell(key, value, list),
            // If this key is in the list, it's further along.
            Ordering::Greater => {
                let new_key = curr_key.clone();
                let new_value = curr_value.clone();
                let new_tail = self.insert(*tail, key, value);
                self.add_cell(new_key, new_value, new_tail)
            }
        }
    }

    /// Inserts a new key/value pair into an existing list.  If there is already an entry with an
    /// equal key, the original value is retained, and the new value is thrown away.
    pub fn insert_if_needed(&mut self, list: Option<I>, key: K, value: V) -> Option<I> {
        let Some(curr_id) = list else {
            // First entry in the list
            return self.add_cell(key, value, None);
        };

        let ListCell(curr_key, _, tail) = &self.storage.cells[curr_id];
        match key.cmp(curr_key) {
            // Existing entry with equal key; retain the previous value.
            Ordering::Equal => list,
            // List does not already contain this key, and this is where we should add it.
            Ordering::Less => self.add_cell(key, value, list),
            // If this key is in the list, it's further along.
            Ordering::Greater => {
                let tail = *tail;
                let new_tail = self.insert_if_needed(tail, key, value);
                if new_tail == tail {
                    // The element was already in the list, so we don't need to stitch up a new
                    // one.
                    return list;
                }
                // Reborrow to satisfy the borrow checker
                let ListCell(curr_key, curr_value, _) = &self.storage.cells[curr_id];
                let new_key = curr_key.clone();
                let new_value = curr_value.clone();
                self.add_cell(new_key, new_value, new_tail)
            }
        }
    }
}

impl<I: Idx, K: Clone + Ord, V: Clone> ListBuilder<I, K, V> {
    /// Returns the intersection of two lists. The result will contain an entry for any key that
    /// appears in both lists. The corresponding values will be combined using the `combine`
    /// function.
    pub fn intersect<F>(&mut self, a: Option<I>, b: Option<I>, mut combine: F) -> Option<I>
    where
        F: FnMut(&V, &V) -> V,
    {
        let (Some(a_id), Some(b_id)) = (a, b) else {
            // a ∩ ∅ == ∅
            // ∅ ∩ a == ∅
            return None;
        };

        let ListCell(a_key, a_value, a_tail) = &self.storage.cells[a_id];
        let ListCell(b_key, b_value, b_tail) = &self.storage.cells[b_id];
        match a_key.cmp(b_key) {
            // Both lists contain this key; combine their values
            Ordering::Equal => {
                let new_key = a_key.clone();
                let new_value = combine(a_value, b_value);
                let new_tail = self.intersect(*a_tail, *b_tail, combine);
                self.add_cell(new_key, new_value, new_tail)
            }
            // a's key is only present in a, so it's not included in the result.
            Ordering::Less => self.intersect(*a_tail, b, combine),
            // b's key is only present in b, so it's not included in the result.
            Ordering::Greater => self.intersect(a, *b_tail, combine),
        }
    }
}

impl<I: Idx, K: Clone + Ord, V: Clone> ListBuilder<I, K, V> {
    /// Returns the union of two lists. The result will contain an entry for any key that appears
    /// in either list. For keys that appear in both lists, the corresponding values will be
    /// combined using the `combine` function.
    pub fn union<F>(&mut self, a: Option<I>, b: Option<I>, mut combine: F) -> Option<I>
    where
        F: FnMut(&V, &V) -> V,
    {
        let (a_id, b_id) = match (a, b) {
            (None, other) | (other, None) => return other,
            (Some(a_id), Some(b_id)) => (a_id, b_id),
        };

        let ListCell(a_key, a_value, a_tail) = &self.storage.cells[a_id];
        let ListCell(b_key, b_value, b_tail) = &self.storage.cells[b_id];
        let (new_key, new_value, new_tail) = match a_key.cmp(b_key) {
            // Both lists contain this key; combine their values
            Ordering::Equal => (
                a_key.clone(),
                combine(a_value, b_value),
                self.union(*a_tail, *b_tail, combine),
            ),
            // a's key is lower, so it goes into the result next
            Ordering::Less => (
                a_key.clone(),
                a_value.clone(),
                self.union(*a_tail, b, combine),
            ),
            // b's key is lower, so it goes into the result next
            Ordering::Greater => (
                b_key.clone(),
                b_value.clone(),
                self.union(a, *b_tail, combine),
            ),
        };
        self.add_cell(new_key, new_value, new_tail)
    }
}

impl<I: Idx, K: Clone, V> ListBuilder<I, K, V> {
    /// Applies a function to each value in a list, returning a new list.
    pub fn map<F>(&mut self, list: Option<I>, mut f: F) -> Option<I>
    where
        F: FnMut(&V) -> V,
    {
        let list_id = list?;
        let ListCell(key, value, tail) = &self.storage.cells[list_id];
        let new_key = key.clone();
        let new_value = f(value);
        let new_tail = self.map(*tail, f);
        self.add_cell(new_key, new_value, new_tail)
    }
}
