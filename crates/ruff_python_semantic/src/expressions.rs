use std::ops::{Index, IndexMut};

use ruff_index::{newtype_index, IndexVec};
use ruff_python_ast::Expr;

/// Id uniquely identifying an [`Expression`] in a program.
///
/// Using a `u32` is sufficient because Ruff only supports parsing documents with a size of max
/// `u32::max` and it is impossible to have more nodes than characters in the file. We use a
/// `NonZeroU32` to take advantage of memory layout optimizations.
#[newtype_index]
#[derive(Ord, PartialOrd)]
pub struct ExpressionId;

/// A [`Expression`] represents an [`Expr`] AST node in a program, along with a pointer to its
/// parent expression (if any).
#[derive(Debug)]
struct Expression<'a> {
    /// A pointer to the AST node.
    node: &'a Expr,
    /// The ID of the parent of this node, if any.
    parent: Option<ExpressionId>,
}

/// The nodes of a program indexed by [`ExpressionId`]
#[derive(Debug, Default)]
pub struct Expressions<'a> {
    nodes: IndexVec<ExpressionId, Expression<'a>>,
}

impl<'a> Expressions<'a> {
    /// Inserts a new expression into the node tree and returns its unique id.
    pub(crate) fn insert(&mut self, node: &'a Expr, parent: Option<ExpressionId>) -> ExpressionId {
        self.nodes.push(Expression { node, parent })
    }

    /// Return the [`ExpressionId`] of the parent node.
    #[inline]
    pub fn parent_id(&self, node_id: ExpressionId) -> Option<ExpressionId> {
        self.nodes[node_id].parent
    }

    /// Returns an iterator over all [`ExpressionId`] ancestors, starting from the given [`ExpressionId`].
    pub(crate) fn ancestor_ids(
        &self,
        node_id: ExpressionId,
    ) -> impl Iterator<Item = ExpressionId> + '_ {
        std::iter::successors(Some(node_id), |&node_id| self.nodes[node_id].parent)
    }
}

impl<'a> Index<ExpressionId> for Expressions<'a> {
    type Output = &'a Expr;

    #[inline]
    fn index(&self, index: ExpressionId) -> &Self::Output {
        &self.nodes[index].node
    }
}

impl<'a> IndexMut<ExpressionId> for Expressions<'a> {
    #[inline]
    fn index_mut(&mut self, index: ExpressionId) -> &mut Self::Output {
        &mut self.nodes[index].node
    }
}
