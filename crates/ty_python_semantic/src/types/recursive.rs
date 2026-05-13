//! Recursive (μ) types.
//!
//! [`RecursiveType<'db>`] is an explicit μ-binder representation of recursive types:
//! `μα. body` where `α` is referenced inside `body` as `Type::Divergent(binder_id)`.
//!
//! For self-referential PEP 695 type aliases, `PEP695TypeAliasType::value_type`
//! constructs a `Type::Recursive` whose `body` has each self-reference replaced
//! by `Type::Divergent(binder_id)`. The co-inductive relation framework in
//! [`crate::types::coinductive`] dispatches on `Type::Recursive` by unfolding
//! one step and recording the visiting pair to break cycles.

use crate::Db;
use crate::types::{Type, TypeAliasType, TypeContext, TypeMapping};
use ty_python_core::definition::Definition;

/// Wrapper around `salsa::Id` that implements `GetSize` so it can be used as a
/// field of a `#[salsa::interned]` struct that uses `heap_size`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, salsa::Update)]
pub struct BinderId(salsa::Id);

impl get_size2::GetSize for BinderId {}

impl BinderId {
    pub fn new(id: salsa::Id) -> Self {
        Self(id)
    }
    pub fn into_id(self) -> salsa::Id {
        self.0
    }
}

/// An explicit μ-binder. Represents `μα. body` where `α` is the
/// `Type::Divergent(self.binder_id(db).into_id())` marker occurring inside `body`.
///
/// Interned by `(binder_id, source_alias, body)` so that two structurally identical
/// recursive types share an identity.
#[salsa::interned(debug, heap_size = ruff_memory_usage::heap_size)]
pub struct RecursiveType<'db> {
    /// Unique identifier of the μ-binder. References to this binder inside `body`
    /// appear as `Type::Divergent(binder_id.into_id())`.
    pub binder: BinderId,

    /// The PEP 695 (or manual) alias whose body this recursive type was constructed
    /// from. Used for display: a `Divergent(binder_id)` inside `body` is rendered as
    /// the alias's name. `None` for implicit recursive types from inference cycles.
    pub source_alias: Option<TypeAliasType<'db>>,

    /// The body of the recursive type, possibly containing the binder's `Divergent`
    /// marker at the recursive positions.
    #[returns(ref)]
    pub body: Type<'db>,
}

// The Salsa heap is tracked separately.
impl get_size2::GetSize for RecursiveType<'_> {}

impl<'db> RecursiveType<'db> {
    /// Construct a new μ-binder. The caller is responsible for ensuring that `body`
    /// references this binder via `Type::Divergent(binder_id)` at appropriate positions.
    ///
    /// `source_alias` is `Some` when the recursive type came from folding a named
    /// type alias body — used to display `Divergent(binder_id)` inside `body` as the
    /// alias's name. Pass `None` for implicit recursive types from inference cycles.
    #[allow(dead_code)]
    pub(crate) fn build(
        db: &'db dyn Db,
        binder_id: salsa::Id,
        source_alias: Option<TypeAliasType<'db>>,
        body: Type<'db>,
    ) -> Self {
        Self::new(db, BinderId::new(binder_id), source_alias, body)
    }

    /// The raw `salsa::Id` of the μ-binder.
    pub(crate) fn binder_id(self, db: &'db dyn Db) -> salsa::Id {
        self.binder(db).into_id()
    }

    /// One-step unfold: returns the body as-is. The body contains
    /// `Type::Divergent(binder_id)` markers at recursive positions; downstream
    /// callers in the co-inductive framework rely on those markers bottoming
    /// out at the `Type::Divergent` arm of relation checks.
    ///
    /// Note: this is not the textbook equi-recursive unfold
    /// `μα.body → body[α := μα.body]` — that would re-introduce the recursive
    /// type. The framework's cycle detection works by recording the pre-unfold
    /// pair in the visitor, so the simpler "body with Divergent leaves" form
    /// is sufficient.
    #[allow(dead_code)]
    pub(crate) fn unfold(self, db: &'db dyn Db) -> Type<'db> {
        *self.body(db)
    }

    /// Returns the body with its `Type::Divergent` α-binder markers substituted
    /// back to the source `Type::TypeAlias` (when `source_alias` is `Some`).
    /// Used by display and by `IntersectionBuilder`'s `Type::Recursive` arms.
    ///
    /// Re-tagging recursive positions as `Type::TypeAlias` lets downstream
    /// `seen_aliases`-style cycle guards (and display) treat them as the same
    /// opaque name rather than the bare `Divergent` marker.
    pub(crate) fn body_with_alias_marker(self, db: &'db dyn Db) -> Type<'db> {
        let body = *self.body(db);
        let Some(source_alias) = self.source_alias(db) else {
            return body;
        };
        let mapping = TypeMapping::ReplaceDivergent {
            binder_id: self.binder(db),
            replacement: Type::TypeAlias(source_alias),
        };
        body.apply_type_mapping(db, &mapping, TypeContext::default())
    }

    /// Returns the body with its `Type::Divergent` α-binder markers substituted
    /// back to `Type::Recursive(self)` — the μ-binder preserved at the recursive
    /// position so further structural operations (iteration, subscript, …) can
    /// continue to descend.
    ///
    /// Compare with [`body_with_alias_marker`][Self::body_with_alias_marker],
    /// which substitutes the source `TypeAlias` instead — used for display and
    /// for `IntersectionBuilder`'s distribution where re-finding the alias name
    /// matters.
    pub(crate) fn unfold_preserving_binder(self, db: &'db dyn Db) -> Type<'db> {
        let body = *self.body(db);
        let mapping = TypeMapping::ReplaceDivergent {
            binder_id: self.binder(db),
            replacement: Type::Recursive(self),
        };
        body.apply_type_mapping(db, &mapping, TypeContext::default())
    }
}

/// Folds a Type by replacing self-references to the given alias definition with
/// `Type::Divergent(binder_id)` markers — used to build a `Type::Recursive` body
/// from a raw alias body.
///
/// The resulting type has `Divergent(binder_id)` at recursive positions, making
/// the structure finite (recursion is captured by the binder rather than by
/// repeated `TypeAlias` references).
#[allow(dead_code)]
pub(crate) fn substitute_self_alias_with_divergent<'db>(
    db: &'db dyn Db,
    ty: Type<'db>,
    alias_def: Definition<'db>,
    binder_id: salsa::Id,
) -> Type<'db> {
    use salsa::plumbing::AsId;
    let mapping = TypeMapping::ReplaceSelfAlias {
        alias_def_id: BinderId::new(alias_def.as_id()),
        binder_id: BinderId::new(binder_id),
    };
    ty.apply_type_mapping(db, &mapping, TypeContext::default())
}
