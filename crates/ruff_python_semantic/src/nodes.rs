use ruff_index::{IndexVec, newtype_index};
use ruff_python_ast::{AstNode, Expr, Stmt, SyntaxNode};

use crate::BranchId;

/// Id uniquely identifying an AST node in a program.
///
/// Using a `u32` is sufficient because Ruff only supports parsing documents with a size of max
/// `u32::max` and it is impossible to have more nodes than characters in the file. We use a
/// `NonZeroU32` to take advantage of memory layout optimizations.
#[newtype_index]
#[derive(Ord, PartialOrd)]
pub struct NodeId;

/// An AST node in a program, along with a pointer to its parent node (if any).
#[derive(Debug)]
struct NodeWithParent<'a> {
    // Quoted annotations can belong to a different AST from the enclosing module.
    node: SyntaxNode<'a>,
    /// The ID of the parent of this node, if any.
    parent: Option<NodeId>,
    /// The branch ID of this node, if any.
    branch: Option<BranchId>,
}

/// The nodes of a program indexed by [`NodeId`]
#[derive(Debug, Default)]
pub struct Nodes<'a> {
    entries: IndexVec<NodeId, NodeWithParent<'a>>,
}

impl<'a> Nodes<'a> {
    /// Inserts a new AST node into the tree and returns its unique ID.
    #[inline]
    pub(crate) fn insert(
        &mut self,
        node: SyntaxNode<'a>,
        parent: Option<NodeId>,
        branch: Option<BranchId>,
    ) -> NodeId {
        self.entries.push(NodeWithParent {
            node,
            parent,
            branch,
        })
    }

    /// Return the [`NodeId`] of the parent node.
    #[inline]
    pub(crate) fn parent_id(&self, node_id: NodeId) -> Option<NodeId> {
        self.entries[node_id].parent
    }

    /// Return the [`BranchId`] of the branch node.
    #[inline]
    pub(crate) fn branch_id(&self, node_id: NodeId) -> Option<BranchId> {
        self.entries[node_id].branch
    }

    /// Returns an iterator over all [`NodeId`] ancestors, starting from the given [`NodeId`].
    pub(crate) fn ancestor_ids(&self, node_id: NodeId) -> impl Iterator<Item = NodeId> + '_ {
        std::iter::successors(Some(node_id), |&node_id| self.entries[node_id].parent)
    }

    #[inline]
    pub(crate) fn node(&self, id: NodeId) -> SyntaxNode<'a> {
        self.entries[id].node
    }

    #[inline]
    pub(crate) fn statement(&self, id: NodeId) -> Option<Stmt<'a>> {
        self.entries[id].node.cast::<Stmt>()
    }

    #[inline]
    pub(crate) fn expression(&self, id: NodeId) -> Option<Expr<'a>> {
        self.entries[id].node.cast::<Expr>()
    }

    #[inline]
    pub(crate) fn is_statement(&self, id: NodeId) -> bool {
        self.entries[id].node.is_statement()
    }
}

#[cfg(test)]
mod tests {
    use ruff_python_ast::AstNode as _;
    use ruff_python_parser::{parse_expression, parse_module};

    use super::Nodes;

    #[test]
    fn borrowed_nodes_keep_their_original_tree() {
        let module = parse_module("module").unwrap();
        let first = parse_expression("first").unwrap();
        let second = parse_expression("second").unwrap();
        let mut nodes = Nodes::default();

        let stmt = module.suite().first().unwrap();
        let expr = stmt.as_expr_stmt().unwrap().value();
        let parent = nodes.insert(stmt.syntax_node(), None, None);
        assert!(nodes.is_statement(parent));
        assert!(nodes.statement(parent).unwrap().is_same_node(stmt));
        assert!(nodes.expression(parent).is_none());

        assert_eq!(expr.node_id(), first.expr().node_id());
        assert_eq!(expr.node_id(), second.expr().node_id());
        let records = [expr, first.expr(), second.expr(), expr]
            .map(|expr| (nodes.insert(expr.syntax_node(), Some(parent), None), expr));

        for (id, original) in records {
            assert_eq!(nodes.parent_id(id), Some(parent));
            assert!(!nodes.is_statement(id));
            assert!(nodes.statement(id).is_none());
            assert!(nodes.expression(id).unwrap().is_same_node(original));
            assert_eq!(nodes.node(id), original.syntax_node());
        }
    }
}
