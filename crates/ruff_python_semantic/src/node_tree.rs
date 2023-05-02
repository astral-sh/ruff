use rustpython_parser::ast::Stmt;
use std::num::TryFromIntError;
use std::ops::{Deref, Index, IndexMut};

/// Id uniquely identifying a statement in a program.
///
/// Using a `u32` is sufficient because Ruff only supports parsing documents with a size of max `u32::max`
/// and it is impossible to have more scopes than characters in the file (because defining a function or class
/// requires more than one character).
#[derive(Debug, Copy, Clone, Eq, PartialEq, Hash, Ord, PartialOrd)]
pub struct NodeId(u32);

impl TryFrom<usize> for NodeId {
    type Error = TryFromIntError;

    fn try_from(value: usize) -> Result<Self, Self::Error> {
        Ok(Self(u32::try_from(value)?))
    }
}

impl From<NodeId> for usize {
    fn from(value: NodeId) -> Self {
        value.0 as usize
    }
}

/// The nodes of a program indexed by [`NodeId`]
#[derive(Debug, Default)]
pub struct Nodes<'a>(Vec<&'a Stmt>, Vec<Option<NodeId>>);

impl<'a> Nodes<'a> {
    /// Pushes a new scope and returns its unique id
    pub fn push_node(&mut self, node: &'a Stmt, parent: Option<NodeId>) -> NodeId {
        let next_id = NodeId::try_from(self.0.len()).unwrap();
        self.0.push(node);
        self.1.push(parent);
        next_id
    }

    /// Returns an iterator over all [`NodeId`] ancestors, starting from the given [`NodeId`].
    pub fn ancestor_ids(&self, node_id: NodeId) -> impl Iterator<Item = NodeId> + '_ {
        std::iter::successors(Some(node_id), |&node_id| self.1[usize::from(node_id)])
    }
}

impl<'a> Index<NodeId> for Nodes<'a> {
    type Output = &'a Stmt;

    fn index(&self, index: NodeId) -> &Self::Output {
        &self.0[usize::from(index)]
    }
}

impl<'a> IndexMut<NodeId> for Nodes<'a> {
    fn index_mut(&mut self, index: NodeId) -> &mut Self::Output {
        &mut self.0[usize::from(index)]
    }
}

impl<'a> Deref for Nodes<'a> {
    type Target = [&'a Stmt];
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}
