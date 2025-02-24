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

    /// Scratch space that lets us implement our list operations iteratively instead of
    /// recursively.
    ///
    /// The cons-list representation that we use for alists is very common in functional
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

    #[allow(clippy::unnecessary_wraps)]
    fn add_cell(&mut self, key: K, value: V, tail: Option<I>) -> Option<I> {
        Some(self.storage.cells.push(ListCell(key, value, tail)))
    }
}

impl<I: Idx, K: Clone + Ord, V: Clone> ListBuilder<I, K, V> {
    /// Inserts a new key/value pair into an existing list.  If there is already an entry with an
    /// equal key, its value is overwritten.
    pub fn insert(&mut self, list: Option<I>, key: K, value: V) -> Option<I> {
        // Iterate through the input list, looking for the position where the key should be
        // inserted. We will need to create new list cells for any elements that appear before the
        // new key. Stash those away in our scratch accumulator as we step through the input. The
        // result of the loop is that "tail" of the result list, which we will stitch the new key
        // (and any preceding keys) onto.
        let mut curr = list;
        let mut result = loop {
            let Some(curr_id) = curr else {
                // First entry in the list
                break None;
            };

            let ListCell(curr_key, curr_value, tail) = &self.storage.cells[curr_id];
            match key.cmp(curr_key) {
                // We found an existing entry in the input list with the desired key, which we need
                // to overwrite.
                Ordering::Equal => break *tail,
                // The input list does not already contain this key, and this is where we should
                // add it. Break out of the loop and start the stitch-up process.
                Ordering::Less => break curr,
                // If this key is in the list, it's further along. We'll need to create a new cell
                // for this entry into the result list, so add its contents to the scratch
                // accumulator.
                Ordering::Greater => {
                    let new_key = curr_key.clone();
                    let new_value = curr_value.clone();
                    self.scratch.push((new_key, new_value));
                    curr = *tail;
                }
            }
        };

        // We found where the new key should be added (either because the list didn't already
        // contain it, or because it needs to be overwritten). Add the new key first, and then add
        // new cells for all of the existing entries that are smaller than the new key.
        result = self.add_cell(key, value, result);
        while let Some((key, value)) = self.scratch.pop() {
            result = self.add_cell(key, value, result);
        }
        result
    }

    /// Inserts a new key/value pair into an existing list.  If there is already an entry with an
    /// equal key, the original value is retained, and the new value is thrown away.
    pub fn insert_if_needed(&mut self, list: Option<I>, key: K, value: V) -> Option<I> {
        // Iterate through the input list, looking for the position where the key should be
        // inserted. We will need to create new list cells for any elements that appear before the
        // new key. Stash those away in our scratch accumulator as we step through the input. The
        // result of the loop is that "tail" of the result list, which we will stitch the new key
        // (and any preceding keys) onto.
        let mut curr = list;
        let mut result = loop {
            let Some(curr_id) = curr else {
                // First entry in the list
                break None;
            };

            let ListCell(curr_key, curr_value, tail) = &self.storage.cells[curr_id];
            match key.cmp(curr_key) {
                // We found an existing entry in the input list with the desired key, which means
                // we can return the original input as-is.
                Ordering::Equal => return list,
                // The input list does not already contain this key, and this is where we should
                // add it. Break out of the loop and start the stitch-up process.
                Ordering::Less => break curr,
                // If this key is in the list, it's further along. We'll need to create a new cell
                // for this entry into the result list, so add its contents to the scratch
                // accumulator.
                Ordering::Greater => {
                    let new_key = curr_key.clone();
                    let new_value = curr_value.clone();
                    self.scratch.push((new_key, new_value));
                    curr = *tail;
                }
            }
        };

        // The input did not already contain the key, and we found where it should be added. Add
        // the new key first, and then add new cells for all of the existing entries that are
        // smaller than the new key.
        result = self.add_cell(key, value, result);
        while let Some((key, value)) = self.scratch.pop() {
            result = self.add_cell(key, value, result);
        }
        result
    }
}

impl<I: Idx, K: Clone + Ord, V: Clone> ListBuilder<I, K, V> {
    /// Returns the intersection of two lists. The result will contain an entry for any key that
    /// appears in both lists. The corresponding values will be combined using the `combine`
    /// function.
    pub fn intersect<F>(&mut self, mut a: Option<I>, mut b: Option<I>, mut combine: F) -> Option<I>
    where
        F: FnMut(&V, &V) -> V,
    {
        // Zip through the lists, building up the keys/values of the new entries into our scratch
        // vector. Continue until we run out of elements in either list. (Any remaining elements in
        // the other list cannot possibly be in the intersection.)
        while let (Some(a_id), Some(b_id)) = (a, b) {
            let ListCell(a_key, a_value, a_tail) = &self.storage.cells[a_id];
            let ListCell(b_key, b_value, b_tail) = &self.storage.cells[b_id];
            match a_key.cmp(b_key) {
                // Both lists contain this key; combine their values
                Ordering::Equal => {
                    let new_key = a_key.clone();
                    let new_value = combine(a_value, b_value);
                    self.scratch.push((new_key, new_value));
                    a = *a_tail;
                    b = *b_tail;
                }
                // a's key is only present in a, so it's not included in the result.
                Ordering::Less => a = *a_tail,
                // b's key is only present in b, so it's not included in the result.
                Ordering::Greater => b = *b_tail,
            }
        }

        // Once the iteration loop terminates, we stitch the new entries back together into proper
        // alist cells.
        let mut result = None;
        while let Some((key, value)) = self.scratch.pop() {
            result = self.add_cell(key, value, result);
        }
        result
    }
}

impl<I: Idx, K: Clone + Ord, V: Clone> ListBuilder<I, K, V> {
    /// Returns the union of two lists. The result will contain an entry for any key that appears
    /// in either list. For keys that appear in both lists, the corresponding values will be
    /// combined using the `combine` function.
    pub fn union<F>(&mut self, mut a: Option<I>, mut b: Option<I>, mut combine: F) -> Option<I>
    where
        F: FnMut(&V, &V) -> V,
    {
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

            let ListCell(a_key, a_value, a_tail) = &self.storage.cells[a_id];
            let ListCell(b_key, b_value, b_tail) = &self.storage.cells[b_id];
            match a_key.cmp(b_key) {
                // Both lists contain this key; combine their values
                Ordering::Equal => {
                    let new_key = a_key.clone();
                    let new_value = combine(a_value, b_value);
                    self.scratch.push((new_key, new_value));
                    a = *a_tail;
                    b = *b_tail;
                }
                // a's key is lower, so it goes into the result next
                Ordering::Less => {
                    let new_key = a_key.clone();
                    let new_value = a_value.clone();
                    self.scratch.push((new_key, new_value));
                    a = *a_tail;
                }
                // b's key is lower, so it goes into the result next
                Ordering::Greater => {
                    let new_key = b_key.clone();
                    let new_value = b_value.clone();
                    self.scratch.push((new_key, new_value));
                    b = *b_tail;
                }
            }
        };

        // Once the iteration loop terminates, we stitch the new entries back together into proper
        // alist cells.
        while let Some((key, value)) = self.scratch.pop() {
            result = self.add_cell(key, value, result);
        }
        result
    }
}
