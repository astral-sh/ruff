use ruff_python_ast::HasNodeIndex;

/// Compact key for a node for use in a hash map.
#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)]
pub(super) struct NodeKey(usize);

impl NodeKey {
    pub(super) fn from_node<N>(node: N) -> Self
    where
        N: HasNodeIndex,
    {
        NodeKey(node.node_index().as_usize())
    }
}
