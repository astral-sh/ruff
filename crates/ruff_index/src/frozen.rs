use crate::{Idx, IndexSlice, IndexVec};
use std::fmt::{Debug, Formatter};
use std::marker::PhantomData;
use std::ops::{Deref, DerefMut};

/// A structurally immutable sequence of `T` indexed by `I`.
#[derive(Clone, PartialEq, Eq, Hash, get_size2::GetSize)]
#[repr(transparent)]
pub struct FrozenIndexVec<I, T> {
    raw: Box<[T]>,
    index: PhantomData<I>,
}

impl<I: Idx, T> FrozenIndexVec<I, T> {
    #[inline]
    pub fn from_raw(raw: Box<[T]>) -> Self {
        Self {
            raw,
            index: PhantomData,
        }
    }
}

impl<I, T> Debug for FrozenIndexVec<I, T>
where
    T: Debug,
{
    fn fmt(&self, f: &mut Formatter) -> std::fmt::Result {
        std::fmt::Debug::fmt(&self.raw, f)
    }
}

impl<I: Idx, T> Deref for FrozenIndexVec<I, T> {
    type Target = IndexSlice<I, T>;

    fn deref(&self) -> &Self::Target {
        IndexSlice::from_raw(&self.raw)
    }
}

impl<I: Idx, T> DerefMut for FrozenIndexVec<I, T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        IndexSlice::from_raw_mut(&mut self.raw)
    }
}

impl<I: Idx, T> From<IndexVec<I, T>> for FrozenIndexVec<I, T> {
    fn from(vec: IndexVec<I, T>) -> Self {
        Self::from_raw(vec.raw.into_boxed_slice())
    }
}

impl<I: Idx, T> FromIterator<T> for FrozenIndexVec<I, T> {
    fn from_iter<Iter: IntoIterator<Item = T>>(iter: Iter) -> Self {
        Self::from_raw(iter.into_iter().collect())
    }
}

// Whether `FrozenIndexVec` is `Send` depends only on the data,
// not the phantom data.
#[expect(unsafe_code)]
unsafe impl<I: Idx, T> Send for FrozenIndexVec<I, T> where T: Send {}

#[expect(unsafe_code)]
#[cfg(feature = "salsa")]
unsafe impl<I, T> salsa::Update for FrozenIndexVec<I, T>
where
    T: salsa::Update,
{
    #[expect(unsafe_code)]
    unsafe fn maybe_update(old_pointer: *mut Self, new_value: Self) -> bool {
        let old_box: &mut FrozenIndexVec<I, T> = unsafe { &mut *old_pointer };
        unsafe { salsa::Update::maybe_update(&raw mut old_box.raw, new_value.raw) }
    }
}

/// Compact structurally immutable key-value entries stored in key order.
#[derive(Debug, Eq, PartialEq, get_size2::GetSize)]
pub struct FrozenMap<K, V>(Box<[(K, V)]>);

impl<K, V> FrozenMap<K, V> {
    pub fn iter(&self) -> std::slice::Iter<'_, (K, V)> {
        self.0.iter()
    }

    pub fn iter_mut(&mut self) -> std::slice::IterMut<'_, (K, V)> {
        self.0.iter_mut()
    }
}

impl<K: Ord, V> FromIterator<(K, V)> for FrozenMap<K, V> {
    fn from_iter<T: IntoIterator<Item = (K, V)>>(iter: T) -> Self {
        let mut entries = iter.into_iter().collect::<Vec<_>>();
        entries.sort_unstable_by(|(left, _), (right, _)| left.cmp(right));
        Self(entries.into_boxed_slice())
    }
}

impl<K: Ord, V> FrozenMap<K, V> {
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

#[expect(unsafe_code)]
#[cfg(feature = "salsa")]
unsafe impl<K, V> salsa::Update for FrozenMap<K, V>
where
    K: salsa::Update,
    V: salsa::Update,
{
    #[expect(unsafe_code)]
    unsafe fn maybe_update(old_pointer: *mut Self, new_value: Self) -> bool {
        let old_map: &mut FrozenMap<K, V> = unsafe { &mut *old_pointer };
        unsafe { salsa::Update::maybe_update(&raw mut old_map.0, new_value.0) }
    }
}
