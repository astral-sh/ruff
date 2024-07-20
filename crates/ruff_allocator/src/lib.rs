use std::fmt::{Display, Formatter};
use std::iter::FusedIterator;
use std::ops::{Deref, DerefMut};
use std::{borrow, fmt};

pub type Allocator = bumpalo::Bump;

pub type String<'allocator> = bumpalo::collections::String<'allocator>;
pub type Vec<'allocator, T> = bumpalo::collections::Vec<'allocator, T>;

#[derive(PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Box<'allocator, T: ?Sized>(bumpalo::boxed::Box<'allocator, T>);

impl<'allocator, T> Box<'allocator, T> {
    pub fn new_in(value: T, arena: &'allocator Allocator) -> Self {
        Self(bumpalo::boxed::Box::new_in(value, arena))
    }

    pub fn into_inner(self) -> T {
        bumpalo::boxed::Box::into_inner(self.0)
    }
}

impl<'allocator, T> CloneIn<'allocator> for Box<'allocator, T>
where
    T: CloneIn<'allocator>,
{
    fn clone_in(&self, allocator: &'allocator Allocator) -> Self {
        Self(bumpalo::boxed::Box::new_in(
            self.0.as_ref().clone_in(allocator),
            allocator,
        ))
    }
}

impl<T> AsRef<T> for Box<'_, T> {
    fn as_ref(&self) -> &T {
        self.0.as_ref()
    }
}

impl<T> std::fmt::Display for Box<'_, T>
where
    T: Display,
{
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

impl<'a, T: fmt::Debug + ?Sized> fmt::Debug for Box<'a, T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(&self.0, f)
    }
}

impl<'a, T: ?Sized> borrow::Borrow<T> for Box<'a, T> {
    fn borrow(&self) -> &T {
        &self.0
    }
}

impl<'a, T: ?Sized> borrow::BorrowMut<T> for Box<'a, T> {
    fn borrow_mut(&mut self) -> &mut T {
        &mut self.0
    }
}

impl<'a, T: ?Sized> AsMut<T> for Box<'a, T> {
    fn as_mut(&mut self) -> &mut T {
        &mut self.0
    }
}

impl<'a, T: ?Sized> Deref for Box<'a, T> {
    type Target = T;

    fn deref(&self) -> &T {
        &self.0
    }
}

impl<'a, T: ?Sized> DerefMut for Box<'a, T> {
    fn deref_mut(&mut self) -> &mut T {
        &mut self.0
    }
}

impl<'a, I: Iterator + ?Sized> Iterator for Box<'a, I> {
    type Item = I::Item;
    fn next(&mut self) -> Option<I::Item> {
        self.0.next()
    }
    fn size_hint(&self) -> (usize, Option<usize>) {
        self.0.size_hint()
    }
    fn nth(&mut self, n: usize) -> Option<I::Item> {
        self.0.nth(n)
    }
    fn last(self) -> Option<I::Item> {
        #[inline]
        fn some<T>(_: Option<T>, x: T) -> Option<T> {
            Some(x)
        }
        self.fold(None, some)
    }
}

impl<'a, I: DoubleEndedIterator + ?Sized> DoubleEndedIterator for Box<'a, I> {
    fn next_back(&mut self) -> Option<I::Item> {
        self.0.next_back()
    }
    fn nth_back(&mut self, n: usize) -> Option<I::Item> {
        self.0.nth_back(n)
    }
}
impl<'a, I: ExactSizeIterator + ?Sized> ExactSizeIterator for Box<'a, I> {
    fn len(&self) -> usize {
        self.0.len()
    }
}

impl<'a, I: FusedIterator + ?Sized> FusedIterator for Box<'a, I> {}

pub trait CloneIn<'allocator> {
    #[must_use]
    fn clone_in(&self, allocator: &'allocator Allocator) -> Self;
}

impl<T> CloneIn<'_> for T
where
    T: Clone,
{
    fn clone_in(&self, _: &Allocator) -> Self {
        self.clone()
    }
}
