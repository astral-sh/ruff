use std::ops::{Index, IndexMut};

use ruff_index::{newtype_index, IndexVec};
use rustc_hash::FxHashMap;
use rustpython_parser::ast::Stmt;

use ruff_python_ast::types::RefEquality;

/// Id uniquely identifying a statement in a program.
///
/// Using a `u32` is sufficient because Ruff only supports parsing documents with a size of max `u32::max`
/// and it is impossible to have more statements than characters in the file. We use a `NonZeroU32` to
/// take advantage of memory layout optimizations.
#[newtype_index]
#[derive(Ord, PartialOrd)]
pub struct NodeId;

/// A [`Node`] represents a statement in a program, along with a pointer to its parent (if any).
#[derive(Debug)]
struct Node<'a> {
    /// The statement this node represents.
    stmt: &'a Stmt,
    /// The ID of the parent of this node, if any.
    parent: Option<NodeId>,
    /// The depth of this node in the tree.
    depth: u32,
}

/// The nodes of a program indexed by [`NodeId`]
#[derive(Debug, Default)]
pub struct Nodes<'a> {
    nodes: IndexVec<NodeId, Node<'a>>,
    node_to_id: FxHashMap<RefEquality<'a, Stmt>, NodeId>,
}

impl<'a> Nodes<'a> {
    /// Inserts a new node into the node tree and returns its unique id.
    ///
    /// Panics if a node with the same pointer already exists.
    pub(crate) fn insert(&mut self, stmt: &'a Stmt, parent: Option<NodeId>) -> NodeId {
        let next_id = self.nodes.next_index();
        if let Some(existing_id) = self.node_to_id.insert(RefEquality(stmt), next_id) {
            panic!("Node already exists with id {existing_id:?}");
        }
        self.nodes.push(Node {
            stmt,
            parent,
            depth: parent.map_or(0, |parent| self.nodes[parent].depth + 1),
        })
    }

    /// Returns the [`NodeId`] of the given node.
    #[inline]
    pub fn node_id(&self, node: &'a Stmt) -> Option<NodeId> {
        self.node_to_id.get(&RefEquality(node)).copied()
    }

    /// Return the [`NodeId`] of the parent node.
    #[inline]
    pub fn parent_id(&self, node_id: NodeId) -> Option<NodeId> {
        self.nodes[node_id].parent
    }

    /// Return the parent of the given node.
    pub fn parent(&self, node: &'a Stmt) -> Option<&'a Stmt> {
        let node_id = self.node_to_id.get(&RefEquality(node))?;
        let parent_id = self.nodes[*node_id].parent?;
        Some(self[parent_id])
    }

    /// Return the depth of the node.
    #[inline]
    pub(crate) fn depth(&self, node_id: NodeId) -> u32 {
        self.nodes[node_id].depth
    }

    /// Returns an iterator over all [`NodeId`] ancestors, starting from the given [`NodeId`].
    pub(crate) fn ancestor_ids(&self, node_id: NodeId) -> impl Iterator<Item = NodeId> + '_ {
        std::iter::successors(Some(node_id), |&node_id| self.nodes[node_id].parent)
    }
}

impl<'a> Index<NodeId> for Nodes<'a> {
    type Output = &'a Stmt;

    #[inline]
    fn index(&self, index: NodeId) -> &Self::Output {
        &self.nodes[index].stmt
    }
}

impl<'a> IndexMut<NodeId> for Nodes<'a> {
    #[inline]
    fn index_mut(&mut self, index: NodeId) -> &mut Self::Output {
        &mut self.nodes[index].stmt
    }
}
