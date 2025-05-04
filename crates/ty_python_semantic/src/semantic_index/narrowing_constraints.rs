//! # Narrowing constraints
//!
//! When building a semantic index for a file, we associate each binding with a _narrowing
//! constraint_, which constrains the type of the binding's symbol. Note that a binding can be
//! associated with a different narrowing constraint at different points in a file. See the
//! [`use_def`][crate::semantic_index::use_def] module for more details.
//!
//! This module defines how narrowing constraints are stored internally.
//!
//! A _narrowing constraint_ consists of a list of _predicates_, each of which corresponds with an
//! expression in the source file (represented by a [`Predicate`]). We need to support the
//! following operations on narrowing constraints:
//!
//! - Adding a new predicate to an existing constraint
//! - Merging two constraints together, which produces the _intersection_ of their predicates
//! - Iterating through the predicates in a constraint
//!
//! In particular, note that we do not need random access to the predicates in a constraint. That
//! means that we can use a simple [_sorted association list_][crate::list] as our data structure.
//! That lets us use a single 32-bit integer to store each narrowing constraint, no matter how many
//! predicates it contains. It also makes merging two narrowing constraints fast, since alists
//! support fast intersection.
//!
//! Because we visit the contents of each scope in source-file order, and assign scoped IDs in
//! source-file order, that means that we will tend to visit narrowing constraints in order by
//! their predicate IDs. This is exactly how to get the best performance from our alist
//! implementation.
//!
//! [`Predicate`]: crate::semantic_index::predicate::Predicate

use crate::list::{List, ListBuilder, ListSetReverseIterator, ListStorage};
use crate::semantic_index::predicate::ScopedPredicateId;

/// A narrowing constraint associated with a live binding.
///
/// A constraint is a list of [`Predicate`]s that each constrain the type of the binding's symbol.
///
/// [`Predicate`]: crate::semantic_index::predicate::Predicate
pub(crate) type ScopedNarrowingConstraint = List<ScopedNarrowingConstraintPredicate>;

/// One of the [`Predicate`]s in a narrowing constraint, which constraints the type of the
/// binding's symbol.
///
/// Note that those [`Predicate`]s are stored in [their own per-scope
/// arena][crate::semantic_index::predicate::Predicates], so internally we use a
/// [`ScopedPredicateId`] to refer to the underlying predicate.
///
/// [`Predicate`]: crate::semantic_index::predicate::Predicate
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub(crate) struct ScopedNarrowingConstraintPredicate(ScopedPredicateId);

impl ScopedNarrowingConstraintPredicate {
    /// Returns (the ID of) the `Predicate`
    pub(crate) fn predicate(self) -> ScopedPredicateId {
        self.0
    }
}

impl From<ScopedPredicateId> for ScopedNarrowingConstraintPredicate {
    fn from(predicate: ScopedPredicateId) -> ScopedNarrowingConstraintPredicate {
        ScopedNarrowingConstraintPredicate(predicate)
    }
}

/// A collection of narrowing constraints for a given scope.
#[derive(Debug, Eq, PartialEq)]
pub(crate) struct NarrowingConstraints {
    lists: ListStorage<ScopedNarrowingConstraintPredicate>,
}

// Building constraints
// --------------------

/// A builder for creating narrowing constraints.
#[derive(Debug, Default, Eq, PartialEq)]
pub(crate) struct NarrowingConstraintsBuilder {
    lists: ListBuilder<ScopedNarrowingConstraintPredicate>,
}

impl NarrowingConstraintsBuilder {
    pub(crate) fn build(self) -> NarrowingConstraints {
        NarrowingConstraints {
            lists: self.lists.build(),
        }
    }

    /// Adds a predicate to an existing narrowing constraint.
    pub(crate) fn add_predicate_to_constraint(
        &mut self,
        constraint: ScopedNarrowingConstraint,
        predicate: ScopedNarrowingConstraintPredicate,
    ) -> ScopedNarrowingConstraint {
        self.lists.insert(constraint, predicate)
    }

    /// Returns the intersection of two narrowing constraints. The result contains the predicates
    /// that appear in both inputs.
    pub(crate) fn intersect_constraints(
        &mut self,
        a: ScopedNarrowingConstraint,
        b: ScopedNarrowingConstraint,
    ) -> ScopedNarrowingConstraint {
        self.lists.intersect(a, b)
    }
}

// Iteration
// ---------

pub(crate) type NarrowingConstraintsIterator<'a> =
    std::iter::Copied<ListSetReverseIterator<'a, ScopedNarrowingConstraintPredicate>>;

impl NarrowingConstraints {
    /// Iterates over the predicates in a narrowing constraint.
    pub(crate) fn iter_predicates(
        &self,
        set: ScopedNarrowingConstraint,
    ) -> NarrowingConstraintsIterator<'_> {
        self.lists.iter_set_reverse(set).copied()
    }
}

// Test support
// ------------

#[cfg(test)]
mod tests {
    use super::*;

    impl ScopedNarrowingConstraintPredicate {
        pub(crate) fn as_u32(self) -> u32 {
            self.0.as_u32()
        }
    }

    impl NarrowingConstraintsBuilder {
        pub(crate) fn iter_predicates(
            &self,
            set: ScopedNarrowingConstraint,
        ) -> NarrowingConstraintsIterator<'_> {
            self.lists.iter_set_reverse(set).copied()
        }
    }
}
