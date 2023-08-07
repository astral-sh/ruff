use std::ops::Index;

use rustc_hash::FxHashMap;

use ruff_index::{newtype_index, IndexVec};
use ruff_python_ast::{Ranged, Stmt};
use ruff_text_size::TextSize;

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
    /// The depth of this node in the tree.
    depth: u32,
}

/// The statements of a program indexed by [`StatementId`]
#[derive(Debug, Default)]
pub struct Statements<'a> {
    statements: IndexVec<StatementId, StatementWithParent<'a>>,
    statement_to_id: FxHashMap<StatementKey, StatementId>,
}

/// A unique key for a statement AST node. No two statements can appear at the same location
/// in the source code, since compound statements must be delimited by _at least_ one character
/// (a colon), so the starting offset is a cheap and sufficient unique identifier.
#[derive(Debug, Copy, Clone, Hash, PartialEq, Eq)]
pub struct StatementKey(TextSize);

impl From<&Stmt> for StatementKey {
    fn from(statement: &Stmt) -> Self {
        Self(statement.start())
    }
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
        if let Some(existing_id) = self
            .statement_to_id
            .insert(StatementKey::from(statement), next_id)
        {
            panic!("Statements already exists with ID: {existing_id:?}");
        }
        self.statements.push(StatementWithParent {
            statement,
            parent,
            depth: parent.map_or(0, |parent| self.statements[parent].depth + 1),
        })
    }

    /// Returns the [`StatementId`] of the given statement.
    #[inline]
    pub fn statement_id(&self, statement: &'a Stmt) -> Option<StatementId> {
        self.statement_to_id
            .get(&StatementKey::from(statement))
            .copied()
    }

    /// Return the [`StatementId`] of the parent statement.
    #[inline]
    pub fn parent_id(&self, statement_id: StatementId) -> Option<StatementId> {
        self.statements[statement_id].parent
    }

    /// Return the depth of the statement.
    #[inline]
    pub(crate) fn depth(&self, id: StatementId) -> u32 {
        self.statements[id].depth
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
