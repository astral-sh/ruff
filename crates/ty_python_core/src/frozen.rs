/// A slice of 32-bit IDs stored at the narrowest width that fits every entry.
#[derive(Debug, PartialEq, Eq, get_size2::GetSize)]
pub(super) enum PackedIds<T> {
    U8(Box<[u8]>),
    U16(Box<[u16]>),
    Full(Box<[T]>),
}

impl<T: Copy + Into<u32>> From<Vec<T>> for PackedIds<T> {
    fn from(ids: Vec<T>) -> Self {
        #[expect(
            clippy::cast_possible_truncation,
            reason = "the largest ID fits in the selected integer width"
        )]
        match ids.iter().copied().map(Into::into).max().unwrap_or(0) {
            0..=0xff => Self::U8(ids.into_iter().map(|id| id.into() as u8).collect()),
            0x100..=0xffff => Self::U16(ids.into_iter().map(|id| id.into() as u16).collect()),
            _ => Self::Full(ids.into_boxed_slice()),
        }
    }
}

impl<T: Copy + From<u32>> PackedIds<T> {
    pub(super) fn get(&self, index: usize) -> T {
        match self {
            Self::U8(ids) => T::from(u32::from(ids[index])),
            Self::U16(ids) => T::from(u32::from(ids[index])),
            Self::Full(ids) => ids[index],
        }
    }
}

/// Compact immutable key-value entries stored in key order.
///
/// Analysis builds these tables with hash maps, but after construction they only need keyed
/// lookup. A sorted slice avoids retaining hash-table capacity for every indexed file.
#[derive(Debug, Eq, PartialEq, get_size2::GetSize, salsa::SalsaValue)]
pub struct FrozenMap<K, V>(Box<[(K, V)]>);

impl<K, V> FrozenMap<K, V> {
    pub fn iter(&self) -> std::slice::Iter<'_, (K, V)> {
        self.0.iter()
    }

    pub fn iter_mut(
        &mut self,
    ) -> impl DoubleEndedIterator<Item = (&K, &mut V)> + ExactSizeIterator + std::iter::FusedIterator
    {
        self.into_iter()
    }

    pub fn values(&self) -> impl DoubleEndedIterator<Item = &V> + ExactSizeIterator {
        self.0.iter().map(|(_, value)| value)
    }
}

impl<K: Ord, V> FromIterator<(K, V)> for FrozenMap<K, V> {
    fn from_iter<T: IntoIterator<Item = (K, V)>>(iter: T) -> Self {
        let entries = sort_and_deduplicate(iter.into_iter().collect());
        Self(entries.into_boxed_slice())
    }
}

impl<K, V> From<std::collections::BTreeMap<K, V>> for FrozenMap<K, V> {
    fn from(map: std::collections::BTreeMap<K, V>) -> Self {
        Self(map.into_iter().collect())
    }
}

impl<K: Ord, V, S> From<std::collections::HashMap<K, V, S>> for FrozenMap<K, V> {
    fn from(map: std::collections::HashMap<K, V, S>) -> Self {
        Self::from_entries(map.into_iter().collect())
    }
}

impl<K: Ord, V> FrozenMap<K, V> {
    /// Freeze entries that are already in strictly increasing key order.
    ///
    /// Duplicate or unsorted keys would make lookups by binary search unreliable.
    pub fn from_sorted(entries: Vec<(K, V)>) -> Self {
        debug_assert!(entries.windows(2).all(|w| w[0].0 < w[1].0));
        Self(entries.into_boxed_slice())
    }

    /// Creates a frozen map from entries with unique keys.
    pub(crate) fn from_entries(mut entries: Vec<(K, V)>) -> Self {
        entries.sort_unstable_by(|(left, _), (right, _)| left.cmp(right));
        debug_assert!(
            entries
                .windows(2)
                .all(|entries| entries[0].0 != entries[1].0),
            "frozen map keys must be unique",
        );
        Self(entries.into_boxed_slice())
    }

    pub fn get(&self, key: &K) -> Option<&V> {
        self.0
            .binary_search_by(|(candidate, _)| candidate.cmp(key))
            .ok()
            .map(|index| &self.0[index].1)
    }
}

impl<K, V> Default for FrozenMap<K, V> {
    fn default() -> Self {
        Self(Box::default())
    }
}

impl<K: Ord, V> std::ops::Index<&K> for FrozenMap<K, V> {
    type Output = V;

    #[track_caller]
    fn index(&self, index: &K) -> &Self::Output {
        self.get(index).expect("key not found")
    }
}

