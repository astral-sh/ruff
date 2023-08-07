use std::ops::Index;

use rustc_hash::FxHashMap;

use ruff_index::{newtype_index, IndexVec};
use ruff_python_ast::types::RefEquality;
use ruff_python_ast::Stmt;

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
}

/// The statements of a program indexed by [`StatementId`]
#[derive(Debug, Default)]
pub struct Statements<'a> {
    statements: IndexVec<StatementId, StatementWithParent<'a>>,
    statement_to_id: FxHashMap<RefEquality<'a, Stmt>, StatementId>,
}

impl<'a> Statements<'a> {
    /// Inserts a new statement into the statement vector and returns its unique ID.
    ///
    /// Panics if a statement with the same pointer already exists.
    pub(crate) fn insert(
        &mut self,
        statement: &'a Stmt,
        parent: Option<StatementId>,
    ) -> StatementId {
        let next_id = self.statements.next_index();
        if let Some(existing_id) = self.statement_to_id.insert(RefEquality(statement), next_id) {
            panic!("Statements already exists with ID: {existing_id:?}");
        }
        self.statements
            .push(StatementWithParent { statement, parent })
    }

    /// Returns the [`StatementId`] of the given statement.
    #[inline]
    pub fn statement_id(&self, statement: &'a Stmt) -> Option<StatementId> {
        self.statement_to_id.get(&RefEquality(statement)).copied()
    }

    /// Return the [`StatementId`] of the parent statement.
    #[inline]
    pub fn parent_id(&self, statement_id: StatementId) -> Option<StatementId> {
        self.statements[statement_id].parent
    }

    /// Returns an iterator over all [`StatementId`] ancestors, starting from the given [`StatementId`].
    pub(crate) fn ancestor_ids(&self, id: StatementId) -> impl Iterator<Item = StatementId> + '_ {
        std::iter::successors(Some(id), |&id| self.statements[id].parent)
    }
}

impl<'a> Index<StatementId> for Statements<'a> {
    type Output = &'a Stmt;

    #[inline]
    fn index(&self, index: StatementId) -> &Self::Output {
        &self.statements[index].statement
    }
}
