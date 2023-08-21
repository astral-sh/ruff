use std::ops::Index;

use ruff_index::{newtype_index, IndexVec};
use ruff_python_ast::Stmt;

use crate::branches::BranchId;

/// Id uniquely identifying a statement AST node.
///
/// Using a `u32` is sufficient because Ruff only supports parsing documents with a size of max
/// `u32::max` and it is impossible to have more nodes than characters in the file. We use a
/// `NonZeroU32` to take advantage of memory layout optimizations.
#[newtype_index]
#[derive(Ord, PartialOrd)]
pub struct StatementId;

/// A [`Stmt`] AST node, along with a pointer to its parent statement (if any).
#[derive(Debug)]
struct StatementWithParent<'a> {
    /// A pointer to the AST node.
    statement: &'a Stmt,
    /// The ID of the parent of this node, if any.
    parent: Option<StatementId>,
    /// The branch ID of this node, if any.
    branch: Option<BranchId>,
}

/// The statements of a program indexed by [`StatementId`]
#[derive(Debug, Default)]
pub struct Statements<'a>(IndexVec<StatementId, StatementWithParent<'a>>);

impl<'a> Statements<'a> {
    /// Inserts a new statement into the statement vector and returns its unique ID.
    pub(crate) fn insert(
        &mut self,
        statement: &'a Stmt,
        parent: Option<StatementId>,
        branch: Option<BranchId>,
    ) -> StatementId {
        self.0.push(StatementWithParent {
            statement,
            parent,
            branch,
        })
    }

    /// Return the [`StatementId`] of the parent statement.
    #[inline]
    pub(crate) fn parent_id(&self, statement_id: StatementId) -> Option<StatementId> {
        self.0[statement_id].parent
    }

    /// Return the [`StatementId`] of the parent statement.
    #[inline]
    pub(crate) fn branch_id(&self, statement_id: StatementId) -> Option<BranchId> {
        self.0[statement_id].branch
    }

    /// Returns an iterator over all [`StatementId`] ancestors, starting from the given [`StatementId`].
    pub(crate) fn ancestor_ids(&self, id: StatementId) -> impl Iterator<Item = StatementId> + '_ {
        std::iter::successors(Some(id), |&id| self.0[id].parent)
    }
}

impl<'a> Index<StatementId> for Statements<'a> {
    type Output = &'a Stmt;

    #[inline]
    fn index(&self, index: StatementId) -> &Self::Output {
        &self.0[index].statement
    }
}
