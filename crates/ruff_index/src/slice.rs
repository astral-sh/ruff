use crate::vec::IndexVec;
use crate::Idx;
use std::fmt::{Debug, Formatter};
use std::marker::PhantomData;
use std::ops::{Index, IndexMut, Range};

/// A view into contiguous `T`s, indexed by `I` rather than by `usize`.
#[derive(PartialEq, Eq, Hash)]
#[repr(transparent)]
pub struct IndexSlice<I, T> {
    index: PhantomData<I>,
    pub raw: [T],
}

impl<I: Idx, T> IndexSlice<I, T> {
    #[inline]
    pub const fn empty() -> &'static Self {
        Self::from_raw(&[])
    }

    #[inline]
    pub const fn from_raw(raw: &[T]) -> &Self {
        let ptr: *const [T] = raw;

        #[allow(unsafe_code)]
        // SAFETY: `IndexSlice` is `repr(transparent)` over a normal slice
        unsafe {
            &*(ptr as *const Self)
        }
    }

    #[inline]
    pub fn from_raw_mut(raw: &mut [T]) -> &mut Self {
        let ptr: *mut [T] = raw;

        #[allow(unsafe_code)]
        // SAFETY: `IndexSlice` is `repr(transparent)` over a normal slice
        unsafe {
            &mut *(ptr as *mut Self)
        }
    }

    #[inline]
    pub const fn first(&self) -> Option<&T> {
        self.raw.first()
    }

    #[inline]
    pub const fn len(&self) -> usize {
        self.raw.len()
    }

    #[inline]
    pub const fn is_empty(&self) -> bool {
        self.raw.is_empty()
    }

    #[inline]
    pub fn iter(&self) -> std::slice::Iter<'_, T> {
        self.raw.iter()
    }

    /// Returns an iterator over the indices
    #[inline]
    pub fn indices(
        &self,
    ) -> impl DoubleEndedIterator<Item = I> + ExactSizeIterator + Clone + 'static {
        (0..self.len()).map(|n| I::new(n))
    }

    #[inline]
    pub fn iter_enumerated(
        &self,
    ) -> impl DoubleEndedIterator<Item = (I, &T)> + ExactSizeIterator + '_ {
        self.raw.iter().enumerate().map(|(n, t)| (I::new(n), t))
    }

    #[inline]
    pub fn iter_mut(&mut self) -> std::slice::IterMut<'_, T> {
        self.raw.iter_mut()
    }

    #[inline]
    pub fn iter_mut_enumerated(
        &mut self,
    ) -> impl DoubleEndedIterator<Item = (I, &mut T)> + ExactSizeIterator + '_ {
        self.raw.iter_mut().enumerate().map(|(n, t)| (I::new(n), t))
    }

    #[inline]
    pub fn last_index(&self) -> Option<I> {
        self.len().checked_sub(1).map(I::new)
    }

    #[inline]
    pub fn swap(&mut self, a: I, b: I) {
        self.raw.swap(a.index(), b.index());
    }

    #[inline]
    pub fn get(&self, index: I) -> Option<&T> {
        self.raw.get(index.index())
    }

    #[inline]
    pub fn get_mut(&mut self, index: I) -> Option<&mut T> {
        self.raw.get_mut(index.index())
    }

    #[inline]
    pub fn binary_search(&self, value: &T) -> Result<I, I>
    where
        T: Ord,
    {
        match self.raw.binary_search(value) {
            Ok(i) => Ok(Idx::new(i)),
            Err(i) => Err(Idx::new(i)),
        }
    }
}

impl<I, T> Debug for IndexSlice<I, T>
where
    I: Idx,
    T: Debug,
{
    fn fmt(&self, fmt: &mut Formatter<'_>) -> std::fmt::Result {
        std::fmt::Debug::fmt(&self.raw, fmt)
    }
}

impl<I: Idx, T> Index<I> for IndexSlice<I, T> {
    type Output = T;

    #[inline]
    fn index(&self, index: I) -> &T {
        &self.raw[index.index()]
    }
}

impl<I: Idx, T> Index<Range<I>> for IndexSlice<I, T> {
    type Output = [T];

    #[inline]
    fn index(&self, range: Range<I>) -> &[T] {
        &self.raw[range.start.index()..range.end.index()]
    }
}

impl<I: Idx, T> IndexMut<I> for IndexSlice<I, T> {
    #[inline]
    fn index_mut(&mut self, index: I) -> &mut T {
        &mut self.raw[index.index()]
    }
}

impl<I: Idx, T> IndexMut<Range<I>> for IndexSlice<I, T> {
    #[inline]
    fn index_mut(&mut self, range: Range<I>) -> &mut [T] {
        &mut self.raw[range.start.index()..range.end.index()]
    }
}

impl<'a, I: Idx, T> IntoIterator for &'a IndexSlice<I, T> {
    type IntoIter = std::slice::Iter<'a, T>;
    type Item = &'a T;

    #[inline]
    fn into_iter(self) -> std::slice::Iter<'a, T> {
        self.raw.iter()
    }
}

impl<'a, I: Idx, T> IntoIterator for &'a mut IndexSlice<I, T> {
    type IntoIter = std::slice::IterMut<'a, T>;
    type Item = &'a mut T;

    #[inline]
    fn into_iter(self) -> std::slice::IterMut<'a, T> {
        self.raw.iter_mut()
    }
}

impl<I: Idx, T: Clone> ToOwned for IndexSlice<I, T> {
    type Owned = IndexVec<I, T>;

    fn to_owned(&self) -> IndexVec<I, T> {
        IndexVec::from_raw(self.raw.to_owned())
    }

    fn clone_into(&self, target: &mut IndexVec<I, T>) {
        self.raw.clone_into(&mut target.raw);
    }
}

impl<I: Idx, T> Default for &IndexSlice<I, T> {
    #[inline]
    fn default() -> Self {
        IndexSlice::from_raw(Default::default())
    }
}

impl<I: Idx, T> Default for &mut IndexSlice<I, T> {
    #[inline]
    fn default() -> Self {
        IndexSlice::from_raw_mut(Default::default())
    }
}

// Whether `IndexSlice` is `Send` depends only on the data,
// not the phantom data.
#[allow(unsafe_code)]
unsafe impl<I: Idx, T> Send for IndexSlice<I, T> where T: Send {}
