use crate::{Idx, IndexSlice, IndexVec};
use std::fmt::{Debug, Formatter};
use std::marker::PhantomData;
use std::ops::{Deref, DerefMut};

/// A structurally immutable sequence of `T` indexed by `I`.
#[derive(Clone, PartialEq, Eq, Hash, get_size2::GetSize)]
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

    #[inline]
    pub fn as_slice(&self) -> &IndexSlice<I, T> {
        IndexSlice::from_raw(&self.raw)
    }

    #[inline]
    pub fn as_mut_slice(&mut self) -> &mut IndexSlice<I, T> {
        IndexSlice::from_raw_mut(&mut self.raw)
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

    #[inline]
    fn deref(&self) -> &Self::Target {
        self.as_slice()
    }
}

impl<I: Idx, T> DerefMut for FrozenIndexVec<I, T> {
    #[inline]
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.as_mut_slice()
    }
}

impl<I: Idx, T> From<IndexVec<I, T>> for FrozenIndexVec<I, T> {
    #[inline]
    fn from(vec: IndexVec<I, T>) -> Self {
        Self::from_raw(vec.raw.into_boxed_slice())
    }
}

impl<I: Idx, T> FromIterator<T> for FrozenIndexVec<I, T> {
    #[inline]
    fn from_iter<Iter: IntoIterator<Item = T>>(iter: Iter) -> Self {
        Self::from_raw(iter.into_iter().collect())
    }
}

// Whether `FrozenIndexVec` is `Send` depends only on the data,
// not the phantom data.
#[expect(unsafe_code)]
unsafe impl<I: Idx, T> Send for FrozenIndexVec<I, T> where T: Send {}

// SAFETY: `FrozenIndexVec` owns its elements; `I` is only a marker.
#[expect(unsafe_code)]
#[cfg(feature = "salsa")]
unsafe impl<I, T: salsa::SalsaValue> salsa::SalsaValue for FrozenIndexVec<I, T> {}
