use countme::Count;
use rustc_hash::FxHashMap;
use std::fmt::{Debug, Formatter};
use std::iter::FusedIterator;
use std::num::NonZeroU32;
use std::ops::Range;

/// An optimized multi-map implementation for storing *leading*, *dangling*, and *trailing* parts for a key.
/// The inserted parts are stored in insertion-order.
///
/// A naive implementation using three multimaps, one to store the *leading*, another for the *dangling* parts,
/// and a third for the *trailing* parts, requires between `keys < allocations < keys * 3` vec allocations.
///
/// This map implementation optimises for the use case where:
/// * Parts belonging to the same key are inserted together. For example, all parts for the key `a` are inserted
///   before inserting any parts for the key `b`.
/// * The parts per key are inserted in the following order: *leading*, *dangling*, and then the *trailing* parts.
///
/// Parts inserted in the above mentioned order are stored in a `Vec` shared by all keys to reduce the number
/// of allocations and increased cache locality. The implementation falls back to storing the *leading*,
/// *dangling*, and *trailing* parts of a key in dedicated `Vec`s if the parts aren't inserted in the before mentioned order.
/// Out of order insertions come with a slight performance penalty due to:
/// * It requiring up to three [Vec] allocations, one for the *leading*, *dangling*, and *trailing* parts.
/// * It requires copying already inserted parts for that key (by cloning) into the newly allocated [Vec]s.
/// * The resolving of each part for that key requires an extra level of indirection.
///
/// ## Limitations
/// The map supports storing up to `u32::MAX - 1` parts. Inserting the `u32::MAX`nth part panics.
///
/// ## Comments
///
/// Storing the *leading*, *dangling*, and *trailing* comments is an exemplary use case for this map implementation because
/// it is generally desired to keep the comments in the same order as they appear in the source document.
/// This translates to inserting the comments per node and for every node in 1) *leading*, 2) *dangling*, 3) *trailing* order (the order this map optimises for).
///
/// Running Rome's formatter on real world use cases showed that more than 99.99% of comments get inserted in
/// the described order.
///
/// The size limitation isn't a concern for comments because Ruff supports source documents with a size up to 4GB (`u32::MAX`)
/// and every comment has at least a size of 2 bytes:
/// * 1 byte for the start sequence, e.g. `#`
/// * 1 byte for the end sequence, e.g. `\n`
///
/// Meaning, the upper bound for comments is `u32::MAX / 2`.
#[derive(Clone)]
pub(super) struct MultiMap<K, V> {
    /// Lookup table to retrieve the entry for a key.
    index: FxHashMap<K, Entry>,

    /// Flat array storing all the parts that have been inserted in order.
    parts: Vec<V>,

    /// Vector containing the *leading*, *dangling*, and *trailing* vectors for out of order entries.
    ///
    /// The length of `out_of_order` is a multiple of 3 where:
    /// * `index % 3 == 0`: *Leading* parts
    /// * `index % 3 == 1`: *Dangling* parts
    /// * `index % 3 == 2`: *Trailing* parts
    out_of_order_parts: Vec<Vec<V>>,
}

impl<K: std::hash::Hash + Eq, V> MultiMap<K, V> {
    pub(super) fn new() -> Self {
        Self {
            index: FxHashMap::default(),
            parts: Vec::new(),
            out_of_order_parts: Vec::new(),
        }
    }

    /// Pushes a *leading* part for `key`.
    pub(super) fn push_leading(&mut self, key: K, part: V)
    where
        V: Clone,
    {
        match self.index.get_mut(&key) {
            None => {
                let start = self.parts.len();
                self.parts.push(part);

                self.index.insert(
                    key,
                    Entry::InOrder(InOrderEntry::leading(start..self.parts.len())),
                );
            }

            // Has only *leading* parts and no elements have been pushed since
            Some(Entry::InOrder(entry))
                if entry.trailing_start.is_none() && self.parts.len() == entry.range().end =>
            {
                self.parts.push(part);
                entry.increment_leading_range();
            }

            Some(Entry::OutOfOrder(entry)) => {
                let leading = &mut self.out_of_order_parts[entry.leading_index()];
                leading.push(part);
            }

            Some(entry) => {
                let out_of_order =
                    Self::entry_to_out_of_order(entry, &self.parts, &mut self.out_of_order_parts);
                self.out_of_order_parts[out_of_order.leading_index()].push(part);
            }
        }
    }

