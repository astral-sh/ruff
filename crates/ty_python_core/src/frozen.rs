use std::hash::Hash;

use ruff_index::{FrozenIndexVec, IndexVec, newtype_index};
use rustc_hash::FxHashMap;

/// Compact immutable key-value entries stored in key order.
///
/// Analysis builds these tables with hash maps, but after construction they only need keyed
/// lookup. A sorted slice avoids retaining hash-table capacity for every indexed file.
#[derive(Debug, Eq, PartialEq, salsa::Update, get_size2::GetSize)]
pub struct FrozenMap<K, V>(Box<[(K, V)]>);

impl<K, V> FrozenMap<K, V> {
    pub fn iter(&self) -> std::slice::Iter<'_, (K, V)> {
        self.0.iter()
    }

    pub fn iter_mut(&mut self) -> std::slice::IterMut<'_, (K, V)> {
        self.0.iter_mut()
    }

    pub fn keys(&self) -> impl DoubleEndedIterator<Item = &K> + ExactSizeIterator {
        self.0.iter().map(|(key, _)| key)
    }

    pub fn values(&self) -> impl DoubleEndedIterator<Item = &V> + ExactSizeIterator {
        self.0.iter().map(|(_, value)| value)
    }
}

impl<K: Ord, V> FromIterator<(K, V)> for FrozenMap<K, V> {
    fn from_iter<T: IntoIterator<Item = (K, V)>>(iter: T) -> Self {
        let mut entries = iter.into_iter().collect::<Vec<_>>();
        entries.sort_unstable_by(|(left, _), (right, _)| left.cmp(right));
        entries.dedup_by(|(left, _), (right, _)| left == right);
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
    type Item = &'a mut (K, V);
    type IntoIter = std::slice::IterMut<'a, (K, V)>;

    fn into_iter(self) -> Self::IntoIter {
        self.0.iter_mut()
    }
}

#[newtype_index]
#[derive(get_size2::GetSize, salsa::Update)]
struct FrozenValueIndex;

fn index_values<K, V>(
    entries: impl IntoIterator<Item = (K, V)>,
) -> (Vec<(K, FrozenValueIndex)>, IndexVec<FrozenValueIndex, V>)
where
    V: Copy + Eq + Hash,
{
    let mut values = IndexVec::new();
    let mut value_indices = FxHashMap::default();
    let entries = entries
        .into_iter()
        .map(|(key, value)| {
            let index = *value_indices
                .entry(value)
                .or_insert_with(|| values.push(value));
            (key, index)
        })
        .collect();

    (entries, values)
}

/// Compact immutable key-value entries that deduplicate repeated values.
#[derive(Debug, Eq, PartialEq, salsa::Update, get_size2::GetSize)]
pub struct FrozenValueMap<K, V> {
    entries: FrozenMap<K, FrozenValueIndex>,
    values: FrozenIndexVec<FrozenValueIndex, V>,
}

impl<K, V> FrozenValueMap<K, V> {
    pub fn get(&self, key: &K) -> Option<&V>
    where
        K: Ord,
    {
        self.entries.get(key).map(|index| &self.values[*index])
    }

    pub fn iter(&self) -> impl DoubleEndedIterator<Item = (K, V)> + ExactSizeIterator + '_
    where
        K: Copy,
        V: Copy,
    {
        self.entries
            .iter()
            .map(|(key, index)| (*key, self.values[*index]))
    }

    pub fn map_values<F>(&mut self, mut map: F)
    where
        K: Copy + Ord,
        V: Copy + Eq + Hash,
        F: FnMut(K, V) -> V,
    {
        *self = self
            .iter()
            .map(|(key, value)| (key, map(key, value)))
            .collect();
    }
}

impl<K, V> FromIterator<(K, V)> for FrozenValueMap<K, V>
where
    K: Ord,
    V: Copy + Eq + Hash,
{
    fn from_iter<T: IntoIterator<Item = (K, V)>>(iter: T) -> Self {
        let mut source_entries = iter.into_iter().collect::<Vec<_>>();
        source_entries.sort_unstable_by(|(left, _), (right, _)| left.cmp(right));
        source_entries.dedup_by(|(left, _), (right, _)| left == right);

        let (entries, values) = index_values(source_entries);

        Self {
            entries: FrozenMap(entries.into_boxed_slice()),
            values: values.into(),
        }
    }
}

impl<K, V, S> From<std::collections::HashMap<K, V, S>> for FrozenValueMap<K, V>
where
    K: Ord,
    V: Copy + Eq + Hash,
{
    fn from(map: std::collections::HashMap<K, V, S>) -> Self {
        let (mut entries, values) = index_values(map);
        entries.sort_unstable_by(|(left, _), (right, _)| left.cmp(right));

        Self {
            entries: FrozenMap(entries.into_boxed_slice()),
            values: values.into(),
        }
    }
}

impl<K, V> Default for FrozenValueMap<K, V> {
    fn default() -> Self {
        Self {
            entries: FrozenMap::default(),
            values: IndexVec::new().into(),
        }
    }
}

/// Compact immutable keys stored in ascending order.
///
/// Analysis builds these sets with hash sets, but after construction they only need membership
/// tests and iteration. A sorted slice avoids retaining hash-table capacity.
#[derive(Debug, Eq, PartialEq, salsa::Update, get_size2::GetSize)]
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
    use super::{FrozenMap, FrozenValueMap};

    #[test]
    fn frozen_value_map_deduplicates_values() {
        let map = FrozenValueMap::from_iter([(3, [1; 4]), (1, [2; 4]), (2, [1; 4])]);

        assert_eq!(map.values.len(), 2);
        assert_eq!(map.get(&1), Some(&[2; 4]));
        assert_eq!(map.get(&2), Some(&[1; 4]));
        assert_eq!(
            map.iter().collect::<Vec<_>>(),
            vec![(1, [2; 4]), (2, [1; 4]), (3, [1; 4])]
        );
    }

    #[test]
    fn frozen_value_map_updates_and_rededuplicates_values() {
        let mut map = FrozenValueMap::from_iter([(1, 10), (2, 20), (3, 30)]);

        map.map_values(|_, _| 42);

        assert_eq!(&map.values.raw, &[42]);
        assert_eq!(
            map.iter().collect::<Vec<_>>(),
            vec![(1, 42), (2, 42), (3, 42)]
        );
    }

    #[test]
    fn frozen_value_map_uses_less_heap_for_repeated_large_values() {
        let entries = [(1, [1; 8]), (2, [1; 8]), (3, [1; 8]), (4, [2; 8])];
        let direct = FrozenMap::from_iter(entries);
        let deduplicated = FrozenValueMap::from_iter(entries);

        assert!(
            ruff_memory_usage::heap_size(&deduplicated) < ruff_memory_usage::heap_size(&direct)
        );
    }
}
