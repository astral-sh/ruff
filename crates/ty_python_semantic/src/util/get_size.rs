use std::ops::{Deref, DerefMut};
use std::sync::Arc;

use get_size2::GetSize;

/// By default, `Arc<T>: GetSize` requires `T: 'static` to enable tracking references
/// of the `Arc` and avoid double-counting. This method opts out of that behavior and
/// removes the `'static` requirement.
///
/// This method will just return the heap-size of the inner `T`.
pub(crate) fn untracked_arc_size<T>(arc: &Arc<T>) -> usize
where
    T: GetSize,
{
    T::get_heap_size(&**arc)
}

#[derive(Clone, Hash, PartialEq, Eq)]
pub(crate) struct ThinVecSized<T>(thin_vec::ThinVec<T>);

impl<T> ThinVecSized<T> {
    pub(crate) fn into_inner(self) -> thin_vec::ThinVec<T> {
        self.0
    }

    pub(crate) fn new() -> Self {
        Self(thin_vec::ThinVec::new())
    }
}

impl<T> Default for ThinVecSized<T> {
    fn default() -> Self {
        Self::new()
    }
}

#[allow(unsafe_code)]
unsafe impl<T> salsa::Update for ThinVecSized<T>
where
    T: salsa::Update,
{
    unsafe fn maybe_update(old_pointer: *mut Self, new_value: Self) -> bool {
        let old: &mut Self = unsafe { &mut *old_pointer };

        unsafe { salsa::Update::maybe_update(&raw mut old.0, new_value.0) }
    }
}

impl<T> std::fmt::Debug for ThinVecSized<T>
where
    T: std::fmt::Debug,
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

impl<T> From<thin_vec::ThinVec<T>> for ThinVecSized<T> {
    fn from(vec: thin_vec::ThinVec<T>) -> Self {
        Self(vec)
    }
}

impl<T> Deref for ThinVecSized<T> {
    type Target = thin_vec::ThinVec<T>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<T> DerefMut for ThinVecSized<T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl<T> GetSize for ThinVecSized<T>
where
    T: GetSize,
{
    fn get_heap_size(&self) -> usize {
        let mut total = 0;
        for v in self {
            total += GetSize::get_size(v);
        }
        let additional: usize = self.capacity() - self.len();
        total += additional * T::get_stack_size();
        total
    }
}

impl<'a, T> IntoIterator for &'a ThinVecSized<T> {
    type Item = &'a T;
    type IntoIter = std::slice::Iter<'a, T>;

    fn into_iter(self) -> Self::IntoIter {
        self.0.iter()
    }
}

impl<'a, T> IntoIterator for &'a mut ThinVecSized<T> {
    type Item = &'a mut T;
    type IntoIter = std::slice::IterMut<'a, T>;

    fn into_iter(self) -> Self::IntoIter {
        self.0.iter_mut()
    }
}

impl<T> IntoIterator for ThinVecSized<T> {
    type Item = T;
    type IntoIter = thin_vec::IntoIter<T>;

    fn into_iter(self) -> Self::IntoIter {
        self.0.into_iter()
    }
}
