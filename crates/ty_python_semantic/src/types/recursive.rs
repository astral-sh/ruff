//! Recursive (μ) types.
//!
//! [`RecursiveType<'db>`] is an explicit μ-binder representation of recursive types:
//! `μα. body` where `α` is referenced inside `body` as `Type::Divergent(binder_id)`.
//!
//! For self-referential PEP 695 type aliases, `PEP695TypeAliasType::value_type`
//! constructs a `Type::Recursive` whose `body` has each self-reference replaced
//! by `Type::Divergent(binder_id)`. Recursive relation checks dispatch on
//! `Type::Recursive` by unfolding one step and recording the visiting pair to
//! break cycles.

use crate::Db;
use crate::types::constraints::{ConstraintSet, ConstraintSetBuilder};
use crate::types::cyclic::CycleDetector;
use crate::types::function::FunctionType;
use crate::types::newtype::NewType;
use crate::types::relation::TypeRelation;
use crate::types::{
    DivergentType, ProtocolInstanceType, Type, TypeAliasType, TypeContext, TypeMapping,
    TypedDictType,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct RecursiveRelationKey<'db> {
    left: Type<'db>,
    right: Type<'db>,
    relation: TypeRelation,
}

pub(crate) struct RecursiveRelationVisitor<'db, 'c> {
    visitor: CycleDetector<TypeRelation, RecursiveRelationKey<'db>, ConstraintSet<'db, 'c>>,
}

impl<'db, 'c> RecursiveRelationVisitor<'db, 'c> {
    /// Construct a visitor for directional relation checks. The fallback is
    /// `true`: if we loop, assume the relation currently being proven holds.
    pub(crate) fn assume_related_on_cycle(constraints: &'c ConstraintSetBuilder<'db>) -> Self {
        Self {
            visitor: CycleDetector::new(ConstraintSet::from_bool(constraints, true)),
        }
    }

    /// Construct a visitor for disjointness checks. The fallback is `false`:
    /// if we loop, assume the types are not disjoint.
    pub(crate) fn assume_not_disjoint_on_cycle(constraints: &'c ConstraintSetBuilder<'db>) -> Self {
        Self {
            visitor: CycleDetector::new(ConstraintSet::from_bool(constraints, false)),
        }
    }

    pub(crate) fn visit_pair(
        &self,
        left: Type<'db>,
        right: Type<'db>,
        relation: TypeRelation,
        work: impl FnOnce() -> ConstraintSet<'db, 'c>,
    ) -> ConstraintSet<'db, 'c> {
        self.visitor.visit(
            RecursiveRelationKey {
                left,
                right,
                relation,
            },
            work,
        )
    }