impl<K, V> IntoIterator for FrozenMap<K, V> {
    type Item = (K, V);
    type IntoIter = std::vec::IntoIter<(K, V)>;

    fn into_iter(self) -> Self::IntoIter {
        self.0.into_vec().into_iter()
    }
}

impl<'a, K, V> IntoIterator for &'a FrozenMap<K, V> {
    type Item = &'a (K, V);
    type IntoIter = std::slice::Iter<'a, (K, V)>;

    fn into_iter(self) -> Self::IntoIter {
        self.0.iter()
    }
}

impl<'a, K, V> IntoIterator for &'a mut FrozenMap<K, V> {
    type Item = (&'a K, &'a mut V);
    type IntoIter =
        std::iter::Map<std::slice::IterMut<'a, (K, V)>, fn(&'a mut (K, V)) -> (&'a K, &'a mut V)>;

    fn into_iter(self) -> Self::IntoIter {
        self.0.iter_mut().map(|(key, value)| (&*key, value))
    }
}

/// Sorts entries by key and removes duplicate keys, retaining the last value for each key.
///
/// Stable sorting preserves the input order of equal-key entries, allowing the deduplication
/// pass to provide last-entry-wins semantics like standard map collection.
fn sort_and_deduplicate<K: Ord, V>(mut entries: Vec<(K, V)>) -> Vec<(K, V)> {
    entries.sort_by(|(left, _), (right, _)| left.cmp(right));
    entries.dedup_by(|(later_key, later_value), (earlier_key, earlier_value)| {
        if later_key == earlier_key {
            // `dedup_by` removes the later entry, so move its value to the retained entry.
            std::mem::swap(later_value, earlier_value);
            true
        } else {
            false
        }
    });

    entries
}

/// Compact immutable keys stored in ascending order.
///
/// Analysis builds these sets with hash sets, but after construction they only need membership
/// tests and iteration. A sorted slice avoids retaining hash-table capacity.
#[derive(Debug, Eq, PartialEq, get_size2::GetSize)]
pub struct FrozenSet<K>(Box<[K]>);

impl<K: Ord, S> From<std::collections::HashSet<K, S>> for FrozenSet<K> {
    fn from(set: std::collections::HashSet<K, S>) -> Self {
        let mut entries = set.into_iter().collect::<Vec<_>>();
        entries.sort_unstable();
        Self(entries.into_boxed_slice())
    }
}

impl<K: Ord> FrozenSet<K> {
    pub fn contains(&self, key: &K) -> bool {
        self.0.binary_search(key).is_ok()
    }
}

impl<K> FrozenSet<K> {
    pub fn iter(&self) -> std::slice::Iter<'_, K> {
        self.0.iter()
    }
}

impl<'a, K> IntoIterator for &'a FrozenSet<K> {
    type Item = &'a K;
    type IntoIter = std::slice::Iter<'a, K>;

    fn into_iter(self) -> Self::IntoIter {
        self.0.iter()
    }
}

impl<K> Default for FrozenSet<K> {
    fn default() -> Self {
        Self(Box::default())
    }
}

#[cfg(test)]
mod tests {
    use std::assert_matches;

    use super::{FrozenMap, PackedIds};

    #[test]
    fn packed_ids_preserve_values_at_width_boundaries() {
        let pack = |max| {
            let ids = PackedIds::from(vec![max, 0]);
            assert_eq!(ids.get(0), max);
            assert_eq!(ids.get(1), 0);
            ids
        };

        assert_matches!(pack(255), PackedIds::U8(_));
        assert_matches!(pack(256), PackedIds::U16(_));
        assert_matches!(pack(65_535), PackedIds::U16(_));
        assert_matches!(pack(65_536), PackedIds::Full(_));
        assert_matches!(pack(u32::MAX), PackedIds::Full(_));
    }

    #[test]
    fn frozen_map_preserves_last_value_for_duplicate_keys() {
        let map = FrozenMap::from_iter([(2, "two"), (1, "first"), (1, "last")]);

        assert_eq!(
            map.iter().copied().collect::<Vec<_>>(),
            vec![(1, "last"), (2, "two")]
        );
    }

    #[test]
    fn frozen_map_iterates_with_mutable_values() {
        let mut map = FrozenMap::from_iter([(2, 20), (1, 10)]);

        for (key, value) in &mut map {
            *value += *key;
        }

        assert_eq!(
            map.iter().copied().collect::<Vec<_>>(),
            vec![(1, 11), (2, 22)]
        );
    }
}
