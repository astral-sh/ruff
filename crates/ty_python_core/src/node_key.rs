use ruff_python_ast::{AstNode, HasNodeIndex, NodeIndex};

use crate::ast_node_ref::AstNodeRef;

/// Compact key for a node for use in a hash map.
#[derive(Copy, Clone, Debug, Eq, PartialEq, Ord, PartialOrd, Hash, get_size2::GetSize)]
pub struct NodeKey(NodeIndex);

impl NodeKey {
    /// Returns the index of the AST node.
    pub fn index(self) -> NodeIndex {
        self.0
    }

    pub(crate) fn module_index(self) -> Option<usize> {
        self.index()
            .as_u32()
            .filter(|&index| index <= ruff_python_ast::MAX_REAL_INDEX)
            .map(|index| index as usize)
    }

    pub(crate) fn from_view<'ast>(node: impl AstNode<'ast>) -> Self {
        Self(node.node_index())
    }

    pub fn from_node<N>(node: N) -> Self
    where
        N: HasNodeIndex,
    {
        NodeKey(node.node_index())
    }

    pub(crate) fn from_node_ref<T>(node_ref: &AstNodeRef<T>) -> Self {
        NodeKey(node_ref.index())
    }
}
