use std::ops::Index;

use ruff_index::{newtype_index, IndexVec};

/// ID uniquely identifying a branch in a program.
///
/// For example, given:
/// ```python
/// if x > 0:
///     pass
/// elif x > 1:
///     pass
/// else:
///     pass
/// ```
///
/// Each of the three arms of the `if`-`elif`-`else` would be considered a branch, and would be
/// assigned their own unique [`BranchId`].
#[newtype_index]
#[derive(Ord, PartialOrd)]
pub struct BranchId;

/// The branches of a program indexed by [`BranchId`]
#[derive(Debug, Default)]
pub(crate) struct Branches(IndexVec<BranchId, Option<BranchId>>);

impl Branches {
    /// Inserts a new branch into the vector and returns its unique [`BranchID`].
    pub(crate) fn insert(&mut self, parent: Option<BranchId>) -> BranchId {
        self.0.push(parent)
    }

    /// Return the [`BranchId`] of the parent branch.
    #[inline]
    pub(crate) fn parent_id(&self, node_id: BranchId) -> Option<BranchId> {
        self.0[node_id]
    }

    /// Returns an iterator over all [`BranchId`] ancestors, starting from the given [`BranchId`].
    pub(crate) fn ancestor_ids(&self, node_id: BranchId) -> impl Iterator<Item = BranchId> + '_ {
        std::iter::successors(Some(node_id), |&node_id| self.0[node_id])
    }
}

impl Index<BranchId> for Branches {
    type Output = Option<BranchId>;

    #[inline]
    fn index(&self, index: BranchId) -> &Self::Output {
        &self.0[index]
    }
}