    /// Find the `Type::Recursive` that wraps `divergent`'s α-binder by scanning
    /// the active relation pairs.
    fn wrapping_recursive_for_divergent(
        &self,
        db: &'db dyn Db,
        divergent: DivergentType,
    ) -> Option<RecursiveType<'db>> {
        let binder_id = divergent.id();
        let found = std::cell::Cell::new(None);
        self.visitor.any_active(|key| {
            for side in [key.left, key.right] {
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
}

/// A relation that supports recursive reasoning over [`Type::Recursive`].
pub(crate) trait RecursiveRelation<'db, 'c> {
    /// The relation's value-level tag, used in the cycle-detection key.
    fn relation_key(&self) -> TypeRelation;

    /// Perform the structural check after one recursive unfold.
    fn check_structural(
        &self,
        db: &'db dyn Db,
        left: Type<'db>,
        right: Type<'db>,
    ) -> ConstraintSet<'db, 'c>;

    /// Return true if a bare `Divergent` marker should be resolved to `recursive`
    /// for this relation.
    fn should_resolve_divergent_marker(
        &self,
        db: &'db dyn Db,
        recursive: RecursiveType<'db>,
    ) -> bool;

    /// Result to use when a `Divergent` marker cannot be resolved to a wrapping
    /// recursive type, or when this relation rejects the wrapping type.
    fn unresolved_divergent_result(&self) -> ConstraintSet<'db, 'c>;
}

/// Delegate a relation through a [`Type::Recursive`] μ-binder.
///
/// Records the pre-unfold pair in the visitor before unfolding. This keeps
/// recursive relation checks finite while allowing a real structural step for
/// non-cyclic pairs.
pub(crate) fn check_recursive_relation<'db, 'c, R>(
    db: &'db dyn Db,
    checker: &R,
    source: Type<'db>,
    target: Type<'db>,
    visitor: &RecursiveRelationVisitor<'db, 'c>,
) -> ConstraintSet<'db, 'c>
where
    R: RecursiveRelation<'db, 'c>,
{
    visitor.visit_pair(source, target, checker.relation_key(), || {
        checker.check_structural(
            db,
            source.unfold_recursive_once(db),
            target.unfold_recursive_once(db),
        )
    })
}

/// Delegate a relation through a bare [`Type::Divergent`] marker if it is the
/// α-binder of an in-flight [`Type::Recursive`] visit.
///
/// Replacing the marker with its wrapping `Type::Recursive` lets relation checks
/// re-enter through normal recursive pair detection. Without this, the bare
/// fallback is too permissive for directional relations; for example,
/// `str <: Divergent` would succeed under assignability and break invariance
/// checks on `list[Recursive]`.
pub(crate) fn check_divergent_relation<'db, 'c, R>(
    db: &'db dyn Db,
    checker: &R,
    left: Type<'db>,
    right: Type<'db>,
    divergent: DivergentType,
    visitor: &RecursiveRelationVisitor<'db, 'c>,
) -> ConstraintSet<'db, 'c>
where
    R: RecursiveRelation<'db, 'c>,
{
    if let Some(wrapping) = visitor.wrapping_recursive_for_divergent(db, divergent)
        && checker.should_resolve_divergent_marker(db, wrapping)
    {
        let recursive_ty = Type::Recursive(wrapping);
        if matches!(left, Type::Divergent(d) if d.id() == divergent.id()) {
            checker.check_structural(db, recursive_ty, right)
        } else {
            checker.check_structural(db, left, recursive_ty)
        }
    } else {
        checker.unresolved_divergent_result()
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
    /// alias value expansion, function signature inference, NewType base evaluation, or
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

    pub(crate) fn binder_id(self, db: &'db dyn Db) -> Option<salsa::Id> {
        use salsa::plumbing::AsId;

        match self {
            Self::Implicit => None,
            Self::TypeAlias(alias) => Some(alias.definition(db).as_id()),
            Self::Function(function) => Some(function.as_id()),
            Self::NewType(newtype) => Some(newtype.definition(db).as_id()),
            Self::TypedDict(typed_dict) => Some(typed_dict.recursive_binder_id()),
            Self::Protocol(protocol) => Some(protocol.recursive_binder_id(db)),
        }
    }

    pub(crate) fn fold_self_references(
        self,
        db: &'db dyn Db,
        ty: Type<'db>,
        binder_id: salsa::Id,
    ) -> Type<'db> {
        let mapping = TypeMapping::ReplaceRecursiveOrigin {
            origin: self,
            binder_id: BinderId::new(binder_id),
        };
        ty.apply_type_mapping(db, &mapping, TypeContext::default())
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
    /// back to the source type when this μ-binder has an explicit origin.
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
    pub(crate) fn unfold_preserving_binder(self, db: &'db dyn Db) -> Type<'db> {
        let body = *self.body(db);
        let mapping = TypeMapping::ReplaceDivergent {
            binder_id: self.binder(db),
            replacement: Type::Recursive(self),
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
    alias: TypeAliasType<'db>,
    binder_id: salsa::Id,
) -> Type<'db> {
    let mapping = TypeMapping::ReplaceRecursiveOrigin {
        origin: RecursiveOrigin::TypeAlias(alias),
        binder_id: BinderId::new(binder_id),
    };
    ty.apply_type_mapping(db, &mapping, TypeContext::default())
}
