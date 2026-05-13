//! Recursive (μ) types.
//!
//! [`RecursiveType<'db>`] is an explicit μ-binder representation of recursive types:
//! `μα. body` where `α` is referenced inside `body` as `Type::Divergent(binder_id)`.
//!
//! This module is introduced in Phase 1 of the μ-type proof-of-concept project.
//! At this phase, the variant exists structurally but is treated equivalently to
//! a bare `Type::Divergent(binder_id)` by all match arms — no semantic change.
//!
//! In later phases:
//! - Phase 2: `PEP695TypeAliasType::value_type` will construct `Type::Recursive`
//!   for self-referential aliases.
//! - Phase 3: A one-step `unfold` operation will be added.
//! - Phase 4+: A co-inductive operation framework will dispatch on `Type::Recursive`.

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
    /// the alias's name. `None` for implicit recursive types from inference cycles
    /// (those will be introduced in later phases).
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

    /// One-step unfold: substitute `Type::Divergent(binder_id)` in body with
    /// `Type::Recursive(self)`. This is the standard equi-recursive
    /// unfold operation `μα.body → body[α := μα.body]`.
    ///
    /// In Phase 3+, operations that need to "look inside" a recursive type
    /// call this to get one-step expansion, then use co-inductive cycle
    /// detection (via pair-visiting set) for subsequent recursion.
    #[allow(dead_code)]
    pub(crate) fn unfold(self, db: &'db dyn Db) -> Type<'db> {
        let body = *self.body(db);
        // TODO Phase 3+: implement actual substitution. For now this is a stub
        // that returns the body as-is. When the body contains
        // `Type::Divergent(binder_id)` markers, those need to be replaced with
        // `Type::Recursive(self)` to produce true one-step unfold.
        //
        // Implementation will use the TypeMapping mechanism (similar to
        // ReplaceSelfAlias but in reverse: replace Divergent → Recursive).
        body
    }
}

/// Folds a Type by replacing self-references to the given alias definition with
/// `Type::Divergent(binder_id)` markers. Used by Phase 3+ to construct
/// `Type::Recursive` bodies from raw alias bodies.
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
