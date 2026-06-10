//! Recursive (μ) types.
//!
//! [`RecursiveType<'db>`] is an explicit μ-binder representation of recursive types:
//! `μα. body` where `α` is referenced inside `body` as `Type::Divergent(binder_id)`.
//!
//! This module owns recursive type operations: construction, origin folding,
//! cycle recovery, and finite type-transform traversal. Type relations use
//! recursive types through their public operations, but relation-specific cycle
//! guards live with the relation checker.

use crate::Db;
use crate::types::visitor;
use crate::types::{
    ApplyTypeMappingVisitor, Type, TypeAliasType, TypeContext, TypeMapping, UnionType,
};
use salsa::plumbing::AsId;

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

/// The source whose recursion is represented by a [`RecursiveType`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, salsa::Update, get_size2::GetSize)]
pub enum RecursiveOrigin<'db> {
    /// An inferred recursion cycle with no stable source name.
    Implicit,
    /// A recursive PEP 695 type alias.
    TypeAlias(TypeAliasType<'db>),
}

impl<'db> RecursiveOrigin<'db> {
    pub(crate) fn source_type(self) -> Option<Type<'db>> {
        match self {
            Self::Implicit => None,
            Self::TypeAlias(alias) => Some(Type::TypeAlias(alias)),
        }
    }

    /// Returns true if `ty` is the source-name occurrence bound by this origin.
    ///
    /// This must stay a shallow identity check: do not call recursive queries such as
    /// alias value expansion, function signature inference, `NewType` base evaluation, or
    /// protocol interface computation here.
    pub(crate) fn matches_type(self, db: &'db dyn Db, ty: Type<'db>) -> bool {
        match (self, ty) {
            (Self::Implicit, _) => false,
            (Self::TypeAlias(alias), Type::TypeAlias(other)) => {
                alias.definition(db) == other.definition(db)
            }
            _ => false,
        }
    }

    pub(crate) fn contains_in_type(self, db: &'db dyn Db, ty: Type<'db>) -> bool {
        visitor::any_over_type(db, ty, false, |inner| self.matches_type(db, inner))
    }

    pub(crate) fn binder_id(self, db: &'db dyn Db) -> Option<salsa::Id> {
        match self {
            Self::Implicit => None,
            Self::TypeAlias(TypeAliasType::PEP695(alias)) => Some(alias.as_id()),
            Self::TypeAlias(alias) => Some(alias.definition(db).as_id()),
        }
    }

    pub(crate) fn build_recursive<'a>(
        self,
        db: &'db dyn Db,
        build_body: impl FnOnce(
            salsa::Id,
            TypeMapping<'a, 'db>,
            &ApplyTypeMappingVisitor<'db>,
        ) -> Type<'db>,
        build_fallback: impl FnOnce(Type<'db>) -> Type<'db>,
    ) -> Option<Type<'db>>
    where
        'db: 'a,
    {
        let binder_id = self.binder_id(db)?;
        let type_mapping = TypeMapping::ReplaceRecursiveOrigin {
            origin: self,
            binder_id: BinderId::new(binder_id),
        };
        let visitor = ApplyTypeMappingVisitor::default();
        let body = build_body(binder_id, type_mapping, &visitor);
        let marker = Type::divergent(binder_id);
        // A marker that is a direct union element represents the current cycle
        // head, not recursive structure inside the body.
        let body = match body {
            Type::Union(union) => {
                let elements = union.elements(db);
                let kept: Vec<Type<'db>> = elements
                    .iter()
                    .copied()
                    .filter(|ty| !ty.is_top_level_cycle_marker(db, marker))
                    .collect();

                if kept.len() == elements.len() {
                    Type::Union(union)
                } else {
                    match kept.len() {
                        0 => marker,
                        1 => kept[0],
                        _ => UnionType::from_elements(db, kept),
                    }
                }
            }
            _ => body,
        };
        if visitor::any_over_type(
            db,
            body,
            false,
            |ty| matches!(ty, Type::Divergent(divergent) if divergent.id() == binder_id),
        ) {
            Some(Type::recursive(db, binder_id, self, body))
        } else {
            Some(build_fallback(body))
        }
    }
}

/// An explicit μ-binder. Represents `μα. body` where `α` is the
/// `Type::Divergent(self.binder_id(db).into_id())` marker occurring inside `body`.
///
/// Interned by `(binder_id, origin, body)` so that two structurally identical
/// recursive types share an identity.
#[salsa::interned(debug, heap_size = ruff_memory_usage::heap_size)]
pub struct RecursiveType<'db> {
    /// Unique identifier of the μ-binder. References to this binder inside `body`
    /// appear as `Type::Divergent(binder_id.into_id())`.
    pub binder: BinderId,

    /// The construct whose recursion this μ-binder represents.
    pub origin: RecursiveOrigin<'db>,

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
    pub(crate) fn build(
        db: &'db dyn Db,
        binder_id: salsa::Id,
        origin: RecursiveOrigin<'db>,
        body: Type<'db>,
    ) -> Self {
        Self::new(db, BinderId::new(binder_id), origin, body)
    }

    /// The raw `salsa::Id` of the μ-binder.
    pub(crate) fn binder_id(self, db: &'db dyn Db) -> salsa::Id {
        self.binder(db).into_id()
    }

    /// Returns the body with its `Type::Divergent` α-binder markers substituted
    /// back to the source type when this μ-binder has an explicit origin.
    /// Use this when you do not need to perform type operations arbitrarily and would rather treat it as a finite type.
    /// In this case, the source type should be consumed as the terminal element (otherwise the source type would be expanded infinitely).
    pub(crate) fn body_with_origin_marker(self, db: &'db dyn Db) -> Type<'db> {
        let body = *self.body(db);
        let Some(replacement) = self.origin(db).source_type() else {
            return body;
        };
        let mapping = TypeMapping::ReplaceDivergent {
            binder_id: self.binder(db),
            replacement,
        };
        body.apply_type_mapping(db, &mapping, TypeContext::default())
    }

    /// Returns the body with its `Type::Divergent` α-binder markers substituted
    /// back to `Type::Recursive(self)` — the μ-binder preserved at the recursive
    /// position so further structural operations (iteration, subscript, …) can
    /// continue to descend.
    ///
    /// Compare with [`body_with_origin_marker`][Self::body_with_origin_marker],
    /// which substitutes the source type instead — used for display and for
    /// `IntersectionBuilder`'s distribution where re-finding the recursive name
    /// matters.
    pub(crate) fn unfold(self, db: &'db dyn Db) -> Type<'db> {
        let body = *self.body(db);
        let replacement = self
            .origin(db)
            .source_type()
            .unwrap_or(Type::Recursive(self));
        let mapping = TypeMapping::ReplaceDivergent {
            binder_id: self.binder(db),
            replacement,
        };
        body.apply_type_mapping(db, &mapping, TypeContext::default())
    }

    /// Whether this μ-binder is *non-contractive*: its body is the bare α-binder marker itself
    /// (`μα. α`), with no constructor around the recursive position.
    ///
    /// Unfolding such a type makes no progress (`μα.α → μα.α`), so structural operations that
    /// recurse on the one-step unfold (subscript, iteration) must not unfold it — they treat it
    /// as a gradual leaf (returning the marker itself) instead, exactly as they would a bare
    /// `Divergent`. This only arises as a not-yet-converged cycle provisional; a converged,
    /// structureless cycle is resolved away rather than wrapped.
    pub(crate) fn is_non_contractive(self, db: &'db dyn Db) -> bool {
        *self.body(db) == Type::divergent(self.binder_id(db))
    }
}
