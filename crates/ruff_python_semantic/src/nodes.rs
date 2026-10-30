use std::ptr;

use ruff_index::{IndexVec, newtype_index};
use ruff_python_ast::{Expr, Stmt, flat};
use ruff_text_size::{Ranged, TextRange};

use crate::BranchId;

/// Id uniquely identifying an AST node in a program.
///
/// Using a `u32` is sufficient because Ruff only supports parsing documents with a size of max
/// `u32::max` and it is impossible to have more nodes than characters in the file. We use a
/// `NonZeroU32` to take advantage of memory layout optimizations.
#[newtype_index]
#[derive(Ord, PartialOrd)]
pub struct NodeId;

#[newtype_index]
struct AnnotationNodeId;

#[derive(Debug, Copy, Clone)]
enum SyntaxNode {
    Stmt(flat::Handle<Stmt>),
    Expr(flat::Handle<Expr>),
    Annotation(AnnotationNodeId),
}

/// An AST node in a program, along with a pointer to its parent node (if any).
#[derive(Debug)]
struct NodeWithParent {
    node: SyntaxNode,
    /// The ID of the parent of this node, if any.
    parent: Option<NodeId>,
    /// The branch ID of this node, if any.
    branch: Option<BranchId>,
}

/// The nodes of a program indexed by [`NodeId`]
#[derive(Debug)]
pub struct Nodes<'a> {
    module: &'a flat::Ast,
    entries: IndexVec<NodeId, NodeWithParent>,
    /// String annotations are parsed into separate trees with their own row indices.
    annotations: IndexVec<AnnotationNodeId, NodeRef<'a>>,
}

impl<'a> Nodes<'a> {
    pub(crate) fn new(module: &'a flat::Ast) -> Self {
        Self {
            module,
            entries: IndexVec::new(),
            annotations: IndexVec::new(),
        }
    }

    /// Inserts a new AST node into the tree and returns its unique ID.
    #[inline]
    pub(crate) fn insert(
        &mut self,
        node: NodeRef<'a>,
        parent: Option<NodeId>,
        branch: Option<BranchId>,
    ) -> NodeId {
        let node = match node {
            NodeRef::Stmt(stmt) if ptr::eq(stmt.ast(), self.module) => {
                SyntaxNode::Stmt(stmt.handle())
            }
            NodeRef::Expr(expr) if ptr::eq(expr.ast(), self.module) => {
                SyntaxNode::Expr(expr.handle())
            }
            _ => SyntaxNode::Annotation(self.annotations.push(node)),
        };
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
    pub(crate) fn node(&self, id: NodeId) -> NodeRef<'a> {
        match self.entries[id].node {
            SyntaxNode::Stmt(stmt) => NodeRef::Stmt(
                self.module
                    .view(stmt)
                    .expect("statement belongs to the module"),
            ),
            SyntaxNode::Expr(expr) => NodeRef::Expr(
                self.module
                    .view(expr)
                    .expect("expression belongs to the module"),
            ),
            SyntaxNode::Annotation(other) => self.annotations[other],
        }
    }

    #[inline]
    pub(crate) fn statement(&self, id: NodeId) -> Option<flat::View<'a, Stmt>> {
        match self.entries[id].node {
            SyntaxNode::Stmt(stmt) => self.module.view(stmt),
            SyntaxNode::Expr(_) => None,
            SyntaxNode::Annotation(other) => self.annotations[other].as_statement(),
        }
    }

    #[inline]
    pub(crate) fn expression(&self, id: NodeId) -> Option<flat::View<'a, Expr>> {
        match self.entries[id].node {
            SyntaxNode::Stmt(_) => None,
            SyntaxNode::Expr(expr) => self.module.view(expr),
            SyntaxNode::Annotation(other) => self.annotations[other].as_expression(),
        }
    }

    #[inline]
    pub(crate) fn is_statement(&self, id: NodeId) -> bool {
        match self.entries[id].node {
            SyntaxNode::Stmt(_) => true,
            SyntaxNode::Expr(_) => false,
            SyntaxNode::Annotation(other) => self.annotations[other].is_statement(),
        }
    }
}

/// A statement or expression visited by the semantic model.
#[derive(Copy, Clone, Debug, PartialEq)]
pub enum NodeRef<'a> {
    Stmt(flat::View<'a, Stmt>),
    Expr(flat::View<'a, Expr>),
}

impl<'a> NodeRef<'a> {
    #[inline]
    fn as_statement(self) -> Option<flat::View<'a, Stmt>> {
        match self {
            Self::Stmt(stmt) => Some(stmt),
            Self::Expr(_) => None,
        }
    }

    #[inline]
    fn as_expression(self) -> Option<flat::View<'a, Expr>> {
        match self {
            Self::Expr(expr) => Some(expr),
            Self::Stmt(_) => None,
        }
    }

    fn is_statement(self) -> bool {
        matches!(self, Self::Stmt(_))
    }
}

impl Ranged for NodeRef<'_> {
    fn range(&self) -> TextRange {
        match self {
            Self::Stmt(stmt) => stmt.range(),
            Self::Expr(expr) => expr.range(),
        }
    }
}

impl<'a> From<flat::View<'a, Expr>> for NodeRef<'a> {
    fn from(expr: flat::View<'a, Expr>) -> Self {
        Self::Expr(expr)
    }
}

impl<'a> From<flat::View<'a, Stmt>> for NodeRef<'a> {
    fn from(stmt: flat::View<'a, Stmt>) -> Self {
        Self::Stmt(stmt)
    }
}

#[cfg(test)]
mod tests {
    use ruff_python_parser::{parse_expression, parse_module};

    use super::Nodes;

    #[test]
    fn expression_ids_resolve_in_their_original_tree() {
        let module = parse_module("module").unwrap();
        let first = parse_expression("first").unwrap();
        let second = parse_expression("second").unwrap();
        let mut nodes = Nodes::new(module.ast());

        let stmt = module.suite().first().unwrap();
        let expr = stmt.as_expr_stmt().unwrap().value();
        let parent = nodes.insert(stmt.into(), None, None);
        assert!(nodes.is_statement(parent));
        assert!(nodes.statement(parent).unwrap().is_same_node(stmt));
        assert!(nodes.expression(parent).is_none());

        assert_eq!(expr.handle(), first.expr().handle());
        assert_eq!(expr.handle(), second.expr().handle());
        let records = [expr, first.expr(), second.expr(), expr]
            .map(|expr| (nodes.insert(expr.into(), Some(parent), None), expr));

        for (id, original) in records {
            assert_eq!(nodes.parent_id(id), Some(parent));
            assert!(!nodes.is_statement(id));
            assert!(nodes.statement(id).is_none());
            assert!(nodes.expression(id).unwrap().is_same_node(original));
            assert!(
                nodes
                    .node(id)
                    .as_expression()
                    .unwrap()
                    .is_same_node(original)
            );
        }
    }
}
