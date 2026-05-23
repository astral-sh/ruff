use std::borrow::Borrow;
use std::fmt;
use std::ops::Deref;

pub type Allocator = bumpalo::Bump;

/// An immutable AST collection whose storage is owned by an [`Allocator`].
#[derive(PartialEq)]
pub struct Slice<'allocator, T: 'allocator>(&'allocator [T]);

impl<'allocator, T> Slice<'allocator, T> {
    pub const fn new_in(_allocator: &'allocator Allocator) -> Self {
        Self(&[])
    }

    pub fn from_vec_in(values: std::vec::Vec<T>, allocator: &'allocator Allocator) -> Self {
        Self(allocator.alloc_slice_fill_iter(values))
    }

    pub fn from_iter_in(
        values: impl IntoIterator<Item = T>,
        allocator: &'allocator Allocator,
    ) -> Self {
        Self::from_vec_in(values.into_iter().collect(), allocator)
    }

    pub const fn as_slice(&self) -> &[T] {
        self.0
    }

    pub fn transform_in(
        &mut self,
        allocator: &'allocator Allocator,
        transform: impl FnOnce(&mut [T]),
    ) where
        T: Clone,
    {
        let values = allocator.alloc_slice_clone(self.0);
        transform(values);
        self.0 = values;
    }
}

impl<T> Copy for Slice<'_, T> {}

impl<T> Clone for Slice<'_, T> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<T: fmt::Debug> fmt::Debug for Slice<'_, T> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(self.0, formatter)
    }
}

impl<T> Deref for Slice<'_, T> {
    type Target = [T];

    fn deref(&self) -> &Self::Target {
        self.0
    }
}

impl<T> AsRef<[T]> for Slice<'_, T> {
    fn as_ref(&self) -> &[T] {
        self.0
    }
}

impl<'a, T> IntoIterator for &'a Slice<'_, T> {
    type Item = &'a T;
    type IntoIter = std::slice::Iter<'a, T>;

    fn into_iter(self) -> Self::IntoIter {
        self.0.iter()
    }
}

#[cfg(feature = "get-size")]
impl<T: get_size2::GetSize> get_size2::GetSize for Slice<'_, T> {
    fn get_heap_size_with_tracker<Tracker: get_size2::GetSizeTracker>(
        &self,
        tracker: Tracker,
    ) -> (usize, Tracker) {
        let (contents, tracker) = self.0.get_heap_size_with_tracker(tracker);
        (std::mem::size_of_val(self.0) + contents, tracker)
    }
}

/// An AST value allocated in an [`Allocator`].
#[derive(Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct Box<'allocator, T: ?Sized>(&'allocator T);

impl<T: ?Sized> Copy for Box<'_, T> {}

impl<T: ?Sized> Clone for Box<'_, T> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<'allocator, T> Box<'allocator, T> {
    pub fn new_in(value: T, allocator: &'allocator Allocator) -> Self {
        Self(allocator.alloc(value))
    }
}

impl<'allocator, T: ?Sized> Box<'allocator, T> {
    pub const fn from_ref(value: &'allocator T) -> Self {
        Self(value)
    }
}

impl<'allocator> Box<'allocator, str> {
    pub fn from_str_in(value: &str, allocator: &'allocator Allocator) -> Self {
        Self(allocator.alloc_str(value))
    }
}

impl<'allocator, T: Copy> Box<'allocator, [T]> {
    pub fn from_slice_copy_in(value: &[T], allocator: &'allocator Allocator) -> Self {
        Self(allocator.alloc_slice_copy(value))
    }
}

impl<T: ?Sized> AsRef<T> for Box<'_, T> {
    fn as_ref(&self) -> &T {
        self
    }
}

impl<T: ?Sized> Borrow<T> for Box<'_, T> {
    fn borrow(&self) -> &T {
        self
    }
}

impl<'allocator, T: ?Sized> From<&'allocator T> for Box<'allocator, T> {
    fn from(value: &'allocator T) -> Self {
        Self::from_ref(value)
    }
}

impl<T: ?Sized> Deref for Box<'_, T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        self.0
    }
}

impl<T: fmt::Debug + ?Sized> fmt::Debug for Box<'_, T> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(&self.0, formatter)
    }
}

impl<T: fmt::Display + ?Sized> fmt::Display for Box<'_, T> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(&self.0, formatter)
    }
}

#[cfg(feature = "get-size")]
impl<T: get_size2::GetSize + Sized> get_size2::GetSize for Box<'_, T> {
    fn get_heap_size_with_tracker<Tracker: get_size2::GetSizeTracker>(
        &self,
        tracker: Tracker,
    ) -> (usize, Tracker) {
        self.0.get_heap_size_with_tracker(tracker)
    }
}

#[cfg(feature = "get-size")]
impl get_size2::GetSize for Box<'_, str> {
    fn get_heap_size_with_tracker<Tracker: get_size2::GetSizeTracker>(
        &self,
        tracker: Tracker,
    ) -> (usize, Tracker) {
        (self.len(), tracker)
    }
}

#[cfg(feature = "get-size")]
impl<T: get_size2::GetSize> get_size2::GetSize for Box<'_, [T]> {
    fn get_heap_size_with_tracker<Tracker: get_size2::GetSizeTracker>(
        &self,
        tracker: Tracker,
    ) -> (usize, Tracker) {
        let (contents, tracker) = self.0.get_heap_size_with_tracker(tracker);
        (std::mem::size_of_val(self.0) + contents, tracker)
    }
}
