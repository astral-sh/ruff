#[derive(Debug)]
pub(crate) enum CowMut<'a, T> {
    Owned(T),
    Borrowed(&'a mut T),
}

impl<T> std::ops::Deref for CowMut<'_, T> {
    type Target = T;
    fn deref(&self) -> &T {
        match self {
            CowMut::Owned(it) => it,
            CowMut::Borrowed(it) => it,
        }
    }
}

impl<T> std::ops::DerefMut for CowMut<'_, T> {
    fn deref_mut(&mut self) -> &mut T {
        match self {
            CowMut::Owned(it) => it,
            CowMut::Borrowed(it) => it,
        }
    }
}

impl<T: Default> Default for CowMut<'_, T> {
    fn default() -> Self {
        CowMut::Owned(T::default())
    }
}