    /// Pushes a *dangling* part for `key`
    pub(super) fn push_dangling(&mut self, key: K, part: V)
    where
        V: Clone,
    {
        match self.index.get_mut(&key) {
            None => {
                let start = self.parts.len();
                self.parts.push(part);

                self.index.insert(
                    key,
                    Entry::InOrder(InOrderEntry::dangling(start..self.parts.len())),
                );
            }

            // Has leading and dangling comments and no new parts have been inserted since.
            Some(Entry::InOrder(entry))
                if entry.trailing_end.is_none() && self.parts.len() == entry.range().end =>
            {
                self.parts.push(part);
                entry.increment_dangling_range();
            }

            Some(Entry::OutOfOrder(entry)) => {
                let dangling = &mut self.out_of_order_parts[entry.dangling_index()];
                dangling.push(part);
            }

            Some(entry) => {
                let out_of_order =
                    Self::entry_to_out_of_order(entry, &self.parts, &mut self.out_of_order_parts);
                self.out_of_order_parts[out_of_order.dangling_index()].push(part);
            }
        }
    }

    /// Pushes a *trailing* part for `key`.
    pub(super) fn push_trailing(&mut self, key: K, part: V)
    where
        V: Clone,
    {
        match self.index.get_mut(&key) {
            None => {
                let start = self.parts.len();
                self.parts.push(part);

                self.index.insert(
                    key,
                    Entry::InOrder(InOrderEntry::trailing(start..self.parts.len())),
                );
            }

            // No new parts have been inserted since
            Some(Entry::InOrder(entry)) if entry.range().end == self.parts.len() => {
                self.parts.push(part);
                entry.increment_trailing_range();
            }

            Some(Entry::OutOfOrder(entry)) => {
                let trailing = &mut self.out_of_order_parts[entry.trailing_index()];
                trailing.push(part);
            }

            Some(entry) => {
                let out_of_order =
                    Self::entry_to_out_of_order(entry, &self.parts, &mut self.out_of_order_parts);
                self.out_of_order_parts[out_of_order.trailing_index()].push(part);
            }
        }
    }

