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
    pub(crate) fn from_entries(mut entries: Vec<(K, V)>) -> Self {
        entries.sort_unstable_by(|(left, _), (right, _)| left.cmp(right));
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

/// Compact immutable key-value entries stored in parallel key and value slices.
#[derive(Debug, Eq, PartialEq, salsa::Update, get_size2::GetSize)]
pub struct FrozenParallelMap<K, V> {
    keys: Box<[K]>,
    values: Box<[V]>,
}

impl<K, V> FrozenParallelMap<K, V> {
    pub fn get(&self, key: &K) -> Option<&V>
    where
        K: Ord,
    {
        self.keys
            .binary_search(key)
            .ok()
            .map(|index| &self.values[index])
    }

    pub fn iter(&self) -> impl DoubleEndedIterator<Item = (K, V)> + ExactSizeIterator + '_
    where
        K: Copy,
        V: Copy,
    {
        self.keys.iter().copied().zip(self.values.iter().copied())
    }

    pub fn map_values<F>(&mut self, mut map: F)
    where
        K: Copy,
        V: Copy,
        F: FnMut(K, V) -> V,
    {
        for (key, value) in self.keys.iter().copied().zip(self.values.iter_mut()) {
            *value = map(key, *value);
        }
    }

    fn from_entries(entries: Vec<(K, V)>) -> Self {
        let mut keys = Vec::with_capacity(entries.len());
        let mut values = Vec::with_capacity(entries.len());
        for (key, value) in entries {
            keys.push(key);
            values.push(value);
        }

        Self {
            keys: keys.into_boxed_slice(),
            values: values.into_boxed_slice(),
        }
    }
}

impl<K: Ord, V> FromIterator<(K, V)> for FrozenParallelMap<K, V> {
    fn from_iter<T: IntoIterator<Item = (K, V)>>(iter: T) -> Self {
        let mut entries = iter.into_iter().collect::<Vec<_>>();
        entries.sort_unstable_by(|(left, _), (right, _)| left.cmp(right));
        entries.dedup_by(|(left, _), (right, _)| left == right);
        Self::from_entries(entries)
    }
}

impl<K: Ord, V, S> From<std::collections::HashMap<K, V, S>> for FrozenParallelMap<K, V> {
    fn from(map: std::collections::HashMap<K, V, S>) -> Self {
        let mut entries = map.into_iter().collect::<Vec<_>>();
        entries.sort_unstable_by(|(left, _), (right, _)| left.cmp(right));
        Self::from_entries(entries)
    }
}

impl<K, V> Default for FrozenParallelMap<K, V> {
    fn default() -> Self {
        Self {
            keys: Box::default(),
            values: Box::default(),
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
    use super::{FrozenMap, FrozenParallelMap};

    #[test]
    fn frozen_parallel_map_sorts_entries() {
        let map = FrozenParallelMap::from_iter([(3, [1; 4]), (1, [2; 4]), (2, [1; 4])]);

        assert_eq!(map.get(&1), Some(&[2; 4]));
        assert_eq!(map.get(&2), Some(&[1; 4]));
        assert_eq!(
            map.iter().collect::<Vec<_>>(),
            vec![(1, [2; 4]), (2, [1; 4]), (3, [1; 4])]
        );
    }

    #[test]
    fn frozen_parallel_map_updates_values() {
        let mut map = FrozenParallelMap::from_iter([(1, 10), (2, 20), (3, 30)]);

        map.map_values(|_, _| 42);

        assert_eq!(map.values.as_ref(), &[42, 42, 42]);
        assert_eq!(
            map.iter().collect::<Vec<_>>(),
            vec![(1, 42), (2, 42), (3, 42)]
        );
    }

    #[test]
    fn frozen_parallel_map_uses_less_heap_for_aligned_values() {
        let entries = [
            (1, [1_u64; 8]),
            (2, [1_u64; 8]),
            (3, [1_u64; 8]),
            (4, [2_u64; 8]),
        ];
        let direct = FrozenMap::from_iter(entries);
        let parallel = FrozenParallelMap::from_iter(entries);

        assert!(ruff_memory_usage::heap_size(&parallel) < ruff_memory_usage::heap_size(&direct));
    }
}
