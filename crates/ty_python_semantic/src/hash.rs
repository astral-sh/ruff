use rustc_hash::FxBuildHasher;
use std::hash::Hash;

/// Always use this instead of [`rustc_hash::FxHashSet`].
/// This struct intentionally does not implement `(Into)Iterator` because the iterator's output order will be unstable if the set's values depend on salsa's non-deterministic IDs.
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

impl<V> std::ops::DerefMut for FxHashSet<V> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl<V: Eq + Hash> FxHashSet<V> {
    pub fn with_capacity_and_hasher(capacity: usize, hasher: FxBuildHasher) -> Self {
        Self(rustc_hash::FxHashSet::with_capacity_and_hasher(
            capacity, hasher,
        ))
    }

    /// Unstable iterator: ordering may be inconsistent across environments and versions. Use this only if you are sure this instability will not be a problem for your use case.
    pub fn unstable_iter(&self) -> std::collections::hash_set::Iter<'_, V> {
        self.0.iter()
    }

    /// Unstable iterator: ordering may be inconsistent across environments and versions. Use this only if you are sure this instability will not be a problem for your use case.
    pub fn unstable_into_iter(self) -> std::collections::hash_set::IntoIter<V> {
        self.0.into_iter()
    }

    #[track_caller]
    #[allow(clippy::iter_without_into_iter)]
    #[deprecated(
        note = "FxHashSet does not guarantee stable iteration order; use FxIndexSet or unstable_iter() instead"
    )]
    pub fn iter(&self) -> std::collections::hash_set::Iter<'_, V> {
        panic!(
            "FxHashSet does not guarantee stable iteration order; use FxIndexSet or unstable_iter() instead"
        );
    }

    #[track_caller]
    #[allow(clippy::should_implement_trait)]
    #[deprecated(
        note = "FxHashSet does not guarantee stable iteration order; use FxIndexSet or unstable_into_iter() instead"
    )]
    pub fn into_iter(self) -> std::collections::hash_set::IntoIter<V> {
        panic!(
            "FxHashSet does not guarantee stable iteration order; use FxIndexSet or unstable_into_iter() instead"
        );
    }
}

impl<V: Ord> FxHashSet<V> {
    pub fn sorted_ref_vec(&self) -> Vec<&V> {
        let mut vec: Vec<&V> = self.0.iter().collect();
        vec.sort();
        vec
    }

    pub fn into_sorted_vec(self) -> Vec<V> {
        let mut vec: Vec<V> = self.0.into_iter().collect();
        vec.sort();
        vec
    }
}

/// Always use this instead of [`rustc_hash::FxHashMap`].
/// This struct intentionally does not implement `(Into)Iterator` because the iterator's output order will be unstable if the map's keys depend on salsa's non-deterministic IDs.
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

impl<K, V> std::ops::Deref for FxHashMap<K, V> {
    type Target = rustc_hash::FxHashMap<K, V>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<K, V> std::ops::DerefMut for FxHashMap<K, V> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl<K: Eq + Hash, V> FxHashMap<K, V> {
    pub fn with_capacity_and_hasher(capacity: usize, hasher: FxBuildHasher) -> Self {
        Self(rustc_hash::FxHashMap::with_capacity_and_hasher(
            capacity, hasher,
        ))
    }

    /// Unstable iterator: ordering may be inconsistent across environments and versions. Use this only if you are sure this instability will not be a problem for your use case.
    pub fn unstable_iter(&self) -> std::collections::hash_map::Iter<'_, K, V> {
        self.0.iter()
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

    #[track_caller]
    #[allow(clippy::iter_without_into_iter)]
    #[deprecated(
        note = "FxHashMap does not guarantee stable iteration order; use FxIndexMap or unstable_iter() instead"
    )]
    pub fn iter(&self) -> std::collections::hash_map::Iter<'_, K, V> {
        panic!(
            "FxHashMap does not guarantee stable iteration order; use FxIndexMap or unstable_iter() instead"
        );
    }

    #[track_caller]
    #[deprecated(
        note = "FxHashMap does not guarantee stable iteration order; use FxIndexMap or unstable_keys() instead"
    )]
    pub fn keys(&self) -> std::collections::hash_map::Keys<'_, K, V> {
        panic!(
            "FxHashMap does not guarantee stable iteration order; use FxIndexMap or unstable_keys() instead"
        );
    }

    #[track_caller]
    #[deprecated(
        note = "FxHashMap does not guarantee stable iteration order; use FxIndexMap or unstable_values() instead"
    )]
    pub fn values(&self) -> std::collections::hash_map::Values<'_, K, V> {
        panic!(
            "FxHashMap does not guarantee stable iteration order; use FxIndexMap or unstable_values() instead"
        );
    }

    #[track_caller]
    #[allow(clippy::iter_without_into_iter)]
    #[deprecated(
        note = "FxHashMap does not guarantee stable iteration order; use FxIndexMap or unstable_iter_mut() instead"
    )]
    pub fn iter_mut(&mut self) -> std::collections::hash_map::IterMut<'_, K, V> {
        panic!(
            "FxHashMap does not guarantee stable iteration order; use FxIndexMap or unstable_iter_mut() instead"
        );
    }

    #[track_caller]
    #[deprecated(
        note = "FxHashMap does not guarantee stable iteration order; use FxIndexMap or unstable_values_mut() instead"
    )]
    pub fn values_mut(&mut self) -> std::collections::hash_map::ValuesMut<'_, K, V> {
        panic!(
            "FxHashMap does not guarantee stable iteration order; use FxIndexMap or unstable_values_mut() instead"
        );
    }

    #[track_caller]
    #[allow(clippy::should_implement_trait)]
    #[deprecated(
        note = "FxHashMap does not guarantee stable iteration order; use FxIndexMap or unstable_into_iter() instead"
    )]
    pub fn into_iter(self) -> std::collections::hash_map::IntoIter<K, V> {
        panic!(
            "FxHashMap does not guarantee stable iteration order; use FxIndexMap or unstable_into_iter() instead"
        );
    }

    #[track_caller]
    #[deprecated(
        note = "FxHashMap does not guarantee stable iteration order; use FxIndexMap or unstable_into_keys() instead"
    )]
    pub fn into_keys(self) -> std::collections::hash_map::IntoKeys<K, V> {
        panic!(
            "FxHashMap does not guarantee stable iteration order; use FxIndexMap or unstable_into_keys() instead"
        );
    }

    #[track_caller]
    #[deprecated(
        note = "FxHashMap does not guarantee stable iteration order; use FxIndexMap or unstable_into_values() instead"
    )]
    pub fn into_values(self) -> std::collections::hash_map::IntoValues<K, V> {
        panic!(
            "FxHashMap does not guarantee stable iteration order; use FxIndexMap or unstable_into_values() instead"
        );
    }
}
