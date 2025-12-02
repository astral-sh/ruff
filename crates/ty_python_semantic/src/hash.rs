use rustc_hash::FxBuildHasher;
use std::borrow::Borrow;
use std::hash::Hash;

/// Always use this instead of [`rustc_hash::FxHashSet`].
/// This struct intentionally does not implement `(Into)Iterator` because the iterator's output order will be unstable if the set depends on salsa's non-deterministic IDs or execution order.
/// Only use `unstable_iter()`, etc. if you are sure the iterator is safe to use despite that.
#[derive(Debug, Clone, get_size2::GetSize)]
pub struct FxHashSet<V>(rustc_hash::FxHashSet<V>);

impl<V: Eq + Hash> PartialEq for FxHashSet<V> {
    fn eq(&self, other: &Self) -> bool {
        self.0 == other.0
    }
}

impl<V: Eq + Hash> Eq for FxHashSet<V> {}

impl<V> Default for FxHashSet<V> {
    fn default() -> Self {
        Self(rustc_hash::FxHashSet::default())
    }
}

#[allow(unsafe_code)]
unsafe impl<V: Eq + Hash + salsa::Update> salsa::Update for FxHashSet<V> {
    unsafe fn maybe_update(old_pointer: *mut Self, new_value: Self) -> bool {
        unsafe { rustc_hash::FxHashSet::maybe_update(&raw mut (*old_pointer).0, new_value.0) }
    }
}

impl<V: Eq + Hash> FromIterator<V> for FxHashSet<V> {
    fn from_iter<T: IntoIterator<Item = V>>(iter: T) -> Self {
        Self(rustc_hash::FxHashSet::from_iter(iter))
    }
}

impl<V> std::ops::Deref for FxHashSet<V> {
    type Target = rustc_hash::FxHashSet<V>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<V: Eq + Hash> FxHashSet<V> {
    pub fn with_capacity_and_hasher(capacity: usize, hasher: FxBuildHasher) -> Self {
        Self(rustc_hash::FxHashSet::with_capacity_and_hasher(
            capacity, hasher,
        ))
    }

    pub fn shrink_to_fit(&mut self) {
        self.0.shrink_to_fit();
    }

    pub fn clear(&mut self) {
        self.0.clear();
    }

    pub fn len(&self) -> usize {
        self.0.len()
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    pub fn extend<I: IntoIterator<Item = V>>(&mut self, iter: I) {
        self.0.extend(iter);
    }

    pub fn insert(&mut self, value: V) -> bool {
        self.0.insert(value)
    }

    pub fn remove<Q: ?Sized + Hash + Eq>(&mut self, value: &Q) -> bool
    where
        V: Borrow<Q>,
    {
        self.0.remove(value)
    }

    pub fn contains<Q: ?Sized + Hash + Eq>(&self, value: &Q) -> bool
    where
        V: Borrow<Q>,
    {
        self.0.contains(value)
    }

    /// Unstable iterator: ordering may be inconsistent across environments and versions. Use this only if you are sure this instability will not be a problem for your use case.
    pub fn unstable_iter(&self) -> std::collections::hash_set::Iter<'_, V> {
        self.0.iter()
    }

    /// Unstable iterator: ordering may be inconsistent across environments and versions. Use this only if you are sure this instability will not be a problem for your use case.
    pub fn unstable_into_iter(self) -> std::collections::hash_set::IntoIter<V> {
        self.0.into_iter()
    }
}

impl<V: Ord> FxHashSet<V> {
    /// If you use this often, consider using `BTreeMap` instead of `FxHashMap`.
    pub fn sorted_ref_vec(&self) -> Vec<&V> {
        let mut vec: Vec<&V> = self.0.iter().collect();
        vec.sort();
        vec
    }

    /// If you use this often, consider using `BTreeMap` instead of `FxHashMap`.
    pub fn into_sorted_vec(self) -> Vec<V> {
        let mut vec: Vec<V> = self.0.into_iter().collect();
        vec.sort();
        vec
    }
}

/// Always use this instead of [`rustc_hash::FxHashMap`].
/// This struct intentionally does not implement `(Into)Iterator` because the iterator's output order will be unstable if the map depends on salsa's non-deterministic IDs or execution order.
/// Only use `unstable_iter()`, etc. if you are sure the iterator is safe to use despite that.
#[derive(Debug, Clone, get_size2::GetSize)]
pub struct FxHashMap<K, V>(rustc_hash::FxHashMap<K, V>);

impl<K: Eq + Hash, V: PartialEq> PartialEq for FxHashMap<K, V> {
    fn eq(&self, other: &Self) -> bool {
        self.0 == other.0
    }
}

impl<K: Eq + Hash, V: Eq> Eq for FxHashMap<K, V> {}

impl<K, V> Default for FxHashMap<K, V> {
    fn default() -> Self {
        Self(rustc_hash::FxHashMap::default())
    }
}

impl<K: Eq + Hash, V> std::ops::Index<&K> for FxHashMap<K, V> {
    type Output = V;

    fn index(&self, index: &K) -> &Self::Output {
        &self.0[index]
    }
}

#[allow(unsafe_code)]
unsafe impl<K: Eq + Hash + salsa::Update, V: salsa::Update> salsa::Update for FxHashMap<K, V> {
    unsafe fn maybe_update(old_pointer: *mut Self, new_value: Self) -> bool {
        unsafe { rustc_hash::FxHashMap::maybe_update(&raw mut (*old_pointer).0, new_value.0) }
    }
}

impl<K: Eq + Hash, V> FromIterator<(K, V)> for FxHashMap<K, V> {
    fn from_iter<T: IntoIterator<Item = (K, V)>>(iter: T) -> Self {
        Self(rustc_hash::FxHashMap::from_iter(iter))
    }
}

impl<K: Eq + Hash, V> FxHashMap<K, V> {
    pub fn with_capacity_and_hasher(capacity: usize, hasher: FxBuildHasher) -> Self {
        Self(rustc_hash::FxHashMap::with_capacity_and_hasher(
            capacity, hasher,
        ))
    }

