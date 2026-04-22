//! # Narrowing constraints
//!
//! When building a semantic index for a file, we associate each binding with a _narrowing
//! constraint_, which constrains the type of the binding's place. Note that a binding can be
//! associated with a different narrowing constraint at different points in a file. See the
//! `use_def` module for more details.
//!
//! Narrowing constraints are represented as TDD (ternary decision diagram) nodes, sharing the
//! same graph as reachability constraints. This allows narrowing constraints to support AND, OR,
//! and NOT operations, which is essential for correctly preserving narrowing information across
//! control flow merges (e.g. after if/elif/else with terminal branches).
//!
//! [`Predicate`]: crate::predicate::Predicate

use crate::ast_ids::ScopedUseId;
use crate::reachability_constraints::ScopedReachabilityConstraintId;
use crate::scope::FileScopeId;

/// A narrowing constraint associated with a live binding.
///
/// This is a TDD node ID in the shared reachability constraints graph.
/// `ALWAYS_TRUE` means "no narrowing constraint" (the base type is unchanged).
pub type ScopedNarrowingConstraint = ScopedReachabilityConstraintId;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ConstraintKey {
    NarrowingConstraint(ScopedNarrowingConstraint),
    NestedScope(FileScopeId),
    UseId(ScopedUseId),
}