    /// Slow path for converting an in-order entry to an out-of order entry.
    /// Copies over all parts into the `out_of_order` vec.
    #[cold]
    fn entry_to_out_of_order<'a>(
        entry: &'a mut Entry,
        parts: &[V],
        out_of_order: &mut Vec<Vec<V>>,
    ) -> &'a mut OutOfOrderEntry
    where
        V: Clone,
    {
        match entry {
            Entry::InOrder(in_order) => {
                let index = out_of_order.len();

                out_of_order.push(parts[in_order.leading_range()].to_vec());
                out_of_order.push(parts[in_order.dangling_range()].to_vec());
                out_of_order.push(parts[in_order.trailing_range()].to_vec());

                *entry = Entry::OutOfOrder(OutOfOrderEntry {
                    leading_index: index,
                    _count: Count::new(),
                });

                match entry {
                    Entry::InOrder(_) => unreachable!(),
                    Entry::OutOfOrder(out_of_order) => out_of_order,
                }
            }
            Entry::OutOfOrder(entry) => entry,
        }
    }

    pub(super) fn keys(&self) -> Keys<'_, K> {
        Keys {
            inner: self.index.keys(),
        }
    }

    /// Returns the *leading* parts of `key` in insertion-order.
    pub(super) fn leading(&self, key: &K) -> &[V] {
        match self.index.get(key) {
            None => &[],
            Some(Entry::InOrder(in_order)) => &self.parts[in_order.leading_range()],
            Some(Entry::OutOfOrder(entry)) => &self.out_of_order_parts[entry.leading_index()],
        }
    }

    /// Returns the *dangling* parts of `key` in insertion-order.
    pub(super) fn dangling(&self, key: &K) -> &[V] {
        match self.index.get(key) {
            None => &[],
            Some(Entry::InOrder(in_order)) => &self.parts[in_order.dangling_range()],
            Some(Entry::OutOfOrder(entry)) => &self.out_of_order_parts[entry.dangling_index()],
        }
    }

    /// Returns the *trailing* parts of `key` in insertion order.
    pub(super) fn trailing(&self, key: &K) -> &[V] {
        match self.index.get(key) {
            None => &[],
            Some(Entry::InOrder(in_order)) => &self.parts[in_order.trailing_range()],
            Some(Entry::OutOfOrder(entry)) => &self.out_of_order_parts[entry.trailing_index()],
        }
    }

    /// Returns `true` if `key` has any *leading*, *dangling*, or *trailing* parts.
    pub(super) fn has(&self, key: &K) -> bool {
        self.index.get(key).is_some()
    }

    /// Returns an iterator over the *leading*, *dangling*, and *trailing* parts of `key`.
    pub(super) fn parts(&self, key: &K) -> PartsIterator<V> {
        match self.index.get(key) {
            None => PartsIterator::Slice([].iter()),
            Some(entry) => PartsIterator::from_entry(entry, self),
        }
    }

    /// Returns an iterator over the parts of all keys.
    #[allow(unused)]
    pub(super) fn all_parts(&self) -> impl Iterator<Item = &V> {
        self.index
            .values()
            .flat_map(|entry| PartsIterator::from_entry(entry, self))
    }
}

impl<K: std::hash::Hash + Eq, V> Default for MultiMap<K, V> {
    fn default() -> Self {
        Self::new()
    }
}

impl<K, V> Debug for MultiMap<K, V>
where
    K: Debug,
    V: Debug,
{
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        let mut builder = f.debug_map();

        for (key, entry) in &self.index {
            builder.entry(&key, &DebugEntry { entry, map: self });
        }

        builder.finish()
    }
}

/// Iterates over all *leading*, *dangling*, and *trailing* parts of a key.
pub(super) enum PartsIterator<'a, V> {
    /// The slice into the [CommentsMap::parts] [Vec] if this is an in-order entry or the *trailing* parts
    /// of an out-of-order entry.
    Slice(std::slice::Iter<'a, V>),

    /// Iterator over the *leading* parts of an out-of-order entry. Returns the *dangling* parts, and then the
    /// *trailing* parts once the *leading* iterator is fully consumed.
    Leading {
        leading: std::slice::Iter<'a, V>,
        dangling: &'a [V],
        trailing: &'a [V],
    },

    /// Iterator over the *dangling* parts of an out-of-order entry. Returns the *trailing* parts
    /// once the *leading* iterator is fully consumed.
    Dangling {
        dangling: std::slice::Iter<'a, V>,
        trailing: &'a [V],
    },
}

impl<'a, V> PartsIterator<'a, V> {
    fn from_entry<K>(entry: &Entry, map: &'a MultiMap<K, V>) -> Self {
        match entry {
            Entry::InOrder(entry) => PartsIterator::Slice(map.parts[entry.range()].iter()),
            Entry::OutOfOrder(entry) => PartsIterator::Leading {
                leading: map.out_of_order_parts[entry.leading_index()].iter(),
                dangling: &map.out_of_order_parts[entry.dangling_index()],
                trailing: &map.out_of_order_parts[entry.trailing_index()],
            },
        }
    }
}

impl<'a, V> Iterator for PartsIterator<'a, V> {
    type Item = &'a V;

