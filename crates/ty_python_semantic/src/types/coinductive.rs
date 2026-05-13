//! Co-inductive relation framework.
//!
//! This module provides the dispatch surface used by relation checkers
//! (`has_relation_to`, `is_disjoint_from`, …) to handle opaque type names
//! ([`Type::Recursive`] and [`Type::TypeAlias`]) co-inductively.
//!
//! ## What this module provides
//!
//! - [`CoInductiveRelation`]: a dispatch trait that every relation checker
//!   implements. Exposes a `relation_key()` (value-level relation identity,
//!   used in the cycle-detection visiting key) and `check_structural(db, l, r)`
//!   (the structural-comparison body invoked after unfold + cycle guard).
//! - [`delegate_recursive`]: the canonical entry point used by `check_type_pair`
//!   arms to handle opaque type names. Records the pair in the visitor for
//!   cycle detection, unfolds one step, then dispatches to the implementer's
//!   `check_structural`.
//! - [`unfold_one`]: low-level one-step unfold helper. Most callers should
//!   prefer [`delegate_recursive`] which performs unfold together with cycle
//!   detection.

use crate::Db;
use crate::types::Type;
use crate::types::constraints::ConstraintSet;
use crate::types::cyclic::CycleDetector;
use crate::types::recursive::RecursiveType;
use crate::types::relation::TypeRelation;

/// A relation that supports co-inductive reasoning over [`Type::Recursive`].
///
/// Implementors expose:
/// - `relation_key()` — the value-level [`TypeRelation`] identifying *which*
///   relation this instance checks (`Subtyping`, `Disjointness`, …). Used as
///   the third component of the cycle-detection visiting key so that
///   different relations on the same `(Type, Type)` pair don't share cycle
///   state.
/// - `check_structural(db, l, r)` — the actual structural check, called by
///   the framework after unfold + cycle-guard logic decides this pair needs
///   a real recursion step.
pub(crate) trait CoInductiveRelation<'db, 'c> {
    /// The relation's value-level tag, used in the cycle-detection key.
    fn relation_key(&self) -> TypeRelation;

    /// Perform the structural check. The framework calls this after unfold +
    /// cycle-guard logic has decided that this pair needs a recursive step.
    fn check_structural(
        &self,
        db: &'db dyn Db,
        left: Type<'db>,
        right: Type<'db>,
    ) -> ConstraintSet<'db, 'c>;
}

/// Co-inductively delegate a relation through an opaque type name
/// ([`Type::Recursive`] or [`Type::TypeAlias`]).
///
/// - Records `(source, target, relation_key)` in the visitor *before*
///   unfolding. Using the pre-unfold types as the key prevents nested
///   non-cyclic comparisons of the same unfolded pair from being incorrectly
///   short-circuited (a post-unfold key empirically broke ~7 mdtest cases).
/// - On cycle (same triple revisited), the visitor returns its fallback.
/// - Otherwise unfolds both sides one step via [`unfold_one`] and dispatches
///   to `checker.check_structural`.
///
/// This is the canonical entry point used by `check_type_pair` arms to handle
/// opaque type names without per-checker duplicated guard logic.
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
) -> ConstraintSet<'db, 'c>
where
    R: CoInductiveRelation<'db, 'c>,
{
    let key = (source, target, checker.relation_key());
    visitor.visit(key, || {
        checker.check_structural(db, unfold_one(db, source), unfold_one(db, target))
    })
}

/// One-step unfold of an opaque type name.
///
/// Both [`Type::Recursive`] and [`Type::TypeAlias`] are "named opaque types"
/// in the relation framework: their identity is not their structure. For
/// recursive checks, the framework needs to step *inside* the name once, then
/// detect cycles structurally via [`delegate_recursive`]'s visiting set.
///
/// - `Type::Recursive(r)` → `*r.body(db)` (which contains `Divergent` markers
///   at α positions; further recursion bottoms out at `Divergent`).
/// - `Type::TypeAlias(alias)` → `alias.value_type(db)` (which, if the alias
///   is self-referential, is itself a `Type::Recursive`; the next unfold step
///   handles that).
/// - All other types are returned unchanged.
pub(crate) fn unfold_one<'db>(db: &'db dyn Db, ty: Type<'db>) -> Type<'db> {
    match ty {
        Type::Recursive(r) => *r.body(db),
        Type::TypeAlias(alias) => alias.value_type(db),
        _ => ty,
    }
}

/// Find the `Type::Recursive` that wraps `divergent`'s α-binder by scanning a
/// relation-framework visitor's active seen set. Returns `None` if no such
/// `Type::Recursive` is currently being visited (legacy implicit-recursion path
/// where `Divergent` is used as a dynamic-like marker without a corresponding
/// `Type::Recursive` wrapper).
///
/// Shared by `TypeRelationChecker` and `DisjointnessChecker`; they pass their
/// respective visitor.
pub(crate) fn find_wrapping_recursive<'db>(
    db: &'db dyn Db,
    divergent: crate::types::DivergentType,
    visitor: &CycleDetector<
        TypeRelation,
        (Type<'db>, Type<'db>, TypeRelation),
        ConstraintSet<'db, '_>,
    >,
) -> Option<RecursiveType<'db>> {
    let binder_id = divergent.id();
    let found = std::cell::Cell::new(None);
    visitor.any_active(|(left, right, _)| {
        for side in [*left, *right] {
            if let Type::Recursive(rec) = side
                && rec.binder_id(db) == binder_id
            {
                found.set(Some(rec));
                return true;
            }
        }
        false
    });
    found.into_inner()
}
