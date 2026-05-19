use rustc_hash::FxHashMap;
use salsa::plumbing::AsId;

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

    pub(crate) fn contains_key(&self, key: &K) -> bool {
        self.get(key).is_some()
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

#[derive(Debug, Eq, PartialEq, salsa::Update, get_size2::GetSize)]
pub(crate) struct FrozenSalsaMap<K, V>(Box<[(K, V)]>);

impl<K: AsId, V> FrozenSalsaMap<K, V> {
    pub(crate) fn from_entries(mut entries: Vec<(K, V)>) -> Self {
        entries.sort_unstable_by_key(|(key, _)| key.as_id());
        Self(entries.into_boxed_slice())
    }

    pub(crate) fn get(&self, key: &K) -> Option<&V> {
        let key_id = key.as_id();
        self.0
            .binary_search_by(|(candidate, _)| candidate.as_id().cmp(&key_id))
            .ok()
            .map(|index| &self.0[index].1)
    }
}

impl<K: AsId, V> From<FxHashMap<K, V>> for FrozenSalsaMap<K, V> {
    fn from(map: FxHashMap<K, V>) -> Self {
        Self::from_entries(map.into_iter().collect())
    }
}

impl<K: AsId, V> std::ops::Index<&K> for FrozenSalsaMap<K, V> {
    type Output = V;

    #[track_caller]
    fn index(&self, index: &K) -> &Self::Output {
        self.get(index).expect("key not found")
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