    fn next(&mut self) -> Option<Self::Item> {
        match self {
            PartsIterator::Slice(inner) => inner.next(),

            PartsIterator::Leading {
                leading,
                dangling,
                trailing,
            } => match leading.next() {
                Some(next) => Some(next),
                None if !dangling.is_empty() => {
                    let mut dangling_iterator = dangling.iter();
                    let next = dangling_iterator.next().unwrap();
                    *self = PartsIterator::Dangling {
                        dangling: dangling_iterator,
                        trailing,
                    };
                    Some(next)
                }
                None => {
                    let mut trailing_iterator = trailing.iter();
                    let next = trailing_iterator.next();
                    *self = PartsIterator::Slice(trailing_iterator);
                    next
                }
            },

            PartsIterator::Dangling { dangling, trailing } => dangling.next().or_else(|| {
                let mut trailing_iterator = trailing.iter();
                let next = trailing_iterator.next();
                *self = PartsIterator::Slice(trailing_iterator);
                next
            }),
        }
    }

    fn fold<B, F>(self, init: B, f: F) -> B
    where
        F: FnMut(B, Self::Item) -> B,
    {
        match self {
            PartsIterator::Slice(slice) => slice.fold(init, f),
            PartsIterator::Leading {
                leading,
                dangling,
                trailing,
            } => leading
                .chain(dangling.iter())
                .chain(trailing.iter())
                .fold(init, f),
            PartsIterator::Dangling { dangling, trailing } => {
                dangling.chain(trailing.iter()).fold(init, f)
            }
        }
    }

    fn all<F>(&mut self, f: F) -> bool
    where
        F: FnMut(Self::Item) -> bool,
    {
        match self {
            PartsIterator::Slice(slice) => slice.all(f),
            PartsIterator::Leading {
                leading,
                dangling,
                trailing,
            } => leading.chain(dangling.iter()).chain(trailing.iter()).all(f),
            PartsIterator::Dangling { dangling, trailing } => {
                dangling.chain(trailing.iter()).all(f)
            }
        }
    }

    fn any<F>(&mut self, f: F) -> bool
    where
        F: FnMut(Self::Item) -> bool,
    {
        match self {
            PartsIterator::Slice(slice) => slice.any(f),
            PartsIterator::Leading {
                leading,
                dangling,
                trailing,
            } => leading.chain(dangling.iter()).chain(trailing.iter()).any(f),
            PartsIterator::Dangling { dangling, trailing } => {
                dangling.chain(trailing.iter()).any(f)
            }
        }
    }

    fn find<P>(&mut self, predicate: P) -> Option<Self::Item>
    where
        P: FnMut(&Self::Item) -> bool,
    {
        match self {
            PartsIterator::Slice(slice) => slice.find(predicate),
            PartsIterator::Leading {
                leading,
                dangling,
                trailing,
            } => leading
                .chain(dangling.iter())
                .chain(trailing.iter())
                .find(predicate),
            PartsIterator::Dangling { dangling, trailing } => {
                dangling.chain(trailing.iter()).find(predicate)
            }
        }
    }

    fn find_map<B, F>(&mut self, f: F) -> Option<B>
    where
        F: FnMut(Self::Item) -> Option<B>,
    {
        match self {
            PartsIterator::Slice(slice) => slice.find_map(f),
            PartsIterator::Leading {
                leading,
                dangling,
                trailing,
            } => leading
                .chain(dangling.iter())
                .chain(trailing.iter())
                .find_map(f),
            PartsIterator::Dangling { dangling, trailing } => {
                dangling.chain(trailing.iter()).find_map(f)
            }
        }
    }

    fn position<P>(&mut self, predicate: P) -> Option<usize>
    where
        P: FnMut(Self::Item) -> bool,
    {
        match self {
            PartsIterator::Slice(slice) => slice.position(predicate),
            PartsIterator::Leading {
                leading,
                dangling,
                trailing,
            } => leading
                .chain(dangling.iter())
                .chain(trailing.iter())
                .position(predicate),
            PartsIterator::Dangling { dangling, trailing } => {
                dangling.chain(trailing.iter()).position(predicate)
            }
        }
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        match self {
            PartsIterator::Slice(slice) => slice.size_hint(),
            PartsIterator::Leading {
                leading,
                dangling,
                trailing,
            } => {
                let len = leading.len() + dangling.len() + trailing.len();

                (len, Some(len))
            }
            PartsIterator::Dangling { dangling, trailing } => {
                let len = dangling.len() + trailing.len();
                (len, Some(len))
            }
        }
    }

