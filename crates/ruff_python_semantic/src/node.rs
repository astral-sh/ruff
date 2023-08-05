use std::ops::{Index, IndexMut};

use ruff_index::{newtype_index, IndexVec};
use rustc_hash::FxHashMap;

use ruff_python_ast::types::RefEquality;

/// Id uniquely identifying an AST node in a program.
///
/// Using a `u32` is sufficient because Ruff only supports parsing documents with a size of max `u32::max`
/// and it is impossible to have more nodes than characters in the file. We use a `NonZeroU32` to
/// take advantage of memory layout optimizations.
#[newtype_index]
#[derive(Ord, PartialOrd)]
pub struct NodeId;

/// A [`Node`] represents an AST node in a program, along with a pointer to its parent (if any).
#[derive(Debug)]
struct Node<'a, T> {
    /// A pointer to the AST node.
    node: &'a T,
    /// The ID of the parent of this node, if any.
    parent: Option<NodeId>,
    /// The depth of this node in the tree.
    depth: u32,
}

/// The nodes of a program indexed by [`NodeId`]
#[derive(Debug)]
pub struct Nodes<'a, T> {
    nodes: IndexVec<NodeId, Node<'a, T>>,
    node_to_id: FxHashMap<RefEquality<'a, T>, NodeId>,
}

impl<'a, T> Default for Nodes<'a, T> {
    fn default() -> Self {
        Self {
            nodes: IndexVec::default(),
            node_to_id: FxHashMap::default(),
        }
    }
}

impl<'a, T> Nodes<'a, T> {
    /// Inserts a new node into the node tree and returns its unique id.
    ///
    /// Panics if a node with the same pointer already exists.
    pub(crate) fn insert(&mut self, node: &'a T, parent: Option<NodeId>) -> NodeId {
        let next_id = self.nodes.next_index();
        if let Some(existing_id) = self.node_to_id.insert(RefEquality(node), next_id) {
            panic!("Node already exists with id {existing_id:?}");
        }
        self.nodes.push(Node {
            node,
            parent,
            depth: parent.map_or(0, |parent| self.nodes[parent].depth + 1),
        })
    }

    /// Returns the [`NodeId`] of the given node.
    #[inline]
    pub fn node_id(&self, node: &'a T) -> Option<NodeId> {
        self.node_to_id.get(&RefEquality(node)).copied()
    }

    /// Return the [`NodeId`] of the parent node.
    #[inline]
    pub fn parent_id(&self, node_id: NodeId) -> Option<NodeId> {
        self.nodes[node_id].parent
    }

    /// Return the parent of the given node.
    pub fn parent(&self, node: &'a T) -> Option<&'a T> {
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

impl<'a, T> Index<NodeId> for Nodes<'a, T> {
    type Output = &'a T;

    #[inline]
    fn index(&self, index: NodeId) -> &Self::Output {
        &self.nodes[index].node
    }
}

impl<'a, T> IndexMut<NodeId> for Nodes<'a, T> {
    #[inline]
    fn index_mut(&mut self, index: NodeId) -> &mut Self::Output {
        &mut self.nodes[index].node
    }
}
