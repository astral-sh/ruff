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
use crate::types::function::FunctionType;
use crate::types::newtype::NewType;
use crate::types::visitor;
use crate::types::{
    ApplyTypeMappingVisitor, ProtocolInstanceType, Type, TypeAliasType, TypeContext, TypeMapping,
    TypedDictType,
};
use salsa::plumbing::AsId;

impl<'db> Type<'db> {
    /// Return the key used by type-transform visitors for cycle detection.
    ///
    /// Recursive origins that can appear as ordinary `Type` variants are keyed by
    /// their `Type::Recursive` wrapper, so the generic cycle detector does not need
    /// origin-specific identity logic.
    pub(crate) fn visit_key(self, db: &'db dyn Db) -> Type<'db> {
        match self {
            Type::FunctionLiteral(function) => function.try_to_recursive_type(db),
            Type::NewTypeInstance(newtype) => newtype.try_to_recursive_type(db),
            Type::ProtocolInstance(protocol) => protocol.try_to_recursive_type(db),
            _ => self,
        }
    }
}

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
    /// A recursive function object, such as a `TypeOf` self-reference in annotations.
    Function(FunctionType<'db>),
    /// A recursive `typing.NewType` definition.
    NewType(NewType<'db>),
    /// A recursive `TypedDict` schema.
    TypedDict(TypedDictType<'db>),
    /// A recursive protocol interface.
    Protocol(ProtocolInstanceType<'db>),
}

impl<'db> RecursiveOrigin<'db> {
    pub(crate) fn source_type(self) -> Option<Type<'db>> {
        match self {
            Self::Implicit => None,
            Self::TypeAlias(alias) => Some(Type::TypeAlias(alias)),
            Self::Function(function) => Some(Type::FunctionLiteral(function)),
            Self::NewType(newtype) => Some(Type::NewTypeInstance(newtype)),
            Self::TypedDict(typed_dict) => Some(Type::TypedDict(typed_dict)),
            Self::Protocol(protocol) => Some(Type::ProtocolInstance(protocol)),
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
            (Self::Function(function), Type::FunctionLiteral(other)) => {
                function.literal(db) == other.literal(db)
            }
            (Self::NewType(newtype), Type::NewTypeInstance(other)) => {
                newtype.definition(db) == other.definition(db)
            }
            (Self::TypedDict(typed_dict), Type::TypedDict(other)) => {
                typed_dict.recursive_binder_id() == other.recursive_binder_id()
            }
            (Self::Protocol(protocol), Type::ProtocolInstance(other)) => {
                protocol.is_same_recursive_origin_as(db, other)
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
            Self::Function(function) => Some(function.as_id()),
            Self::NewType(newtype) => Some(newtype.definition(db).as_id()),
            Self::TypedDict(typed_dict) => Some(typed_dict.recursive_binder_id()),
            Self::Protocol(protocol) => Some(protocol.recursive_binder_id(db)),
        }
    }

    pub(crate) fn build_recursive<'a>(
        self,
        db: &'db dyn Db,
        build_body: impl FnOnce(
            salsa::Id,
            TypeMapping<'a, 'db>,
            &ApplyTypeMappingVisitor<'db>,
        ) -> (Type<'db>, Type<'db>),
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
        let (body, fallback) = build_body(binder_id, type_mapping, &visitor);
        if visitor::any_over_type(
            db,
            body,
            false,
            |ty| matches!(ty, Type::Divergent(divergent) if divergent.id() == binder_id),
        ) {
            Some(Type::recursive(db, binder_id, self, body))
        } else {
            Some(fallback)
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

    pub(crate) fn is_implicit(self, db: &'db dyn Db) -> bool {
        matches!(self.origin(db), RecursiveOrigin::Implicit)
    }

    pub(crate) fn has_explicit_origin(self, db: &'db dyn Db) -> bool {
        !self.is_implicit(db)
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
