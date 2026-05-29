use crate::{Idx, IndexSlice, IndexVec};
use std::fmt::{Debug, Formatter};
use std::marker::PhantomData;
use std::ops::{Deref, DerefMut};

/// An immutable boxed sequence of `T` indexed by `I`.
#[derive(Clone, PartialEq, Eq, Hash, get_size2::GetSize)]
#[repr(transparent)]
pub struct IndexBox<I, T> {
    raw: Box<[T]>,
    index: PhantomData<I>,
}

impl<I: Idx, T> IndexBox<I, T> {
    #[inline]
    pub fn from_raw(raw: Box<[T]>) -> Self {
        Self {
            raw,
            index: PhantomData,
        }
    }
}

impl<I, T> Debug for IndexBox<I, T>
where
    T: Debug,
{
    fn fmt(&self, f: &mut Formatter) -> std::fmt::Result {
        std::fmt::Debug::fmt(&self.raw, f)
    }
}

impl<I: Idx, T> Deref for IndexBox<I, T> {
    type Target = IndexSlice<I, T>;

    fn deref(&self) -> &Self::Target {
        IndexSlice::from_raw(&self.raw)
    }
}

impl<I: Idx, T> DerefMut for IndexBox<I, T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        IndexSlice::from_raw_mut(&mut self.raw)
    }
}

impl<I: Idx, T> From<IndexVec<I, T>> for IndexBox<I, T> {
    fn from(vec: IndexVec<I, T>) -> Self {
        Self::from_raw(vec.raw.into_boxed_slice())
    }
}

// Whether `IndexBox` is `Send` depends only on the data,
// not the phantom data.
#[expect(unsafe_code)]
unsafe impl<I: Idx, T> Send for IndexBox<I, T> where T: Send {}

#[expect(unsafe_code)]
#[cfg(feature = "salsa")]
unsafe impl<I, T> salsa::Update for IndexBox<I, T>
where
    T: salsa::Update,
{
    #[expect(unsafe_code)]
    unsafe fn maybe_update(old_pointer: *mut Self, new_value: Self) -> bool {
        let old_box: &mut IndexBox<I, T> = unsafe { &mut *old_pointer };
        unsafe { salsa::Update::maybe_update(&raw mut old_box.raw, new_value.raw) }
    }
}
