//! # Narrowing constraints
//!
//! When building a semantic index for a file, we associate each binding with _narrowing
//! constraints_. The narrowing constraint is used to constrain the type of the binding's symbol.
//! Note that a binding can be associated with a different narrowing constraint at different points
//! in a file. See the [`use_def`][crate::semantic_index::use_def] module for more details.
//!
//! This module defines how narrowing constraints are stored internally.
//!
//! A _narrowing constraint_ consists of a list of _clauses_, each of which corresponds with an
//! expression in the source file (represented by a [`Constraint`]). We need to support the
//! following operations on narrowing constraints:
//!
//! - Adding a new clause to an existing constraint
//! - Merging two constraints together, which produces the _intersection_ of their clauses
//! - Iterating through the clauses in a constraint
//!
//! In particular, note that we do not need random access to the clauses in a constraint. That
//! means that we can use a simple [_sorted association list_][ruff_index::list] as our data
//! structure.
//!
//! Because we visit the contents of each scope in source-file order, and assign scoped IDs in
//! source-file order, that means that we will tend to visit narrowing constraints in order by
//! their IDs. This is exactly how to get the best performance from our alist implementation.
//!
//! [`Constraint`]: crate::semantic_index::constraint::Constraint

use ruff_index::list::{ListBuilder, ListSetReverseIterator, ListStorage};
use ruff_index::newtype_index;

use crate::semantic_index::constraint::ScopedConstraintId;

/// A narrowing constraint associated with a live binding.
///
/// A constraint is a list of clauses, each of which is a [`Constraint`] that constrains the type
/// of the binding's symbol.
///
/// An instance of this type represents a _non-empty_ narrowing constraint. You will often wrap
/// this in `Option` and use `None` to represent an empty narrowing constraint.
///
/// [`Constraint`]: crate::semantic_index::constraint::Constraint
#[newtype_index]
pub(crate) struct ScopedNarrowingConstraintId;

/// One of the clauses in a narrowing constraint, which is a [`Constraint`] that constrains the
/// type of the binding's symbol.
///
/// Note that those [`Constraint`]s are stored in [their own per-scope
/// arena][crate::semantic_index::constraint::Constraints], so internally we use a
/// [`ScopedConstraintId`] to refer to the underlying constraint.
///
/// [`Constraint`]: crate::semantic_index::constraint::Constraint
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub(crate) struct ScopedNarrowingConstraintClause(ScopedConstraintId);

impl ScopedNarrowingConstraintClause {
    /// Returns (the ID of) the `Constraint` for this clause
    pub(crate) fn constraint(self) -> ScopedConstraintId {
        self.0
    }
}

impl From<ScopedConstraintId> for ScopedNarrowingConstraintClause {
    fn from(constraint: ScopedConstraintId) -> ScopedNarrowingConstraintClause {
        ScopedNarrowingConstraintClause(constraint)
    }
}

/// A collection of narrowing constraints for a given scope.
#[derive(Debug, Eq, PartialEq)]
pub(crate) struct NarrowingConstraints {
    lists: ListStorage<ScopedNarrowingConstraintId, ScopedNarrowingConstraintClause>,
}

// Building constraints
// --------------------

/// A builder for creating narrowing constraints.
#[derive(Debug, Default, Eq, PartialEq)]
pub(crate) struct NarrowingConstraintsBuilder {
    lists: ListBuilder<ScopedNarrowingConstraintId, ScopedNarrowingConstraintClause>,
}

impl NarrowingConstraintsBuilder {
    pub(crate) fn build(self) -> NarrowingConstraints {
        NarrowingConstraints {
            lists: self.lists.build(),
        }
    }

    /// Adds a clause to an existing narrowing constraint.
    pub(crate) fn add(
        &mut self,
        constraint: Option<ScopedNarrowingConstraintId>,
        clause: ScopedNarrowingConstraintClause,
    ) -> Option<ScopedNarrowingConstraintId> {
        self.lists.insert(constraint, clause)
    }

    /// Returns the intersection of two narrowing constraints. The result contains the clauses that
    /// appear in both inputs.
    pub(crate) fn intersect(
        &mut self,
        a: Option<ScopedNarrowingConstraintId>,
        b: Option<ScopedNarrowingConstraintId>,
    ) -> Option<ScopedNarrowingConstraintId> {
        self.lists.intersect(a, b)
    }
}

// Iteration
// ---------

pub(crate) type NarrowingConstraintsIterator<'a> = std::iter::Copied<
    ListSetReverseIterator<'a, ScopedNarrowingConstraintId, ScopedNarrowingConstraintClause>,
>;

impl NarrowingConstraints {
    /// Iterates over the clauses in a narrowing constraint.
    pub(crate) fn iter_clauses(
        &self,
        set: Option<ScopedNarrowingConstraintId>,
    ) -> NarrowingConstraintsIterator<'_> {
        self.lists.iter_set_reverse(set).copied()
    }
}

// Test support
// ------------

#[cfg(test)]
mod tests {
    use super::*;

    impl ScopedNarrowingConstraintClause {
        pub(crate) fn as_u32(self) -> u32 {
            self.0.as_u32()
        }
    }

    impl NarrowingConstraintsBuilder {
        pub(crate) fn iter_constraints(
            &self,
            set: Option<ScopedNarrowingConstraintId>,
        ) -> NarrowingConstraintsIterator<'_> {
            self.lists.iter_set_reverse(set).copied()
        }
    }
}
