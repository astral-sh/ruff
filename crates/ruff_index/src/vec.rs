use crate::slice::IndexSlice;
use crate::Idx;
use std::borrow::{Borrow, BorrowMut};
use std::fmt::{Debug, Formatter};
use std::marker::PhantomData;
use std::ops::{Deref, DerefMut, RangeBounds};

/// An owned sequence of `T` indexed by `I`
#[derive(Clone, PartialEq, Eq, Hash)]
#[repr(transparent)]
pub struct IndexVec<I, T> {
    pub raw: Vec<T>,
    index: PhantomData<I>,
}

impl<I: Idx, T> IndexVec<I, T> {
    #[inline]
    pub fn new() -> Self {
        Self {
            raw: Vec::new(),
            index: PhantomData,
        }
    }

    #[inline]
    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            raw: Vec::with_capacity(capacity),
            index: PhantomData,
        }
    }

    #[inline]
    pub fn from_raw(raw: Vec<T>) -> Self {
        Self {
            raw,
            index: PhantomData,
        }
    }

    #[inline]
    pub fn drain<R: RangeBounds<usize>>(&mut self, range: R) -> impl Iterator<Item = T> + '_ {
        self.raw.drain(range)
    }

    #[inline]
    pub fn truncate(&mut self, a: usize) {
        self.raw.truncate(a);
    }

    #[inline]
    pub fn as_slice(&self) -> &IndexSlice<I, T> {
        IndexSlice::from_raw(&self.raw)
    }

    #[inline]
    pub fn as_mut_slice(&mut self) -> &mut IndexSlice<I, T> {
        IndexSlice::from_raw_mut(&mut self.raw)
    }

    #[inline]
    pub fn push(&mut self, data: T) -> I {
        let index = self.next_index();
        self.raw.push(data);
        index
    }

    #[inline]
    pub fn next_index(&self) -> I {
        I::new(self.raw.len())
    }

    #[inline]
    pub fn shrink_to_fit(&mut self) {
        self.raw.shrink_to_fit();
    }

    #[inline]
    pub fn resize(&mut self, new_len: usize, value: T)
    where
        T: Clone,
    {
        self.raw.resize(new_len, value);
    }
}

impl<I, T> Debug for IndexVec<I, T>
where
    T: Debug,
{
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        std::fmt::Debug::fmt(&self.raw, f)
    }
}

impl<I: Idx, T> Deref for IndexVec<I, T> {
    type Target = IndexSlice<I, T>;

    fn deref(&self) -> &Self::Target {
        self.as_slice()
    }
}

impl<I: Idx, T> DerefMut for IndexVec<I, T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.as_mut_slice()
    }
}

impl<I: Idx, T> Borrow<IndexSlice<I, T>> for IndexVec<I, T> {
    fn borrow(&self) -> &IndexSlice<I, T> {
        self
    }
}

impl<I: Idx, T> BorrowMut<IndexSlice<I, T>> for IndexVec<I, T> {
    fn borrow_mut(&mut self) -> &mut IndexSlice<I, T> {
        self
    }
}

impl<I, T> Extend<T> for IndexVec<I, T> {
    #[inline]
    fn extend<Iter: IntoIterator<Item = T>>(&mut self, iter: Iter) {
        self.raw.extend(iter);
    }
}

impl<I: Idx, T> FromIterator<T> for IndexVec<I, T> {
    #[inline]
    fn from_iter<Iter: IntoIterator<Item = T>>(iter: Iter) -> Self {
        Self::from_raw(Vec::from_iter(iter))
    }
}

impl<I: Idx, T> IntoIterator for IndexVec<I, T> {
    type IntoIter = std::vec::IntoIter<T>;
    type Item = T;

    #[inline]
    fn into_iter(self) -> std::vec::IntoIter<T> {
        self.raw.into_iter()
    }
}

impl<'a, I: Idx, T> IntoIterator for &'a IndexVec<I, T> {
    type IntoIter = std::slice::Iter<'a, T>;
    type Item = &'a T;

    #[inline]
    fn into_iter(self) -> std::slice::Iter<'a, T> {
        self.iter()
    }
}

impl<'a, I: Idx, T> IntoIterator for &'a mut IndexVec<I, T> {
    type IntoIter = std::slice::IterMut<'a, T>;
    type Item = &'a mut T;

    #[inline]
    fn into_iter(self) -> std::slice::IterMut<'a, T> {
        self.iter_mut()
    }
}

impl<I: Idx, T> Default for IndexVec<I, T> {
    #[inline]
    fn default() -> Self {
        IndexVec::new()
    }
}

impl<I: Idx, T, const N: usize> From<[T; N]> for IndexVec<I, T> {
    #[inline]
    fn from(array: [T; N]) -> Self {
        IndexVec::from_raw(array.into())
    }
}

// Whether `IndexVec` is `Send` depends only on the data,
// not the phantom data.
#[allow(unsafe_code)]
unsafe impl<I: Idx, T> Send for IndexVec<I, T> where T: Send {}