    fn count(self) -> usize {
        self.size_hint().0
    }

    fn last(self) -> Option<Self::Item> {
        match self {
            PartsIterator::Slice(slice) => slice.last(),
            PartsIterator::Leading {
                leading,
                dangling,
                trailing,
            } => trailing
                .last()
                .or_else(|| dangling.last())
                .or_else(|| leading.last()),
            PartsIterator::Dangling { dangling, trailing } => {
                trailing.last().or_else(|| dangling.last())
            }
        }
    }
}

impl<V> ExactSizeIterator for PartsIterator<'_, V> {}

impl<V> FusedIterator for PartsIterator<'_, V> {}

#[derive(Clone, Debug)]
enum Entry {
    InOrder(InOrderEntry),
    OutOfOrder(OutOfOrderEntry),
}

struct DebugEntry<'a, K, V> {
    entry: &'a Entry,
    map: &'a MultiMap<K, V>,
}

impl<K, V> Debug for DebugEntry<'_, K, V>
where
    K: Debug,
    V: Debug,
{
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        let leading = match self.entry {
            Entry::OutOfOrder(entry) => {
                self.map.out_of_order_parts[entry.leading_index()].as_slice()
            }
            Entry::InOrder(entry) => &self.map.parts[entry.leading_range()],
        };

        let dangling = match self.entry {
            Entry::OutOfOrder(entry) => {
                self.map.out_of_order_parts[entry.dangling_index()].as_slice()
            }
            Entry::InOrder(entry) => &self.map.parts[entry.dangling_range()],
        };

        let trailing = match self.entry {
            Entry::OutOfOrder(entry) => {
                self.map.out_of_order_parts[entry.trailing_index()].as_slice()
            }
            Entry::InOrder(entry) => &self.map.parts[entry.trailing_range()],
        };

        let mut list = f.debug_list();

        list.entries(leading.iter().map(DebugValue::Leading));
        list.entries(dangling.iter().map(DebugValue::Dangling));
        list.entries(trailing.iter().map(DebugValue::Trailing));

        list.finish()
    }
}

enum DebugValue<'a, V> {
    Leading(&'a V),
    Dangling(&'a V),
    Trailing(&'a V),
}

impl<V> Debug for DebugValue<'_, V>
where
    V: Debug,
{
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            DebugValue::Leading(leading) => f.debug_tuple("Leading").field(leading).finish(),
            DebugValue::Dangling(dangling) => f.debug_tuple("Dangling").field(dangling).finish(),
            DebugValue::Trailing(trailing) => f.debug_tuple("Trailing").field(trailing).finish(),
        }
    }
}

#[derive(Clone, Debug)]
struct InOrderEntry {
    /// Index into the [MultiMap::parts] vector where the leading parts of this entry start
    leading_start: PartIndex,

    /// Index into the [MultiMap::parts] vector where the dangling parts (and, thus, the leading parts end) start.
    dangling_start: PartIndex,

    /// Index into the [MultiMap::parts] vector where the trailing parts (and, thus, the dangling parts end) of this entry start
    trailing_start: Option<PartIndex>,

    /// Index into the [MultiMap::parts] vector where the trailing parts of this entry end
    trailing_end: Option<PartIndex>,

    _count: Count<InOrderEntry>,
}

impl InOrderEntry {
    fn leading(range: Range<usize>) -> Self {
        InOrderEntry {
            leading_start: PartIndex::from_len(range.start),
            dangling_start: PartIndex::from_len(range.end),
            trailing_start: None,
            trailing_end: None,
            _count: Count::new(),
        }
    }

