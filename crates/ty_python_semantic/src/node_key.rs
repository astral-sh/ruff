use ruff_python_ast::{HasNodeIndex, NodeIndex};

/// Compact key for a node for use in a hash map.
#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash, get_size2::GetSize)]
pub(super) struct NodeKey(NodeIndex);

impl NodeKey {
    pub(super) fn from_node<N>(node: N) -> Self
    where
        N: HasNodeIndex,
    {
        NodeKey(node.node_index().load())
    }
}
