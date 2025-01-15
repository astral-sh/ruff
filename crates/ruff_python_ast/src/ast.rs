#![allow(clippy::derive_partial_eq_without_eq)]

use std::ops::{Deref, Index};

use crate as ast;

#[derive(Clone, Default, PartialEq)]
pub struct Ast {
    pub(crate) storage: ast::Storage,
}

impl Ast {
    #[inline]
    pub fn wrap<T>(&self, node: T) -> Node<T> {
        Node { ast: self, node }
    }

    #[inline]
    pub fn node<'a, I>(&'a self, id: I) -> <I as AstId>::Output<'a>
    where
        I: AstId,
    {
        id.node(self)
    }
}

impl std::fmt::Debug for Ast {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Ast").finish()
    }
}

pub trait AstId {
    type Output<'a>;
    fn node<'a>(self, ast: &'a Ast) -> Self::Output<'a>;
}

pub trait AstIdMut {
    type Output<'a>;
    fn node_mut<'a>(self, ast: &'a mut Ast) -> Self::Output<'a>;
}

#[derive(Clone, Copy)]
pub struct Node<'ast, T> {
    pub ast: &'ast Ast,
    pub node: T,
}

impl<T> Node<'_, T> {
    pub fn as_ref(&self) -> &T {
        &self.node
    }
}

impl<T> std::fmt::Debug for Node<'_, T>
where
    T: std::fmt::Debug,
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("Node").field(&self.node).finish()
    }
}

impl<T> Deref for Node<'_, T> {
    type Target = T;
    fn deref(&self) -> &Self::Target {
        &self.node
    }
}

impl<T> Eq for Node<'_, T> where T: Eq {}

impl<T> std::hash::Hash for Node<'_, T>
where
    T: std::hash::Hash,
{
    fn hash<H>(&self, state: &mut H)
    where
        H: std::hash::Hasher,
    {
        self.node.hash(state);
    }
}

impl<T> PartialEq for Node<'_, T>
where
    T: PartialEq,
{
    fn eq(&self, other: &Self) -> bool {
        self.node == other.node
    }
}
