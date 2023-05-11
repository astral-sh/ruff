use std::ops::Deref;

use rustpython_parser::ast::{Expr, Stmt};

#[derive(Clone)]
pub enum Node<'a> {
    Stmt(&'a Stmt),
    Expr(&'a Expr),
}

#[derive(Debug)]
pub struct RefEquality<'a, T>(pub &'a T);

impl<'a, T> RefEquality<'a, T> {
    // More specific implementation that keeps the `'a` lifetime.
    // It's otherwise the same as [`AsRef::as_ref`]
    #[allow(clippy::should_implement_trait)]
    pub fn as_ref(&self) -> &'a T {
        self.0
    }
}

impl<'a, T> AsRef<T> for RefEquality<'a, T> {
    fn as_ref(&self) -> &T {
        self.0
    }
}

impl<'a, T> Clone for RefEquality<'a, T> {
    fn clone(&self) -> Self {
        Self(self.0)
    }
}

impl<'a, T> Copy for RefEquality<'a, T> {}

impl<'a, T> std::hash::Hash for RefEquality<'a, T> {
    fn hash<H>(&self, state: &mut H)
    where
        H: std::hash::Hasher,
    {
        (self.0 as *const T).hash(state);
    }
}

impl<'a, 'b, T> PartialEq<RefEquality<'b, T>> for RefEquality<'a, T> {
    fn eq(&self, other: &RefEquality<'b, T>) -> bool {
        std::ptr::eq(self.0, other.0)
    }
}

impl<'a, T> Eq for RefEquality<'a, T> {}

impl<'a, T> Deref for RefEquality<'a, T> {
    type Target = T;

    fn deref(&self) -> &T {
        self.0
    }
}

impl<'a> From<&RefEquality<'a, Stmt>> for &'a Stmt {
    fn from(r: &RefEquality<'a, Stmt>) -> Self {
        r.0
    }
}

impl<'a> From<&RefEquality<'a, Expr>> for &'a Expr {
    fn from(r: &RefEquality<'a, Expr>) -> Self {
        r.0
    }
}

impl<'a> From<RefEquality<'a, Stmt>> for &'a Stmt {
    fn from(r: RefEquality<'a, Stmt>) -> Self {
        r.0
    }
}

impl<'a> From<RefEquality<'a, Expr>> for &'a Expr {
    fn from(r: RefEquality<'a, Expr>) -> Self {
        r.0
    }
}