    fn dangling(range: Range<usize>) -> Self {
        let start = PartIndex::from_len(range.start);
        InOrderEntry {
            leading_start: start,
            dangling_start: start,
            trailing_start: Some(PartIndex::from_len(range.end)),
            trailing_end: None,
            _count: Count::new(),
        }
    }

    fn trailing(range: Range<usize>) -> Self {
        let start = PartIndex::from_len(range.start);
        InOrderEntry {
            leading_start: start,
            dangling_start: start,
            trailing_start: Some(start),
            trailing_end: Some(PartIndex::from_len(range.end)),
            _count: Count::new(),
        }
    }

    fn increment_leading_range(&mut self) {
        assert!(
            self.trailing_start.is_none(),
            "Can't extend the leading range for an in order entry with dangling comments."
        );

        self.dangling_start.increment();
    }

    fn increment_dangling_range(&mut self) {
        assert!(
            self.trailing_end.is_none(),
            "Can't extend the dangling range for an in order entry with trailing comments."
        );

        match &mut self.trailing_start {
            Some(start) => start.increment(),
            None => self.trailing_start = Some(self.dangling_start.incremented()),
        }
    }

    fn increment_trailing_range(&mut self) {
        match (self.trailing_start, &mut self.trailing_end) {
            // Already has some trailing comments
            (Some(_), Some(end)) => end.increment(),
            // Has dangling comments only
            (Some(start), None) => self.trailing_end = Some(start.incremented()),
            // Has leading comments only
            (None, None) => {
                self.trailing_start = Some(self.dangling_start);
                self.trailing_end = Some(self.dangling_start.incremented());
            }
            (None, Some(_)) => {
                unreachable!()
            }
        }
    }

    fn leading_range(&self) -> Range<usize> {
        self.leading_start.value()..self.dangling_start.value()
    }

    fn dangling_range(&self) -> Range<usize> {
        match self.trailing_start {
            None => self.dangling_start.value()..self.dangling_start.value(),
            Some(trailing_start) => self.dangling_start.value()..trailing_start.value(),
        }
    }

    fn trailing_range(&self) -> Range<usize> {
        match (self.trailing_start, self.trailing_end) {
            (Some(trailing_start), Some(trailing_end)) => {
                trailing_start.value()..trailing_end.value()
            }
            // Only dangling comments
            (Some(trailing_start), None) => trailing_start.value()..trailing_start.value(),
            (None, Some(_)) => {
                panic!("Trailing end shouldn't be set if trailing start is none");
            }
            (None, None) => self.dangling_start.value()..self.dangling_start.value(),
        }
    }

    fn range(&self) -> Range<usize> {
        self.leading_start.value()
            ..self
                .trailing_end
                .or(self.trailing_start)
                .unwrap_or(self.dangling_start)
                .value()
    }
}

#[derive(Clone, Debug)]
struct OutOfOrderEntry {
    /// Index into the [MultiMap::out_of_order] vector at which offset the leaading vec is stored.
    leading_index: usize,
    _count: Count<OutOfOrderEntry>,
}

impl OutOfOrderEntry {
    const fn leading_index(&self) -> usize {
        self.leading_index
    }

    const fn dangling_index(&self) -> usize {
        self.leading_index + 1
    }

    const fn trailing_index(&self) -> usize {
        self.leading_index + 2
    }
}

/// Index into the [`MultiMap::parts`] vector.
///
/// Stores the index as a [`NonZeroU32`], starting at 1 instead of 0 so that
/// `size_of::<PartIndex>() == size_of::<Option<PartIndex>>()`.
///
/// This means, that only `u32 - 1` parts can be stored. This should be sufficient for storing comments
/// because: Comments have length of two or more bytes because they consist of a start and end character sequence (`#` + new line, `/*` and `*/`).
/// Thus, a document with length `u32` can have at most `u32::MAX / 2` comment-parts.
#[repr(transparent)]
#[derive(Debug, Copy, Clone, Eq, PartialEq, Hash)]
struct PartIndex(NonZeroU32);

