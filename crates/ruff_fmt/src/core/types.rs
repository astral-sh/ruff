use std::ops::Deref;

use rustpython_ast::Location;

use crate::cst::{Expr, Located, Stmt};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord)]
pub struct Range {
    pub location: Location,
    pub end_location: Location,
}

impl Range {
    pub fn new(location: Location, end_location: Location) -> Self {
        Self {
            location,
            end_location,
        }
    }

    pub fn from_located<T>(located: &Located<T>) -> Self {
        Range::new(located.location, located.end_location.unwrap())
    }
}

#[derive(Debug, Copy, Clone)]
pub struct RefEquality<'a, T>(pub &'a T);

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

impl<'a> From<&RefEquality<'a, rustpython_ast::Stmt>> for &'a rustpython_ast::Stmt {
    fn from(r: &RefEquality<'a, rustpython_ast::Stmt>) -> Self {
        r.0
    }
}

impl<'a> From<&RefEquality<'a, rustpython_ast::Expr>> for &'a rustpython_ast::Expr {
    fn from(r: &RefEquality<'a, rustpython_ast::Expr>) -> Self {
        r.0
    }
}
