//! Co-inductive relation framework.
//!
//! This module provides a small framework for writing type relations that
//! transparently handle [`Type::Recursive`] via co-inductive reasoning.
//!
//! ## Background
//!
//! The Phase 0 codebase had a "visitor zoo": each relation
//! (`has_relation_to`, `is_disjoint_from`, `is_equivalent_to`, …) carried its
//! own [`crate::types::cyclic::PairVisitor`] alias and threaded a `&Visitor`
//! parameter through many `_impl` methods. Phase 3 added a [`Type::Recursive`]
//! variant whose body contains [`Type::Divergent`] markers at recursive
//! positions. The current arms in [`crate::types::relation`] handle that
//! variant via `with_recursion_guard(...)`, which is itself a thin wrapper
//! over the relation's `PairVisitor`.
//!
//! This module abstracts the shared pattern (unfold → delegate → cycle-guard)
//! into helpers, so that subsequent phases can migrate each relation onto a
//! uniform API.
//!
//! ## What this module provides
//!
//! - [`CoInductiveRelation`]: a dispatch trait that every relation checker
//!   implements. Exposes a `relation_key()` (value-level relation identity,
//!   used in the cycle-detection visiting key) and `check_structural(db, l, r)`
//!   (the structural-comparison body invoked after unfold + cycle guard).
//! - [`delegate_recursive`]: the canonical entry point used by `check_type_pair`
//!   arms to handle `Type::Recursive`. Unfolds once, records the pair in the
//!   visitor, dispatches to the implementer's `check_structural`. Removes the
//!   need for per-checker `with_recursion_guard` boilerplate at recursive
//!   arms.
//! - [`unfold_one`] / [`unfold_pair`]: low-level one-step unfold helpers.
//!   Most callers should prefer [`delegate_recursive`] which performs unfold
//!   together with cycle detection.

use crate::Db;
use crate::types::Type;
use crate::types::constraints::ConstraintSet;
use crate::types::cyclic::CycleDetector;
use crate::types::relation::TypeRelation;

/// A relation that supports co-inductive reasoning over [`Type::Recursive`].
///
/// Implementors associate:
/// - a `Tag` marker type (used to disambiguate the cycle-detection set)
/// - an `Output` type (typically `bool` or `ConstraintSet<'db, 'c>`)
///
/// They also expose:
/// - `relation_key()` — the value-level tag identifying *which* relation this
///   instance checks (e.g. `TypeRelation::Subtyping`, `TypeRelation::Disjointness`).
///   Used as the third component of the cycle-detection visiting key so that
///   different relations on the same `(Type, Type)` pair don't share cycle
///   state.
/// - `check_structural(db, l, r)` — the actual structural check, called by the
///   framework after unfold + cycle-guard logic decides this pair needs a real
///   recursion step.
///
/// Phase 5 makes this trait dispatchable from [`delegate_recursive`], which
/// uniformly handles `Type::Recursive` unfolding for any relation that
/// implements the trait.
pub(crate) trait CoInductiveRelation<'db, 'c> {
    type Tag: 'static;
    type Output: Clone;

    /// The relation's value-level tag, used in the cycle-detection key.
    fn relation_key(&self) -> Self::Tag;

    /// Perform the structural check. The framework calls this after unfold +
    /// cycle-guard logic has decided that this pair needs a recursive step.
    fn check_structural(&self, db: &'db dyn Db, left: Type<'db>, right: Type<'db>) -> Self::Output;
}

/// Co-inductively delegate a relation through `Type::Recursive` on either
/// side.
///
/// - Unfolds `Type::Recursive` (one step) on whichever side has it.
/// - Records `(l, r, relation_key)` in the visitor; if the same triple recurses,
///   the visitor returns its fallback (co-inductive hypothesis).
/// - Otherwise dispatches to `checker.check_structural(db, l, r)`.
///
/// This is the canonical Phase 5+ entry point used by `check_type_pair` arms
/// to handle `Type::Recursive` without per-checker duplicated guard logic.
pub(crate) fn delegate_recursive<'db, 'c, R>(
    db: &'db dyn Db,
    checker: &R,
    source: Type<'db>,
    target: Type<'db>,
    visitor: &CycleDetector<
        TypeRelation,
        (Type<'db>, Type<'db>, TypeRelation),
        ConstraintSet<'db, 'c>,
    >,
) -> R::Output
where
    R: CoInductiveRelation<'db, 'c, Tag = TypeRelation, Output = ConstraintSet<'db, 'c>>,
{
    let (l, r) = unfold_pair(db, source, target);
    let key = (l, r, checker.relation_key());
    visitor.visit(key, || checker.check_structural(db, l, r))
}

/// One-step unfold of a [`Type::Recursive`] to its body.
///
/// The body contains [`Type::Divergent`] markers at recursive positions; for
/// relations that delegate to a cycle-guarded recursive call, this is the
/// canonical "unfold once, then keep going" step. Subsequent recursion bottoms
/// out at `Divergent`, which existing relation arms handle as the relation's
/// neutral element.
///
/// Non-`Recursive` types are returned unchanged.
pub(crate) fn unfold_one<'db>(db: &'db dyn Db, ty: Type<'db>) -> Type<'db> {
    match ty {
        Type::Recursive(r) => *r.body(db),
        _ => ty,
    }
}

/// Unfold both sides of a pair if either is a [`Type::Recursive`].
pub(crate) fn unfold_pair<'db>(
    db: &'db dyn Db,
    left: Type<'db>,
    right: Type<'db>,
) -> (Type<'db>, Type<'db>) {
    (unfold_one(db, left), unfold_one(db, right))
}
