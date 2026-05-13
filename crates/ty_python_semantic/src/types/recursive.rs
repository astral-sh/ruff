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
use crate::types::Type;

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
/// Interned by `(binder_id, body)` so that two structurally identical recursive
/// types share an identity.
#[salsa::interned(debug, heap_size = ruff_memory_usage::heap_size)]
pub struct RecursiveType<'db> {
    /// Unique identifier of the μ-binder. References to this binder inside `body`
    /// appear as `Type::Divergent(binder_id.into_id())`.
    pub binder: BinderId,

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
    /// Used by later phases (Phase 2+) when `value_type` folds recursive aliases.
    #[allow(dead_code)]
    pub(crate) fn build(db: &'db dyn Db, binder_id: salsa::Id, body: Type<'db>) -> Self {
        Self::new(db, BinderId::new(binder_id), body)
    }

    /// The raw `salsa::Id` of the μ-binder.
    pub(crate) fn binder_id(self, db: &'db dyn Db) -> salsa::Id {
        self.binder(db).into_id()
    }
}
