use ruff_python_ast::{self as ast, HasNodeIndex, NodeIndex};

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

#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug, salsa::Update, get_size2::GetSize)]
pub(crate) struct ExpressionNodeKey(NodeKey);

// TODO: Delete after merging https://github.com/astral-sh/ruff/pull/19025
impl From<&ast::Identifier> for ExpressionNodeKey {
    fn from(value: &ast::Identifier) -> Self {
        Self(NodeKey::from_node(value))
    }
}

impl From<ast::ExprRef<'_>> for ExpressionNodeKey {
    fn from(value: ast::ExprRef<'_>) -> Self {
        Self(NodeKey::from_node(value))
    }
}

impl From<&ast::Expr> for ExpressionNodeKey {
    fn from(value: &ast::Expr) -> Self {
        Self(NodeKey::from_node(value))
    }
}

impl From<&ast::ExprCall> for ExpressionNodeKey {
    fn from(value: &ast::ExprCall) -> Self {
        Self(NodeKey::from_node(value))
    }
}