impl PartIndex {
    fn from_len(value: usize) -> Self {
        assert!(value < u32::MAX as usize);
        // SAFETY:
        // * The `value < u32::MAX` guarantees that the add doesn't overflow.
        // * The `+ 1` guarantees that the index is not zero
        #[allow(unsafe_code, clippy::cast_possible_truncation)]
        Self(unsafe { std::num::NonZeroU32::new_unchecked((value as u32) + 1) })
    }

    fn value(self) -> usize {
        (u32::from(self.0) - 1) as usize
    }

    fn increment(&mut self) {
        *self = self.incremented();
    }

    fn incremented(self) -> PartIndex {
        PartIndex(NonZeroU32::new(self.0.get() + 1).unwrap())
    }
}

/// Iterator over the keys of a comments multi map
pub(super) struct Keys<'a, K> {
    inner: std::collections::hash_map::Keys<'a, K, Entry>,
}

impl<'a, K> Iterator for Keys<'a, K> {
    type Item = &'a K;

    fn next(&mut self) -> Option<Self::Item> {
        self.inner.next()
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.inner.size_hint()
    }
}

impl<K> ExactSizeIterator for Keys<'_, K> {}
impl<K> FusedIterator for Keys<'_, K> {}

#[cfg(test)]
mod tests {
    use crate::comments::map::MultiMap;

    static EMPTY: [i32; 0] = [];

    #[test]
    fn leading_dangling_trailing() {
        let mut map = MultiMap::new();

        map.push_leading("a", 1);
        map.push_dangling("a", 2);
        map.push_dangling("a", 3);
        map.push_trailing("a", 4);

        assert_eq!(map.parts, vec![1, 2, 3, 4]);

        assert_eq!(map.leading(&"a"), &[1]);
        assert_eq!(map.dangling(&"a"), &[2, 3]);
        assert_eq!(map.trailing(&"a"), &[4]);

        assert!(map.has(&"a"));

        assert_eq!(
            map.parts(&"a").copied().collect::<Vec<_>>(),
            vec![1, 2, 3, 4]
        );
    }

    #[test]
    fn dangling_trailing() {
        let mut map = MultiMap::new();

        map.push_dangling("a", 1);
        map.push_dangling("a", 2);
        map.push_trailing("a", 3);

        assert_eq!(map.parts, vec![1, 2, 3]);

        assert_eq!(map.leading(&"a"), &EMPTY);
        assert_eq!(map.dangling(&"a"), &[1, 2]);
        assert_eq!(map.trailing(&"a"), &[3]);

        assert!(map.has(&"a"));

        assert_eq!(map.parts(&"a").copied().collect::<Vec<_>>(), vec![1, 2, 3]);
    }

    #[test]
    fn trailing() {
        let mut map = MultiMap::new();

        map.push_trailing("a", 1);
        map.push_trailing("a", 2);

        assert_eq!(map.parts, vec![1, 2]);

        assert_eq!(map.leading(&"a"), &EMPTY);
        assert_eq!(map.dangling(&"a"), &EMPTY);
        assert_eq!(map.trailing(&"a"), &[1, 2]);

        assert!(map.has(&"a"));

        assert_eq!(map.parts(&"a").copied().collect::<Vec<_>>(), vec![1, 2]);
    }

    #[test]
    fn empty() {
        let map = MultiMap::<&str, i32>::default();

        assert_eq!(map.parts, Vec::<i32>::new());

        assert_eq!(map.leading(&"a"), &EMPTY);
        assert_eq!(map.dangling(&"a"), &EMPTY);
        assert_eq!(map.trailing(&"a"), &EMPTY);

        assert!(!map.has(&"a"));

        assert_eq!(
            map.parts(&"a").copied().collect::<Vec<_>>(),
            Vec::<i32>::new()
        );
    }

