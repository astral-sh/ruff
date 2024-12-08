use ruff_python_ast::AnyNodeRef;

/// Compact key for a node for use in a hash map.
///
/// Stores the memory address of the node, because using the range and the kind
/// of the node is not enough to uniquely identify them in ASTs resulting from
/// invalid syntax. For example, parsing the input `for` results in a `StmtFor`
/// AST node where both the `target` and the `iter` field are `ExprName` nodes
/// with the same (empty) range `3..3`.
#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)]
pub(super) struct NodeKey(usize);

impl NodeKey {
    pub(super) fn from_node<'a, N>(node: N) -> Self
    where
        N: Into<AnyNodeRef<'a>>,
    {
        let node = node.into();
        NodeKey(node.as_ptr().as_ptr() as usize)
    }
}
