use std::ops::Index;

use ruff_index::{newtype_index, IndexVec};
use ruff_python_ast::{Expr, Stmt};
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

/// An AST node in a program, along with a pointer to its parent node (if any).
#[derive(Debug)]
struct NodeWithParent<'a> {
    /// A pointer to the AST node.
    node: NodeRef<'a>,
    /// The ID of the parent of this node, if any.
    parent: Option<NodeId>,
    /// The branch ID of this node, if any.
    branch: Option<BranchId>,
}

/// The nodes of a program indexed by [`NodeId`]
#[derive(Debug, Default)]
pub struct Nodes<'a> {
    nodes: IndexVec<NodeId, NodeWithParent<'a>>,
}

impl<'a> Nodes<'a> {
    /// Inserts a new AST node into the tree and returns its unique ID.
    pub(crate) fn insert(
        &mut self,
        node: NodeRef<'a>,
        parent: Option<NodeId>,
        branch: Option<BranchId>,
    ) -> NodeId {
        self.nodes.push(NodeWithParent {
            node,
            parent,
            branch,
        })
    }

    /// Return the [`NodeId`] of the parent node.
    #[inline]
    pub fn parent_id(&self, node_id: NodeId) -> Option<NodeId> {
        self.nodes[node_id].parent
    }

    /// Return the [`BranchId`] of the branch node.
    #[inline]
    pub(crate) fn branch_id(&self, node_id: NodeId) -> Option<BranchId> {
        self.nodes[node_id].branch
    }

    /// Returns an iterator over all [`NodeId`] ancestors, starting from the given [`NodeId`].
    pub(crate) fn ancestor_ids(&self, node_id: NodeId) -> impl Iterator<Item = NodeId> + '_ {
        std::iter::successors(Some(node_id), |&node_id| self.nodes[node_id].parent)
    }
}

impl<'a> Index<NodeId> for Nodes<'a> {
    type Output = NodeRef<'a>;

    #[inline]
    fn index(&self, index: NodeId) -> &Self::Output {
        &self.nodes[index].node
    }
}

/// A reference to an AST node. Like [`ruff_python_ast::AnyNodeRef`], but wraps the node
/// itself (like [`Stmt`]) rather than the narrowed type (like [`ruff_python_ast::StmtAssign`]).
///
/// TODO(charlie): Replace with [`ruff_python_ast::AnyNodeRef`]. This requires migrating
/// the rest of the codebase to use [`ruff_python_ast::AnyNodeRef`] and related abstractions,
/// like [`ruff_python_ast::ExprRef`] instead of [`Expr`].
#[derive(Copy, Clone, Debug, PartialEq)]
pub enum NodeRef<'a> {
    Stmt(&'a Stmt),
    Expr(&'a Expr),
}

impl<'a> NodeRef<'a> {
    /// Returns the [`Stmt`] if this is a statement, or `None` if the reference is to another
    /// kind of AST node.
    pub fn as_statement(&self) -> Option<&'a Stmt> {
        match self {
            NodeRef::Stmt(stmt) => Some(stmt),
            NodeRef::Expr(_) => None,
        }
    }

    /// Returns the [`Expr`] if this is a expression, or `None` if the reference is to another
    /// kind of AST node.
    pub fn as_expression(&self) -> Option<&'a Expr> {
        match self {
            NodeRef::Stmt(_) => None,
            NodeRef::Expr(expr) => Some(expr),
        }
    }

    pub fn is_statement(&self) -> bool {
        self.as_statement().is_some()
    }

    pub fn is_expression(&self) -> bool {
        self.as_expression().is_some()
    }
}

impl Ranged for NodeRef<'_> {
    fn range(&self) -> TextRange {
        match self {
            NodeRef::Stmt(stmt) => stmt.range(),
            NodeRef::Expr(expr) => expr.range(),
        }
    }
}

impl<'a> From<&'a Expr> for NodeRef<'a> {
    fn from(expr: &'a Expr) -> Self {
        NodeRef::Expr(expr)
    }
}

impl<'a> From<&'a Stmt> for NodeRef<'a> {
    fn from(stmt: &'a Stmt) -> Self {
        NodeRef::Stmt(stmt)
    }
}