    #[test]
    fn multiple_keys() {
        let mut map = MultiMap::new();

        map.push_leading("a", 1);
        map.push_dangling("b", 2);
        map.push_trailing("c", 3);
        map.push_leading("d", 4);
        map.push_dangling("d", 5);
        map.push_trailing("d", 6);

        assert_eq!(map.parts, &[1, 2, 3, 4, 5, 6]);

        assert_eq!(map.leading(&"a"), &[1]);
        assert_eq!(map.dangling(&"a"), &EMPTY);
        assert_eq!(map.trailing(&"a"), &EMPTY);
        assert_eq!(map.parts(&"a").copied().collect::<Vec<_>>(), vec![1]);

        assert_eq!(map.leading(&"b"), &EMPTY);
        assert_eq!(map.dangling(&"b"), &[2]);
        assert_eq!(map.trailing(&"b"), &EMPTY);
        assert_eq!(map.parts(&"b").copied().collect::<Vec<_>>(), vec![2]);

        assert_eq!(map.leading(&"c"), &EMPTY);
        assert_eq!(map.dangling(&"c"), &EMPTY);
        assert_eq!(map.trailing(&"c"), &[3]);
        assert_eq!(map.parts(&"c").copied().collect::<Vec<_>>(), vec![3]);

        assert_eq!(map.leading(&"d"), &[4]);
        assert_eq!(map.dangling(&"d"), &[5]);
        assert_eq!(map.trailing(&"d"), &[6]);
        assert_eq!(map.parts(&"d").copied().collect::<Vec<_>>(), vec![4, 5, 6]);
    }

    #[test]
    fn dangling_leading() {
        let mut map = MultiMap::new();

        map.push_dangling("a", 1);
        map.push_leading("a", 2);
        map.push_dangling("a", 3);
        map.push_trailing("a", 4);

        assert_eq!(map.leading(&"a"), [2]);
        assert_eq!(map.dangling(&"a"), [1, 3]);
        assert_eq!(map.trailing(&"a"), [4]);

        assert_eq!(
            map.parts(&"a").copied().collect::<Vec<_>>(),
            vec![2, 1, 3, 4]
        );

        assert!(map.has(&"a"));
    }

    #[test]
    fn trailing_leading() {
        let mut map = MultiMap::new();

        map.push_trailing("a", 1);
        map.push_leading("a", 2);
        map.push_dangling("a", 3);
        map.push_trailing("a", 4);

        assert_eq!(map.leading(&"a"), [2]);
        assert_eq!(map.dangling(&"a"), [3]);
        assert_eq!(map.trailing(&"a"), [1, 4]);

        assert_eq!(
            map.parts(&"a").copied().collect::<Vec<_>>(),
            vec![2, 3, 1, 4]
        );

        assert!(map.has(&"a"));
    }

    #[test]
    fn trailing_dangling() {
        let mut map = MultiMap::new();

        map.push_trailing("a", 1);
        map.push_dangling("a", 2);
        map.push_trailing("a", 3);

        assert_eq!(map.leading(&"a"), &EMPTY);
        assert_eq!(map.dangling(&"a"), &[2]);
        assert_eq!(map.trailing(&"a"), &[1, 3]);

        assert_eq!(map.parts(&"a").copied().collect::<Vec<_>>(), vec![2, 1, 3]);

        assert!(map.has(&"a"));
    }

    #[test]
    fn keys_out_of_order() {
        let mut map = MultiMap::new();

        map.push_leading("a", 1);
        map.push_dangling("b", 2);
        map.push_leading("a", 3);

        map.push_trailing("c", 4);
        map.push_dangling("b", 5);

        map.push_leading("d", 6);
        map.push_trailing("c", 7);

        assert_eq!(map.leading(&"a"), &[1, 3]);
        assert_eq!(map.dangling(&"b"), &[2, 5]);
        assert_eq!(map.trailing(&"c"), &[4, 7]);

        assert!(map.has(&"a"));
        assert!(map.has(&"b"));
        assert!(map.has(&"c"));
    }
}