    pub fn shrink_to_fit(&mut self) {
        self.0.shrink_to_fit();
    }

    pub fn clear(&mut self) {
        self.0.clear();
    }

    pub fn len(&self) -> usize {
        self.0.len()
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    pub fn get<Q: ?Sized + Hash + Eq>(&self, k: &Q) -> Option<&V>
    where
        K: Borrow<Q>,
    {
        self.0.get(k)
    }

    pub fn get_mut<Q: ?Sized + Hash + Eq>(&mut self, k: &Q) -> Option<&mut V>
    where
        K: Borrow<Q>,
    {
        self.0.get_mut(k)
    }

    pub fn entry(&mut self, k: K) -> std::collections::hash_map::Entry<'_, K, V> {
        self.0.entry(k)
    }

    pub fn insert(&mut self, k: K, v: V) -> Option<V> {
        self.0.insert(k, v)
    }

    pub fn extend<I: IntoIterator<Item = (K, V)>>(&mut self, iter: I) {
        self.0.extend(iter);
    }

    pub fn remove<Q: ?Sized + Hash + Eq>(&mut self, k: &Q) -> Option<V>
    where
        K: Borrow<Q>,
    {
        self.0.remove(k)
    }

    pub fn contains_key<Q: ?Sized + Hash + Eq>(&self, k: &Q) -> bool
    where
        K: Borrow<Q>,
    {
        self.0.contains_key(k)
    }

    pub fn retain<F>(&mut self, f: F)
    where
        F: FnMut(&K, &mut V) -> bool,
    {
        self.0.retain(f);
    }

    /// Unstable iterator: ordering may be inconsistent across environments and versions. Use this only if you are sure this instability will not be a problem for your use case.
    pub fn unstable_iter(&self) -> std::collections::hash_map::Iter<'_, K, V> {
        self.0.iter()
    }

    pub fn unstable_iter_copied(&self) -> impl Iterator<Item = (K, V)>
    where
        K: Copy,
        V: Copy,
    {
        self.0.iter().map(|(k, v)| (*k, *v))
    }

    /// Unstable iterator: ordering may be inconsistent across environments and versions. Use this only if you are sure this instability will not be a problem for your use case.
    pub fn unstable_keys(&self) -> std::collections::hash_map::Keys<'_, K, V> {
        self.0.keys()
    }

    /// Unstable iterator: ordering may be inconsistent across environments and versions. Use this only if you are sure this instability will not be a problem for your use case.
    pub fn unstable_values(&self) -> std::collections::hash_map::Values<'_, K, V> {
        self.0.values()
    }

    /// Unstable iterator: ordering may be inconsistent across environments and versions. Use this only if you are sure this instability will not be a problem for your use case.
    pub fn unstable_iter_mut(&mut self) -> std::collections::hash_map::IterMut<'_, K, V> {
        self.0.iter_mut()
    }

    /// Unstable iterator: ordering may be inconsistent across environments and versions. Use this only if you are sure this instability will not be a problem for your use case.
    pub fn unstable_values_mut(&mut self) -> std::collections::hash_map::ValuesMut<'_, K, V> {
        self.0.values_mut()
    }

    /// Unstable iterator: ordering may be inconsistent across environments and versions. Use this only if you are sure this instability will not be a problem for your use case.
    pub fn unstable_into_iter(self) -> std::collections::hash_map::IntoIter<K, V> {
        self.0.into_iter()
    }

    /// Unstable iterator: ordering may be inconsistent across environments and versions. Use this only if you are sure this instability will not be a problem for your use case.
    pub fn unstable_into_keys(self) -> std::collections::hash_map::IntoKeys<K, V> {
        self.0.into_keys()
    }

    /// Unstable iterator: ordering may be inconsistent across environments and versions. Use this only if you are sure this instability will not be a problem for your use case.
    pub fn unstable_into_values(self) -> std::collections::hash_map::IntoValues<K, V> {
        self.0.into_values()
    }
}

impl<K: Ord, V> FxHashMap<K, V> {
    /// If you use this often, consider using `BTreeMap` instead of `FxHashMap`.
    pub fn sorted_key_ref_vec(&self) -> Vec<&K> {
        let mut vec: Vec<&K> = self.0.keys().collect();
        vec.sort();
        vec
    }

    /// If you use this often, consider using `BTreeMap` instead of `FxHashMap`.
    pub fn into_sorted_key_vec(self) -> Vec<K> {
        let mut vec: Vec<K> = self.0.into_keys().collect();
        vec.sort();
        vec
    }

    /// If you use this often, consider using `BTreeMap` instead of `FxHashMap`.
    pub fn sorted_ref_vec(&self) -> Vec<(&K, &V)> {
        let mut vec: Vec<(&K, &V)> = self.0.iter().collect();
        vec.sort_by(|(k1, _), (k2, _)| k1.cmp(k2));
        vec
    }

    /// If you use this often, consider using `BTreeMap` instead of `FxHashMap`.
    pub fn into_sorted_vec(self) -> Vec<(K, V)> {
        let mut vec: Vec<(K, V)> = self.0.into_iter().collect();
        vec.sort_by(|(k1, _), (k2, _)| k1.cmp(k2));
        vec
    }
}
