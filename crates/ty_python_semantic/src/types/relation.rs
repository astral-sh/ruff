mod callable;
pub(in crate::types) mod dependencies;
pub(in crate::types) mod disjoint_intersection;
mod disjointness_effects;
pub(in crate::types) mod execution;
mod field_reads;
#[cfg(test)]
mod field_reads_tests;
mod guard;
#[cfg(test)]
mod guard_measurement;
mod pair;
mod pair_effects;
mod preparation;
pub(in crate::types) mod redundancy;
mod resources;
#[cfg(test)]
pub(in crate::types) mod runtime;
#[cfg(test)]
pub(in crate::types) mod runtime_resources;
#[cfg(feature = "experimental-analysis")]
pub(in crate::types) mod source;
pub(in crate::types) mod source_intersection;
#[cfg(any(test, feature = "experimental-analysis"))]
pub(crate) mod source_operations;
#[cfg(any(test, feature = "experimental-analysis"))]
pub(in crate::types) mod stable_storage;
pub(in crate::types) mod target_intersection;
pub(in crate::types) mod target_union;
mod typevar_subclass;

#[cfg(test)]
pub(in crate::types) use callable::scheduled_requests;

#[cfg(test)]
mod borrowed_constraint_probe;

pub(in crate::types) use field_reads::RelationFieldReads;
use pair_effects::{AsyncConstraintSet, AsyncIteratorConstraints, AsyncOptionConstraints};
pub(in crate::types) use resources::RelationOwners;

use crate::{FxOrderMap, FxOrderSet, ProgramEnvironment};
use std::borrow::Cow;
use std::cell::{Cell, RefCell};
use std::convert::Infallible;
use std::future::ready;

use rustc_hash::FxHashSet;

use crate::place::{DefinedPlace, Place};
use crate::types::callable::CallableTypeKind;
use crate::types::constraints::{
    ConstraintSetBuilder, IteratorConstraintsExtension, OptionConstraintsExtension,
    OwnedConstraintSet,
};
use crate::types::cyclic::{HasIdentity, PairVisitor, TypeIdentity};
use crate::types::enums::EnumClassLiteral;
use crate::types::function::FunctionDecorators;
use crate::types::known_instance::{FunctoolsPartialInstance, MethodWrapper, SentinelInstance};
use crate::types::relation_error::ErrorRelation;
use crate::types::signatures::effects::legacy_inline;
use crate::types::signatures::{ParametersKind, SignatureRelationVisitor};
use crate::types::typevar::TypeVarDomain;
use crate::types::visitor::{TypeKind, TypeVisitor, walk_non_atomic_type};
use crate::types::{
    ApplyTypeMappingVisitor, BoundMethodType, BoundSuperType, BoundTypeVarIdentity,
    BoundTypeVarInstance, CallableType, ClassLiteral, ClassType, CycleDetector, EnumComplementType,
    EnumLiteralType, FunctionType, GenericAlias, InternedType, IntersectionType,
    KnownBoundMethodType, KnownClass, KnownInstanceType, LiteralValueType, LiteralValueTypeKind,
    MemberLookupPolicy, NewType, NominalInstanceType, PropertyInstanceType, ProtocolInstanceType,
    RecursiveType, SpecialFormType, StaticClassLiteral, SubclassOfInner, SubclassOfType,
    TypeAliasType, TypeFormType, TypeVarBoundOrConstraints, TypedDictType, UnionType,
};
use crate::{
    Db,
    types::{
        ErrorContext, ErrorContextTree, Type, TypePair, constraints::ConstraintSet,
        typevar::TypeVarSet,
    },
};

#[salsa::tracked(configuration = (pub(in crate::types) WhenConstraintSetAssignableToOwnedImplConfiguration), attempt = ReturnOnly,
    returns(ref),
    cycle_initial=|_, _, _| OwnedConstraintSet::always(),
    heap_size=ruff_memory_usage::heap_size,
)]
fn when_constraint_set_assignable_to_owned_impl<'db>(
    db: &'db dyn Db,
    types: TypePair<'db>,
) -> OwnedConstraintSet<'db> {
    owned_relation_impl(db, types, OwnedRelationKind::Assignability)
}

#[salsa::tracked(configuration = (pub(in crate::types) WhenConstraintSetEquivalentToImplConfiguration), attempt = ReturnOnly,
    returns(ref),
    cycle_initial=|_, _, _| OwnedConstraintSet::always(),
    heap_size=ruff_memory_usage::heap_size,
)]
fn when_constraint_set_equivalent_to_impl<'db>(
    db: &'db dyn Db,
    types: TypePair<'db>,
) -> OwnedConstraintSet<'db> {
    owned_relation_impl(db, types, OwnedRelationKind::Equivalence)
}

#[salsa::tracked(configuration = (pub(in crate::types) IsRedundantWithImplConfiguration), attempt = ReturnOnly, returns(copy), cycle_initial=|_, _, _| true, heap_size=ruff_memory_usage::heap_size)]
fn is_redundant_with_impl<'db>(db: &'db dyn Db, types: TypePair<'db>) -> bool {
    redundancy::produce_sync(types, &redundancy::OrdinaryRedundancyProducer { db })
        .unwrap_or_else(|never| match never {})
}

#[cfg(any(test, feature = "experimental-analysis"))]
pub(super) fn owned_assignability_ingredient(
    db: &dyn Db,
) -> &salsa::plumbing::function::IngredientImpl<WhenConstraintSetAssignableToOwnedImplConfiguration>
{
    when_constraint_set_assignable_to_owned_impl::fn_ingredient_(db, db.zalsa())
}

#[cfg(any(test, feature = "experimental-analysis"))]
pub(super) fn owned_equivalence_ingredient(
    db: &dyn Db,
) -> &salsa::plumbing::function::IngredientImpl<WhenConstraintSetEquivalentToImplConfiguration> {
    when_constraint_set_equivalent_to_impl::fn_ingredient_(db, db.zalsa())
}

#[cfg(any(test, feature = "experimental-analysis"))]
pub(super) fn redundancy_ingredient(
    db: &dyn Db,
) -> &salsa::plumbing::function::IngredientImpl<IsRedundantWithImplConfiguration> {
    is_redundant_with_impl::fn_ingredient_(db, db.zalsa())
}

/// A non-exhaustive enumeration of relations that can exist between types.
#[derive(Debug, Copy, Clone, Hash, PartialEq, Eq)]
pub(crate) enum TypeRelation {
    /// The "subtyping" relation.
    ///
    /// A [fully static] type `B` is a subtype of a fully static type `A` if and only if
    /// the set of possible runtime values represented by `B` is a subset of the set
    /// of possible runtime values represented by `A`.
    ///
    /// For a pair of types `C` and `D` that may or may not be fully static,
    /// `D` can be said to be a subtype of `C` if every possible fully static
    /// [materialization] of `D` is a subtype of every possible fully static
    /// materialization of `C`. Another way of saying this is that `D` will be a
    /// subtype of `C` if and only if the union of all possible sets of values
    /// represented by `D` (the "top materialization" of `D`) is a subtype of the
    /// intersection of all possible sets of values represented by `C` (the "bottom
    /// materialization" of `C`). More concisely: `D <: C` iff `Top[D] <: Bottom[C]`.
    ///
    /// For example, `list[Any]` can be said to be a subtype of `Sequence[object]`,
    /// because every possible fully static materialization of `list[Any]` (`list[int]`,
    /// `list[str]`, `list[bytes | bool]`, `list[SupportsIndex]`, etc.) would be
    /// considered a subtype of `Sequence[object]`.
    ///
    /// Note that this latter expansion of the subtyping relation to non-fully-static
    /// types is not described in the typing spec, but this expansion to gradual types is
    /// sound and consistent with the principles laid out in the spec. This definition
    /// does mean the subtyping relation is not reflexive for non-fully-static types
    /// (e.g. `Any` is not a subtype of `Any`).
    ///
    /// [fully static]: https://typing.python.org/en/latest/spec/glossary.html#term-fully-static-type
    /// [materialization]: https://typing.python.org/en/latest/spec/glossary.html#term-materialize
    Subtyping,

    /// The "assignability" relation.
    ///
    /// The assignability relation between two types `A` and `B` dictates whether a
    /// type checker should emit an error when a value of type `B` is assigned to a
    /// variable declared as having type `A`.
    ///
    /// For a pair of [fully static] types `A` and `B`, the assignability relation
    /// between `A` and `B` is the same as the subtyping relation.
    ///
    /// Between a pair of `C` and `D` where either `C` or `D` is not fully static, the
    /// assignability relation may be more permissive than the subtyping relation. `D`
    /// can be said to be assignable to `C` if *some* possible fully static [materialization]
    /// of `D` is a subtype of *some* possible fully static materialization of `C`.
    /// Another way of saying this is that `D` will be assignable to `C` if and only if the
    /// intersection of all possible sets of values represented by `D` (the "bottom
    /// materialization" of `D`) is a subtype of the union of all possible sets of values
    /// represented by `C` (the "top materialization" of `C`).
    /// More concisely: `D <: C` iff `Bottom[D] <: Top[C]`.
    ///
    /// For example, `Any` is not a subtype of `int`, because there are possible
    /// materializations of `Any` (e.g., `str`) that are not subtypes of `int`.
    /// `Any` is *assignable* to `int`, however, as there are *some* possible materializations
    /// of `Any` (such as `int` itself!) that *are* subtypes of `int`. `Any` cannot even
    /// be considered a subtype of itself, as two separate uses of `Any` in the same scope
    /// might materialize to different types between which there would exist no subtyping
    /// relation; nor is `Any` a subtype of `int | Any`, for the same reason. Nonetheless,
    /// `Any` is assignable to both `Any` and `int | Any`.
    ///
    /// While `Any` can materialize to anything, the presence of `Any` in a type does not
    /// necessarily make it assignable to everything. For example, `list[Any]` is not
    /// assignable to `int`, because there are no possible fully static types we could
    /// substitute for `Any` in this type that would make it a subtype of `int`. For the
    /// same reason, a union such as `str | Any` is not assignable to `int`.
    ///
    /// [fully static]: https://typing.python.org/en/latest/spec/glossary.html#term-fully-static-type
    /// [materialization]: https://typing.python.org/en/latest/spec/glossary.html#term-materialize
    Assignability,

    /// The "redundancy" relation.
    ///
    /// The redundancy relation is really an alternative, less strict, version of subtyping.
    /// Unlike the subtyping relation, the redundancy relation sometimes allows a non-fully-static
    /// type to be considered redundant with another type, and allows some types to be considered
    /// redundant with non-fully-static types.
    ///
    /// For a pair of [fully static] types `A` and `B`, the redundancy relation between `A`
    /// and `B` is the same as the subtyping relation.
    ///
    /// Between a pair of `C` and `D` where either `C` or `D` is not fully static, the
    /// redundancy relation sits in between the subtyping relation and the assignability relation.
    /// `D` can be said to be redundant in a union with `C` if the top materialization of the type
    /// `C | D` is equivalent to the top materialization of `C`, *and* the bottom materialization
    /// of `C | D` is equivalent to the bottom materialization of `C`.
    /// More concisely: `D <: C` iff `Top[C | D] == Top[C]` AND `Bottom[C | D] == Bottom[C]`.
    ///
    /// As stated above, in most respects the redundancy relation is the same as the subtyping
    /// relation. It is redundant to add `bool` to a union that includes `int`, because `bool` is a
    /// subtype of `int`, so inference of attribute access or binary expressions on the union
    /// `int | bool` would always produce a type that represents the same set of possible sets of
    /// runtime values as if ty had inferred the attribute access or binary expression on `int`
    /// alone.
    ///
    /// The redundancy relation is used prominently in two places as of 2026-02-25: for
    /// simplifying unions and intersections in our smart type builders, and for calculating
    /// equivalence between types. Union simplification is pragmatic, and passes `pure: false`;
    /// equivalence checking requires "pure redundancy", and thus passes `pure: true`. Practically,
    /// the behaviour difference here is that we want `Literal[False]` to always be considered
    /// equivalent to `Literal[False]`, but we don't *necessarily* want `Literal[False]` to always
    /// be considered redundant with `Literal[False]` if one `Literal[False]` is promotable and the
    /// other is not.
    ///
    /// In comparing the redundancy relation with subtyping, one practical way in which they differ is
    /// that the redundancy relation permits a number of simplifications that can be made when
    /// simplifying unions that would not be strictly permitted by the subtyping relation. For example,
    /// it is safe to avoid adding `Any` to a union that already includes `Any`, because `Any` already
    /// represents an unknown set of possible sets of runtime values that can materialize to any type in
    /// a gradual, permissive way. Inferring attribute access or binary expressions over
    /// `Any | Any` could never conceivably yield a type that represents a different set of
    /// possible sets of runtime values to inferring the same expression over `Any` alone;
    /// although `Any` is not a subtype of `Any`, top materialization of both `Any` and
    /// `Any | Any` is `object`, and the bottom materialization of both types is `Never`.
    ///
    /// The same principle also applies to intersections that include `Any` being added to
    /// unions that include `Any`: for any type `A`, although naively distributing
    /// type-inference operations over `(Any & A) | Any` could produce types that have
    /// different displays to `Any`, `(Any & A) | Any` nonetheless has the same top
    /// materialization as `Any` and the same bottom materialization as `Any`, and thus it is
    /// redundant to add `Any & A` to a union that already includes `Any`.
    ///
    /// Union simplification cannot use the assignability relation, meanwhile, as it is
    /// trivial to produce examples of cases where adding a type `B` to a union that includes
    /// `A` would impact downstream type inference, even where `B` is assignable to `A`. For
    /// example, `int` is assignable to `Any`, but attribute access over the union `int | Any`
    /// will yield very different results to attribute access over `Any` alone. The top
    /// materialization of `Any` and `int | Any` may be the same type (`object`), but the
    /// two differ in their bottom materializations (`Never` and `int`, respectively).
    ///
    /// Despite the above principles, there is one exceptional type that should never be union-simplified: the `Divergent` type.
    /// This is a kind of dynamic type, but it acts as a marker to track recursive type structures.
    /// If this type is accidentally eliminated by simplification, the fixed-point iteration will not converge.
    ///
    /// [fully static]: https://typing.python.org/en/latest/spec/glossary.html#term-fully-static-type
    /// [materializations]: https://typing.python.org/en/latest/spec/glossary.html#term-materialize
    Redundancy { pure: bool },

    /// The "constraint implication" relationship, aka "implies subtype of".
    ///
    /// This relationship tests whether one type is a [subtype][Self::Subtyping] of another,
    /// assuming that the constraints in a particular constraint set hold.
    ///
    /// For concrete types (types that do not contain typevars), this relationship is the same as
    /// [subtyping][Self::Subtyping]. (Constraint sets place restrictions on typevars, so if you
    /// are not comparing typevars, the constraint set can have no effect on whether subtyping
    /// holds.)
    ///
    /// If you're comparing a typevar, we have to consider what restrictions the constraint set
    /// places on that typevar to determine if subtyping holds. For instance, if you want to check
    /// whether `T ≤ int`, then the answer will depend on what constraint set you are considering:
    ///
    /// ```text
    /// implies_subtype_of(T ≤ bool, T, int) ⇒ true
    /// implies_subtype_of(T ≤ int, T, int)  ⇒ true
    /// implies_subtype_of(T ≤ str, T, int)  ⇒ false
    /// ```
    ///
    /// In the first two cases, the constraint set ensures that `T` will always specialize to a
    /// type that is a subtype of `int`. In the final case, the constraint set requires `T` to
    /// specialize to a subtype of `str`, and there is no such type that is also a subtype of
    /// `int`.
    ///
    /// There are two constraint sets that deserve special consideration.
    ///
    /// - The "always true" constraint set does not place any restrictions on any typevar. In this
    ///   case, `implies_subtype_of` will return the same result as `when_subtype_of`, even if
    ///   you're comparing against a typevar.
    ///
    /// - The "always false" constraint set represents an impossible situation. In this case, every
    ///   subtype check will be vacuously true, even if you're comparing two concrete types that
    ///   are not actually subtypes of each other. (That is, `implies_subtype_of(false, int, str)`
    ///   will return true!)
    SubtypingAssuming,
}

impl TypeRelation {
    pub(crate) const fn is_assignability(self) -> bool {
        matches!(self, TypeRelation::Assignability)
    }

    pub(crate) const fn is_subtyping(self) -> bool {
        matches!(self, TypeRelation::Subtyping)
    }

    const fn can_safely_assume_reflexivity(self, ty: Type<'_>) -> bool {
        match self {
            TypeRelation::Assignability | TypeRelation::Redundancy { .. } => true,
            TypeRelation::Subtyping | TypeRelation::SubtypingAssuming => {
                ty.subtyping_is_always_reflexive()
            }
        }
    }

    pub(super) const fn description(self) -> &'static str {
        match self {
            TypeRelation::Assignability => "assignable to",
            _ => "a subtype of",
        }
    }
}

/// Determines when comparisons involving type variables are evaluated.
#[derive(Debug, Copy, Clone, Hash, PartialEq, Eq)]
pub(crate) enum TypeVarEvaluation {
    /// Check immediately whether the relation holds for all or any valid specializations,
    /// depending on whether the type variable is inferable.
    Eager,

    /// Move comparisons involving a type variable into the constraint set for later evaluation.
    ///
    /// This is currently opt-in, but will eventually replace eager type-variable evaluation.
    Lazy,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::types) enum OwnedRelationKind {
    Assignability,
    Equivalence,
}

pub(in crate::types) struct ConstraintSetRelationFacts;

ty_mapping_probe_macros::shared_semantic_family! {
    #[synchronous(SyncScalarConstraintSetEffects)]
    trait ScalarConstraintSetEffects<'a, 'c, 'db> {
        type Error;
        #[operation(child)]
        async fn pair(
            &self,
            checker: &TypeRelationChecker<'a, 'c, 'db>,
            source: Type<'db>,
            target: Type<'db>,
        ) -> Result<ConstraintSet<'db, 'c>, Self::Error>;
        #[operation(child)]
        async fn always(
            &self,
            checker: &TypeRelationChecker<'a, 'c, 'db>,
            constraints: ConstraintSet<'db, 'c>,
        ) -> Result<bool, Self::Error>;
    }

    #[synchronous(SyncOwnedConstraintSetEffects)]
    pub(in crate::types) trait OwnedConstraintSetEffects<'db> {
        type Error;
        #[operation(child)]
        async fn trivially_assignable(
            &self,
            source: Type<'db>,
            target: Type<'db>,
        ) -> Result<bool, Self::Error>;
        #[operation(local)]
        async fn union_contains(
            &self,
            union: UnionType<'db>,
            source: Type<'db>,
        ) -> Result<bool, Self::Error>;
        #[operation(local)]
        async fn intersection_contains(
            &self,
            intersection: IntersectionType<'db>,
            target: Type<'db>,
        ) -> Result<bool, Self::Error>;
        #[operation(child)]
        async fn cached_owned_assignable(
            &self,
            source: Type<'db>,
            target: Type<'db>,
        ) -> Result<&'db OwnedConstraintSet<'db>, Self::Error>;
    }

    #[synchronous(SyncEquivalenceWrapperEffects)]
    trait EquivalenceWrapperEffects<'db> {
        type Error;
        #[operation(child)]
        async fn cached_owned_equivalent(
            &self,
            source: Type<'db>,
            target: Type<'db>,
        ) -> Result<&'db OwnedConstraintSet<'db>, Self::Error>;
        #[operation(child)]
        async fn owned_always(&self, value: &'db OwnedConstraintSet<'db>) -> Result<bool, Self::Error>;
    }

    #[synchronous(SyncOwnedRelationProducerEffects)]
    trait OwnedRelationProducerEffects<'c, 'db: 'c> {
        type Error;
        #[operation(child)]
        async fn assignable(&self, source: Type<'db>, target: Type<'db>) -> Result<ConstraintSet<'db, 'c>, Self::Error>;
        #[operation(child)]
        async fn equivalent(&self, source: Type<'db>, target: Type<'db>) -> Result<ConstraintSet<'db, 'c>, Self::Error>;
    }

    #[synchronous(SyncDirectionalEquivalenceEffects)]
    trait DirectionalEquivalenceEffects<'a, 'c, 'db> {
        type Error;
        #[operation(child)]
        async fn direction(&self, checker: &EquivalenceChecker<'a, 'c, 'db>, source: Type<'db>, target: Type<'db>) -> Result<ConstraintSet<'db, 'c>, Self::Error>;
        #[operation(local)]
        async fn is_never(&self, checker: &EquivalenceChecker<'a, 'c, 'db>, value: ConstraintSet<'db, 'c>) -> Result<bool, Self::Error>;
        #[operation(child)]
        async fn conjoin(&self, checker: &EquivalenceChecker<'a, 'c, 'db>, left: ConstraintSet<'db, 'c>, right: ConstraintSet<'db, 'c>) -> Result<ConstraintSet<'db, 'c>, Self::Error>;
    }

    #[synchronous(owned_relation_constraints_sync)]
    #[capabilities(effects = OwnedRelationProducerEffects)]
    #[passive_values()]
    async fn owned_relation_constraints_with<'c, 'db: 'c, E: OwnedRelationProducerEffects<'c, 'db>>(
        kind: OwnedRelationKind,
        source: Type<'db>,
        target: Type<'db>,
        effects: &E,
    ) -> Result<ConstraintSet<'db, 'c>, E::Error> {
        match kind {
            OwnedRelationKind::Assignability => effects.assignable(source, target).await,
            OwnedRelationKind::Equivalence => effects.equivalent(source, target).await,
        }
    }

    #[synchronous(directional_equivalence_sync)]
    #[capabilities(effects = DirectionalEquivalenceEffects)]
    #[passive_values()]
    async fn directional_equivalence_with<'a, 'c, 'db, E: DirectionalEquivalenceEffects<'a, 'c, 'db>>(
        checker: &EquivalenceChecker<'a, 'c, 'db>,
        source: Type<'db>,
        target: Type<'db>,
        effects: &E,
    ) -> Result<ConstraintSet<'db, 'c>, E::Error> {
        let forward = effects.direction(checker, source, target).await?;
        if effects.is_never(checker, forward).await? {
            return Ok(forward);
        }
        let reverse = effects.direction(checker, target, source).await?;
        effects.conjoin(checker, forward, reverse).await
    }

    #[synchronous(constraint_set_equivalent_owned_sync)]
    #[capabilities(effects = EquivalenceWrapperEffects, facts = ConstraintSetRelationFacts)]
    #[passive_values(Cow::Owned, Cow::Borrowed)]
    async fn constraint_set_equivalent_owned_with<'db, E: EquivalenceWrapperEffects<'db>>(
        source: Type<'db>, target: Type<'db>, effects: &E, facts: ConstraintSetRelationFacts,
    ) -> Result<Cow<'db, OwnedConstraintSet<'db>>, E::Error> {
        if facts.equal(source, target) {
            return Ok(Cow::Owned(facts.always()));
        }
        Ok(Cow::Borrowed(effects.cached_owned_equivalent(source, target).await?))
    }

    #[synchronous(constraint_set_equivalent_sync)]
    #[capabilities(effects = EquivalenceWrapperEffects, facts = ConstraintSetRelationFacts)]
    #[passive_values()]
    async fn constraint_set_equivalent_with<'db, E: EquivalenceWrapperEffects<'db>>(
        source: Type<'db>, target: Type<'db>, effects: &E, facts: ConstraintSetRelationFacts,
    ) -> Result<bool, E::Error> {
        if facts.equal(source, target) {
            return Ok(true);
        }
        let owned = effects.cached_owned_equivalent(source, target).await?;
        effects.owned_always(owned).await
    }

    #[finite_capability]
    impl ConstraintSetRelationFacts {
        fn nondivergent(&self, ty: Type<'_>) -> bool {
            ty.materialized_divergent_fallback().is_none()
        }
        fn equal<'db>(&self, source: Type<'db>, target: Type<'db>) -> bool {
            source == target
        }
        fn is_typevar(&self, ty: Type<'_>) -> bool {
            ty.is_type_var()
        }
        fn is_object(&self, ty: crate::types::NominalInstanceType<'_>) -> bool {
            ty.is_object()
        }
        fn always<'db>(&self) -> OwnedConstraintSet<'db> {
            // An owned terminal stores only its node, no source order and no backing storage.
            OwnedConstraintSet::always()
        }
    }

    #[synchronous(constraint_set_assignable_sync)]
    #[capabilities(effects = ScalarConstraintSetEffects)]
    #[passive_values()]
    async fn constraint_set_assignable_with<'a, 'c, 'db, E: ScalarConstraintSetEffects<'a, 'c, 'db>>(
        checker: &TypeRelationChecker<'a, 'c, 'db>,
        source: Type<'db>,
        target: Type<'db>,
        effects: &E,
    ) -> Result<bool, E::Error> {
        let constraints = effects.pair(checker, source, target).await?;
        effects.always(checker, constraints).await
    }

    #[synchronous(trivially_constraint_set_assignable_sync)]
    #[capabilities(effects = OwnedConstraintSetEffects, facts = ConstraintSetRelationFacts)]
    #[passive_values()]
    pub(in crate::types) async fn trivially_constraint_set_assignable_with<'db, E: OwnedConstraintSetEffects<'db>>(
        source: Type<'db>,
        target: Type<'db>,
        effects: &E,
        facts: ConstraintSetRelationFacts,
    ) -> Result<bool, E::Error> {
        if facts.nondivergent(source) && facts.equal(source, target) {
            return Ok(true);
        }

        // Type variables must be converted into constraints before applying the remaining
        // relation shortcuts.
        if facts.is_typevar(source) || facts.is_typevar(target) {
            return Ok(false);
        }

        Ok(match (source, target) {
            (Type::Never | Type::Dynamic(_), _) | (_, Type::Dynamic(_)) => true,
            (_, Type::NominalInstance(target)) if facts.is_object(target) => true,
            (_, Type::Union(union)) => {
                facts.nondivergent(source) && effects.union_contains(union, source).await?
            }
            (Type::Intersection(intersection), _) => {
                facts.nondivergent(target) && effects.intersection_contains(intersection, target).await?
            }
            _ => false,
        })
    }

    #[synchronous(constraint_set_assignable_owned_sync)]
    #[capabilities(effects = OwnedConstraintSetEffects, facts = ConstraintSetRelationFacts)]
    #[passive_values(Cow::Owned, Cow::Borrowed)]
    pub(in crate::types) async fn constraint_set_assignable_owned_with<'db, E: OwnedConstraintSetEffects<'db>>(
        source: Type<'db>,
        target: Type<'db>,
        effects: &E,
        facts: ConstraintSetRelationFacts,
    ) -> Result<Cow<'db, OwnedConstraintSet<'db>>, E::Error> {
        if effects.trivially_assignable(source, target).await? {
            return Ok(Cow::Owned(facts.always()));
        }
        Ok(Cow::Borrowed(effects.cached_owned_assignable(source, target).await?))
    }
}

struct OrdinaryConstraintSetRelations<'a, 'db> {
    db: &'db dyn Db,
    env: &'a ProgramEnvironment<'db>,
}

impl<'a, 'c, 'db> SyncScalarConstraintSetEffects<'a, 'c, 'db>
    for OrdinaryConstraintSetRelations<'_, 'db>
{
    type Error = Infallible;

    fn pair(
        &self,
        checker: &TypeRelationChecker<'a, 'c, 'db>,
        source: Type<'db>,
        target: Type<'db>,
    ) -> Result<ConstraintSet<'db, 'c>, Infallible> {
        Ok(checker.check_type_pair(self.db, source, target))
    }

    fn always(
        &self,
        checker: &TypeRelationChecker<'a, 'c, 'db>,
        constraints: ConstraintSet<'db, 'c>,
    ) -> Result<bool, Infallible> {
        Ok(constraints.is_always_satisfied(self.db, checker.env))
    }
}

impl<'db> SyncOwnedConstraintSetEffects<'db> for OrdinaryConstraintSetRelations<'_, 'db> {
    type Error = Infallible;

    fn trivially_assignable(
        &self,
        source: Type<'db>,
        target: Type<'db>,
    ) -> Result<bool, Infallible> {
        Ok(source.is_trivially_constraint_set_assignable_to(self.db, self.env, target))
    }

    fn union_contains(&self, union: UnionType<'db>, source: Type<'db>) -> Result<bool, Infallible> {
        Ok(union.elements(self.db).contains(&source))
    }

    fn intersection_contains(
        &self,
        intersection: IntersectionType<'db>,
        target: Type<'db>,
    ) -> Result<bool, Infallible> {
        Ok(intersection.positive(self.db).contains(&target))
    }

    fn cached_owned_assignable(
        &self,
        source: Type<'db>,
        target: Type<'db>,
    ) -> Result<&'db OwnedConstraintSet<'db>, Infallible> {
        Ok(when_constraint_set_assignable_to_owned_impl(
            self.db,
            TypePair::new(self.db, self.env.program(self.db), source, target),
        ))
    }
}

impl<'db> SyncEquivalenceWrapperEffects<'db> for OrdinaryConstraintSetRelations<'_, 'db> {
    type Error = Infallible;

    fn cached_owned_equivalent(
        &self,
        source: Type<'db>,
        target: Type<'db>,
    ) -> Result<&'db OwnedConstraintSet<'db>, Infallible> {
        Ok(when_constraint_set_equivalent_to_impl(
            self.db,
            TypePair::new(self.db, self.env.program(self.db), source, target),
        ))
    }

    fn owned_always(&self, value: &'db OwnedConstraintSet<'db>) -> Result<bool, Infallible> {
        Ok(value.query(|_constraints, when| when.is_always_satisfied(self.db, self.env)))
    }
}

struct OrdinaryOwnedRelationProducer<'a, 'c, 'db> {
    db: &'db dyn Db,
    env: &'a ProgramEnvironment<'db>,
    constraints: &'c ConstraintSetBuilder<'db>,
}

impl<'c, 'db> SyncOwnedRelationProducerEffects<'c, 'db>
    for OrdinaryOwnedRelationProducer<'_, 'c, 'db>
{
    type Error = Infallible;

    fn assignable(
        &self,
        source: Type<'db>,
        target: Type<'db>,
    ) -> Result<ConstraintSet<'db, 'c>, Infallible> {
        Ok(source.has_relation_to_with_typevar_evaluation(
            self.db,
            self.env,
            target,
            self.constraints,
            TypeVarSet::None,
            TypeRelation::Assignability,
            TypeVarEvaluation::Lazy,
        ))
    }

    fn equivalent(
        &self,
        source: Type<'db>,
        target: Type<'db>,
    ) -> Result<ConstraintSet<'db, 'c>, Infallible> {
        let materialization_visitor = ApplyTypeMappingVisitor::new(self.env);
        Ok(source.when_equivalent_to_with_materialization_visitor(
            self.db,
            target,
            self.constraints,
            &materialization_visitor,
            TypeVarEvaluation::Lazy,
        ))
    }
}

fn owned_relation_impl<'db>(
    db: &'db dyn Db,
    types: TypePair<'db>,
    kind: OwnedRelationKind,
) -> OwnedConstraintSet<'db> {
    let env = ProgramEnvironment::from_program(types.program(db));
    let constraints = ConstraintSetBuilder::new();
    constraints.into_owned(|constraints| {
        owned_relation_constraints_sync(
            kind,
            types.first(db),
            types.second(db),
            &OrdinaryOwnedRelationProducer {
                db,
                env: &env,
                constraints,
            },
        )
        .unwrap_or_else(|never| match never {})
    })
}

struct OrdinaryDirectionalEquivalence<'db> {
    db: &'db dyn Db,
}

impl<'a, 'c, 'db> SyncDirectionalEquivalenceEffects<'a, 'c, 'db>
    for OrdinaryDirectionalEquivalence<'db>
{
    type Error = Infallible;

    fn direction(
        &self,
        checker: &EquivalenceChecker<'a, 'c, 'db>,
        source: Type<'db>,
        target: Type<'db>,
    ) -> Result<ConstraintSet<'db, 'c>, Infallible> {
        // Recursive materialization fallbacks depend on the comparison root, so each directional
        // pass needs fresh materialization caches. Nested equivalence checks still share the
        // materialization-equivalence recursion guard to avoid re-entering the same comparison.
        let visitor = checker
            .materialization_visitor
            .for_new_materialization_root();
        Ok(checker
            .as_relation_checker(&visitor)
            .check_type_pair(self.db, source, target))
    }

    fn is_never(
        &self,
        checker: &EquivalenceChecker<'a, 'c, 'db>,
        value: ConstraintSet<'db, 'c>,
    ) -> Result<bool, Infallible> {
        value.verify_builder(checker.constraints);
        Ok(value.is_trivially_never_satisfied())
    }

    fn conjoin(
        &self,
        checker: &EquivalenceChecker<'a, 'c, 'db>,
        left: ConstraintSet<'db, 'c>,
        right: ConstraintSet<'db, 'c>,
    ) -> Result<ConstraintSet<'db, 'c>, Infallible> {
        Ok(left.and(self.db, checker.constraints, || right))
    }
}

#[salsa::tracked]
impl<'db> Type<'db> {
    /// Return `true` if subtyping is always reflexive for this type; `T <: T` is always true for
    /// any `T` of this type.
    ///
    /// This is true for fully static types, but also for some types that may not be fully static.
    /// For example, a `ClassLiteral` may inherit `Any`, but its subtyping is still reflexive.
    ///
    /// This method may have false negatives, but it should not have false positives. It should be
    /// a cheap shallow check, not an exhaustive recursive check.
    const fn subtyping_is_always_reflexive(self) -> bool {
        match self {
            Type::RecursiveVar(_) => panic!("semantic operation on an unbound recursive variable"),
            Type::Never
            | Type::FunctionLiteral(..)
            | Type::WrapperDescriptor(_)
            | Type::KnownBoundMethod(
                KnownBoundMethodType::StrStartswith(_)
                | KnownBoundMethodType::ConstraintSetLowerBound
                | KnownBoundMethodType::ConstraintSetUpperBound
                | KnownBoundMethodType::ConstraintSetEquality
                | KnownBoundMethodType::ConstraintSetRange
                | KnownBoundMethodType::ConstraintSetAlways
                | KnownBoundMethodType::ConstraintSetNever
                | KnownBoundMethodType::ConstraintSetImpliesSubtypeOf(_)
                | KnownBoundMethodType::ConstraintSetSatisfies(_)
                | KnownBoundMethodType::ConstraintSetExists(_)
                | KnownBoundMethodType::ConstraintSetForAll(_)
                | KnownBoundMethodType::ConstraintSetSolutionsFor(_)
                | KnownBoundMethodType::ConstraintSetSolutions(_)
                | KnownBoundMethodType::ConstraintSetWithDetailedDisplay(_),
            )
            | Type::DataclassDecorator(_)
            | Type::DataclassTransformer(_)
            | Type::ModuleLiteral(..)
            | Type::LiteralValue(_)
            | Type::SpecialForm(_)
            | Type::KnownInstance(_)
            | Type::AlwaysFalsy
            | Type::AlwaysTruthy => true,

            // `T` is always a subtype of itself,
            // and `T` is always a subtype of `T | None`
            Type::TypeVar(_) => true,

            // might inherit `Any`, but subtyping is still reflexive
            Type::ClassLiteral(_) => true,

            Type::BoundMethod(_)
            | Type::Dynamic(_)
            | Type::Divergent(_)
            | Type::Recursive(_)
            | Type::NominalInstance(_)
            | Type::ProtocolInstance(_)
            | Type::GenericAlias(_)
            | Type::SubclassOf(_)
            | Type::Union(_)
            | Type::Intersection(_)
            | Type::EnumComplement(_)
            | Type::Callable(_)
            | Type::KnownBoundMethod(
                KnownBoundMethodType::MethodTypeDunderGet(_)
                | KnownBoundMethodType::DunderCall(_)
                | KnownBoundMethodType::FunctionTypeDunderGet(_)
                | KnownBoundMethodType::PropertyDunderGet(_)
                | KnownBoundMethodType::PropertyDunderSet(_)
                | KnownBoundMethodType::PropertyDunderDelete(_),
            )
            | Type::PropertyInstance(_)
            | Type::SlotDescriptor(_)
            | Type::BoundSuper(_)
            | Type::TypeIs(_)
            | Type::TypeGuard(_)
            | Type::TypeForm(_)
            | Type::TypedDict(_)
            | Type::TypeAlias(_)
            | Type::NewTypeInstance(_) => false,
        }
    }

    /// Return true if this type is a subtype of type `target`.
    ///
    /// See [`TypeRelation::Subtyping`] for more details.
    pub(crate) fn is_subtype_of(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        target: Type<'db>,
    ) -> bool {
        let constraints = ConstraintSetBuilder::new();
        self.when_subtype_of(db, env, target, &constraints, TypeVarSet::None)
            .is_always_satisfied(db, env)
    }

    pub(super) fn when_subtype_of<'c>(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        target: Type<'db>,
        constraints: &'c ConstraintSetBuilder<'db>,
        inferable: TypeVarSet<'db>,
    ) -> ConstraintSet<'db, 'c> {
        self.has_relation_to(
            db,
            env,
            target,
            constraints,
            inferable,
            TypeRelation::Subtyping,
        )
    }

    /// Return the constraints under which this type is a subtype of type `target`, assuming that
    /// all of the restrictions in `constraints` hold.
    ///
    /// See [`TypeRelation::SubtypingAssuming`] for more details.
    pub(super) fn when_subtype_of_assuming<'c>(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        target: Type<'db>,
        assuming: ConstraintSet<'db, 'c>,
        constraints: &'c ConstraintSetBuilder<'db>,
        inferable: TypeVarSet<'db>,
    ) -> ConstraintSet<'db, 'c> {
        let relation_visitor = HasRelationToVisitor::default(constraints);
        let disjointness_visitor = IsDisjointVisitor::default(constraints);
        let signature_relation_visitor = SignatureRelationVisitor::default();
        let materialization_visitor = ApplyTypeMappingVisitor::new(env);
        let checker = TypeRelationChecker {
            env,
            constraints,
            inferable,
            relation: TypeRelation::SubtypingAssuming,
            typevar_evaluation: TypeVarEvaluation::Eager,
            context_tree: None,
            observations: None,
            given: assuming,
            perform_expensive_checks: true,
            relation_visitor: &relation_visitor,
            disjointness_visitor: &disjointness_visitor,
            signature_relation_visitor: &signature_relation_visitor,
            materialization_visitor: &materialization_visitor,
        };
        checker.check_type_pair(db, self, target)
    }

    /// Return true if this type is assignable to type `target`.
    ///
    /// See `TypeRelation::Assignability` for more details.
    pub fn is_assignable_to(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        target: Type<'db>,
    ) -> bool {
        let constraints = ConstraintSetBuilder::new();
        self.when_assignable_to(db, env, target, &constraints, TypeVarSet::None)
            .is_always_satisfied(db, env)
    }

    /// Records comparisons for one signature, including relationships between types constraining
    /// the same inferable variable. Constructor expansion uses these observations to retain
    /// progress towards descriptor overloads without implementing its own type relations.
    pub(super) fn assignability_observations(
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        comparisons: impl IntoIterator<Item = (usize, Type<'db>, Type<'db>)>,
        patterns: &FxHashSet<Type<'db>>,
        inferable: TypeVarSet<'db>,
    ) -> FxHashSet<RelationObservation<'db>> {
        let constraints = ConstraintSetBuilder::new();
        let relation_visitor = HasRelationToVisitor::default(&constraints);
        let disjointness_visitor = IsDisjointVisitor::default(&constraints);
        let signature_visitor = SignatureRelationVisitor::default();
        let mapping_visitor = ApplyTypeMappingVisitor::new(env);
        let observations = RelationObservations {
            patterns,
            results: RefCell::default(),
            site: Cell::new(RelationObservationSite::Argument(0)),
            inferred: RefCell::default(),
        };
        let checker = TypeRelationChecker {
            observations: Some(&observations),
            ..TypeRelationChecker::new(
                env,
                TypeRelation::Assignability,
                &constraints,
                inferable,
                &relation_visitor,
                &disjointness_visitor,
                &signature_visitor,
                &mapping_visitor,
            )
        };
        for (index, source, target) in comparisons {
            observations
                .site
                .set(RelationObservationSite::Argument(index));
            checker.check_type_pair(db, source, target);
        }
        // A repeated type variable relates its occurrences even when comparing each occurrence
        // to the variable itself reveals no structure. Observe those relationships using the same
        // checker; this does not choose an inferred type or decide whether the overload matches.
        for (typevar, candidates) in observations.inferred.take() {
            observations
                .site
                .set(RelationObservationSite::TypeVar(typevar));
            for (index, &left) in candidates.iter().enumerate() {
                for &right in candidates.iter().skip(index + 1) {
                    checker.check_type_pair(db, left, right);
                    checker.check_type_pair(db, right, left);
                }
            }
        }
        observations.results.into_inner()
    }

    /// Re-run the assignability check with error context collection enabled.
    ///
    /// This should normally be called when `is_assignable_to` has returned `false` and we
    /// are now about to emit a diagnostic where additional context could be useful.
    ///
    /// This is a separate method so that we can skip this expensive check when diagnostics
    /// are suppressed.
    pub(crate) fn assignability_error_context(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        target: Type<'db>,
    ) -> ErrorContextTree<'db> {
        self.relation_error_context(db, env, TypeRelation::Assignability, target)
    }

    /// Re-run the pure redundancy check with error context collection enabled.
    ///
    /// This should normally be called when `is_pure_redundant_with` has returned `false`
    /// and we are now about to emit a diagnostic where additional context could be
    /// useful.
    ///
    /// This is a separate method so that we can skip this expensive check when diagnostics
    /// are suppressed.
    pub(crate) fn pure_redundancy_error_context(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        target: Type<'db>,
    ) -> ErrorContextTree<'db> {
        self.relation_error_context(db, env, TypeRelation::Redundancy { pure: true }, target)
    }

    /// Re-run the relation check with error context collection enabled.
    ///
    /// This is a separate method so that we can skip this expensive check when diagnostics
    /// are suppressed.
    fn relation_error_context(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        relation: TypeRelation,
        target: Type<'db>,
    ) -> ErrorContextTree<'db> {
        let builder = ConstraintSetBuilder::new();
        let checker = TypeRelationChecker {
            env,
            constraints: &builder,
            inferable: TypeVarSet::None,
            relation,
            typevar_evaluation: TypeVarEvaluation::Eager,
            context_tree: Some(ErrorContextTree::new(relation)),
            observations: None,
            given: ConstraintSet::from_bool(&builder, false),
            perform_expensive_checks: true,
            relation_visitor: &HasRelationToVisitor::default(&builder),
            disjointness_visitor: &IsDisjointVisitor::default(&builder),
            signature_relation_visitor: &SignatureRelationVisitor::default(),
            materialization_visitor: &ApplyTypeMappingVisitor::new(env),
        };
        checker.check_type_pair(db, self, target);
        checker.into_error_context()
    }

    /// Return true if this type is assignable to type `target` using constraint-set typevar rules.
    pub(crate) fn is_constraint_set_assignable_to(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        target: Type<'db>,
    ) -> bool {
        let constraints = ConstraintSetBuilder::new();
        let owners = RelationOwners::new(env, &constraints);
        constraint_set_assignable_sync(
            &owners.constraint_set_assignability(),
            self,
            target,
            &OrdinaryConstraintSetRelations { db, env },
        )
        .unwrap_or_else(|never| match never {})
    }

    pub(super) fn when_assignable_to<'c>(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        target: Type<'db>,
        constraints: &'c ConstraintSetBuilder<'db>,
        inferable: TypeVarSet<'db>,
    ) -> ConstraintSet<'db, 'c> {
        self.has_relation_to(
            db,
            env,
            target,
            constraints,
            inferable,
            TypeRelation::Assignability,
        )
    }

    /// Returns whether constraint-set assignability is known to be unconditionally satisfied
    /// before constructing the relation checker.
    fn is_trivially_constraint_set_assignable_to(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        target: Type<'db>,
    ) -> bool {
        trivially_constraint_set_assignable_sync(
            self,
            target,
            &OrdinaryConstraintSetRelations { db, env },
            ConstraintSetRelationFacts,
        )
        .unwrap_or_else(|never| match never {})
    }

    /// Returns an _owned_ (i.e. salsa-cached) constraint set that describes when `self` is
    /// constraint-set assignable to `target`.
    ///
    /// Recursive relations are evaluated coinductively: a cycle is provisionally satisfied until
    /// another part of the relation produces a contradiction.
    pub(super) fn when_constraint_set_assignable_to_owned(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        target: Type<'db>,
    ) -> Cow<'db, OwnedConstraintSet<'db>> {
        constraint_set_assignable_owned_sync(
            self,
            target,
            &OrdinaryConstraintSetRelations { db, env },
            ConstraintSetRelationFacts,
        )
        .unwrap_or_else(|never| match never {})
    }

    pub(super) fn when_constraint_set_assignable_to<'c>(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        target: Type<'db>,
        constraints: &'c ConstraintSetBuilder<'db>,
    ) -> ConstraintSet<'db, 'c> {
        self.has_relation_to_with_typevar_evaluation(
            db,
            env,
            target,
            constraints,
            TypeVarSet::None,
            TypeRelation::Assignability,
            TypeVarEvaluation::Lazy,
        )
    }

    /// Return `true` if it would be redundant to add `self` to a union that already contains `other`.
    ///
    /// See [`TypeRelation::Redundancy`] for more details.
    pub(super) fn is_redundant_with(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        other: Type<'db>,
    ) -> bool {
        redundancy::compare_sync(
            self,
            other,
            &redundancy::OrdinaryRedundancyComparison { db, env },
        )
        .unwrap_or_else(|never| match never {})
    }

    /// Return `true` if `self` is redundant with `other` under the pure redundancy relation.
    ///
    /// Unlike [`Self::is_redundant_with`], this does not apply shortcuts intended for simplifying
    /// unions.
    pub(super) fn is_pure_redundant_with(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        other: Type<'db>,
    ) -> bool {
        if self == other {
            return true;
        }

        let program = env.program(db);
        let env = ProgramEnvironment::from_program(program);
        self.has_relation_to(
            db,
            &env,
            other,
            &ConstraintSetBuilder::new(),
            TypeVarSet::None,
            TypeRelation::Redundancy { pure: true },
        )
        .is_always_satisfied(db, &env)
    }

    pub(super) fn has_relation_to<'c>(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        target: Type<'db>,
        constraints: &'c ConstraintSetBuilder<'db>,
        inferable: TypeVarSet<'db>,
        relation: TypeRelation,
    ) -> ConstraintSet<'db, 'c> {
        self.has_relation_to_with_typevar_evaluation(
            db,
            env,
            target,
            constraints,
            inferable,
            relation,
            TypeVarEvaluation::Eager,
        )
    }

    #[expect(clippy::too_many_arguments)]
    fn has_relation_to_with_typevar_evaluation<'c>(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        target: Type<'db>,
        constraints: &'c ConstraintSetBuilder<'db>,
        inferable: TypeVarSet<'db>,
        relation: TypeRelation,
        typevar_evaluation: TypeVarEvaluation,
    ) -> ConstraintSet<'db, 'c> {
        let relation_visitor = HasRelationToVisitor::default(constraints);
        let disjointness_visitor = IsDisjointVisitor::default(constraints);
        let signature_relation_visitor = SignatureRelationVisitor::default();
        let materialization_visitor = ApplyTypeMappingVisitor::new(env);
        let checker = TypeRelationChecker {
            env,
            constraints,
            inferable,
            relation,
            typevar_evaluation,
            context_tree: None,
            observations: None,
            given: ConstraintSet::from_bool(constraints, false),
            perform_expensive_checks: true,
            relation_visitor: &relation_visitor,
            disjointness_visitor: &disjointness_visitor,
            signature_relation_visitor: &signature_relation_visitor,
            materialization_visitor: &materialization_visitor,
        };
        checker.check_type_pair(db, self, target)
    }

    /// Return true if this type is [equivalent to] type `other`.
    ///
    /// Two equivalent types represent the same sets of values.
    ///
    /// > Two gradual types `A` and `B` are equivalent
    /// > (that is, the same gradual type, not merely consistent with one another)
    /// > if and only if all materializations of `A` are also materializations of `B`,
    /// > and all materializations of `B` are also materializations of `A`.
    /// >
    /// > &mdash; [Summary of type relations]
    ///
    /// [equivalent to]: https://typing.python.org/en/latest/spec/glossary.html#term-equivalent
    pub(crate) fn is_equivalent_to(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        other: Type<'db>,
    ) -> bool {
        self.when_equivalent_to(db, env, other, &ConstraintSetBuilder::new())
            .is_always_satisfied(db, env)
    }

    pub(crate) fn is_equivalent_to_with_materialization_visitor(
        self,
        db: &'db dyn Db,
        other: Type<'db>,
        materialization_visitor: &ApplyTypeMappingVisitor<'_, 'db>,
    ) -> bool {
        self.when_equivalent_to_with_materialization_visitor(
            db,
            other,
            &ConstraintSetBuilder::new(),
            materialization_visitor,
            TypeVarEvaluation::Eager,
        )
        .is_always_satisfied(db, materialization_visitor.env)
    }

    pub(crate) fn when_equivalent_to<'c>(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        other: Type<'db>,
        constraints: &'c ConstraintSetBuilder<'db>,
    ) -> ConstraintSet<'db, 'c> {
        let materialization_visitor = ApplyTypeMappingVisitor::new(env);
        self.when_equivalent_to_with_materialization_visitor(
            db,
            other,
            constraints,
            &materialization_visitor,
            TypeVarEvaluation::Eager,
        )
    }

    pub(super) fn when_constraint_set_equivalent_to_owned(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        other: Type<'db>,
    ) -> Cow<'db, OwnedConstraintSet<'db>> {
        constraint_set_equivalent_owned_sync(
            self,
            other,
            &OrdinaryConstraintSetRelations { db, env },
            ConstraintSetRelationFacts,
        )
        .unwrap_or_else(|never| match never {})
    }

    pub(super) fn is_constraint_set_equivalent_to(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        other: Type<'db>,
    ) -> bool {
        constraint_set_equivalent_sync(
            self,
            other,
            &OrdinaryConstraintSetRelations { db, env },
            ConstraintSetRelationFacts,
        )
        .unwrap_or_else(|never| match never {})
    }

    fn when_equivalent_to_with_materialization_visitor<'c>(
        self,
        db: &'db dyn Db,
        other: Type<'db>,
        constraints: &'c ConstraintSetBuilder<'db>,
        materialization_visitor: &ApplyTypeMappingVisitor<'_, 'db>,
        typevar_evaluation: TypeVarEvaluation,
    ) -> ConstraintSet<'db, 'c> {
        let relation_visitor = HasRelationToVisitor::default(constraints);
        let disjointness_visitor = IsDisjointVisitor::default(constraints);
        let signature_relation_visitor = SignatureRelationVisitor::default();
        let checker = EquivalenceChecker {
            observations: None,
            env: materialization_visitor.env,
            constraints,
            given: ConstraintSet::from_bool(constraints, false),
            perform_expensive_checks: true,
            typevar_evaluation,
            relation_visitor: &relation_visitor,
            disjointness_visitor: &disjointness_visitor,
            signature_relation_visitor: &signature_relation_visitor,
            materialization_visitor,
        };
        checker.check_type_pair(db, self, other)
    }

    /// Return true if `self & other` should simplify to `Never`:
    /// if the intersection of the two types could never be inhabited by any
    /// possible runtime value.
    ///
    /// Our implementation of disjointness for non-fully-static types only
    /// returns true if the *top materialization* of `self` has no overlap with
    /// the *top materialization* of `other`.
    ///
    /// For example, `list[int]` is disjoint from `list[str]`: the two types have
    /// no overlap. But `list[Any]` is not disjoint from `list[str]`: there exists
    /// a fully static materialization of `list[Any]` (`list[str]`) that is a
    /// subtype of `list[str]`
    ///
    /// This function aims to have no false positives, but might return wrong
    /// `false` answers in some cases.
    pub(crate) fn is_disjoint_from(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        other: Type<'db>,
    ) -> bool {
        let constraints = ConstraintSetBuilder::new();
        self.when_disjoint_from(db, env, other, &constraints, TypeVarSet::None)
            .is_always_satisfied(db, env)
    }

    pub(crate) fn when_disjoint_from<'c>(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        other: Type<'db>,
        constraints: &'c ConstraintSetBuilder<'db>,
        inferable: TypeVarSet<'db>,
    ) -> ConstraintSet<'db, 'c> {
        let relation_visitor = HasRelationToVisitor::default(constraints);
        let disjointness_visitor = IsDisjointVisitor::default(constraints);
        let signature_relation_visitor = SignatureRelationVisitor::default();
        let materialization_visitor = ApplyTypeMappingVisitor::new(env);
        let checker = DisjointnessChecker {
            env,
            constraints,
            inferable,
            context_tree: None,
            observations: None,
            given: ConstraintSet::from_bool(constraints, false),
            perform_expensive_checks: true,
            disjointness_visitor: &disjointness_visitor,
            relation_visitor: &relation_visitor,
            signature_relation_visitor: &signature_relation_visitor,
            materialization_visitor: &materialization_visitor,
        };
        checker.check_type_pair(db, self, other)
    }

    /// Re-run a successful disjointness check with diagnostic context collection enabled.
    pub(crate) fn disjointness_error_context(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        other: Type<'db>,
    ) -> ErrorContextTree<'db> {
        let constraints = ConstraintSetBuilder::new();
        let context = ErrorContextTree::new(ErrorRelation::Disjointness);
        let checker = DisjointnessChecker {
            env,
            constraints: &constraints,
            inferable: TypeVarSet::None,
            context_tree: Some(context.clone()),
            observations: None,
            given: ConstraintSet::from_bool(&constraints, false),
            perform_expensive_checks: true,
            relation_visitor: &HasRelationToVisitor::default(&constraints),
            disjointness_visitor: &IsDisjointVisitor::default(&constraints),
            signature_relation_visitor: &SignatureRelationVisitor::default(),
            materialization_visitor: &ApplyTypeMappingVisitor::new(env),
        };
        checker.check_type_pair(db, self, other);
        context
    }

    /// Checks whether `self` is disjoint from `other`, while being more accepting of false
    /// negatives. Use this when you want to _quickly_ check whether two types are _definitely_
    /// disjoint, typically for engaging a fast path in some algorithm.
    pub(crate) fn when_trivially_disjoint_from<'c>(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        other: Type<'db>,
        constraints: &'c ConstraintSetBuilder<'db>,
        inferable: TypeVarSet<'db>,
    ) -> ConstraintSet<'db, 'c> {
        let relation_visitor = HasRelationToVisitor::default(constraints);
        let disjointness_visitor = IsDisjointVisitor::default(constraints);
        let signature_relation_visitor = SignatureRelationVisitor::default();
        let materialization_visitor = ApplyTypeMappingVisitor::new(env);
        let checker = DisjointnessChecker {
            env,
            constraints,
            inferable,
            context_tree: None,
            observations: None,
            given: ConstraintSet::from_bool(constraints, false),
            perform_expensive_checks: false,
            disjointness_visitor: &disjointness_visitor,
            relation_visitor: &relation_visitor,
            signature_relation_visitor: &signature_relation_visitor,
            materialization_visitor: &materialization_visitor,
        };
        checker.check_type_pair(db, self, other)
    }
}

/// A [`CycleDetector`] that is used in `has_relation_to` methods.
pub(crate) type HasRelationToVisitor<'db, 'c> = CycleDetector<
    'db,
    TypeRelation,
    (Type<'db>, Type<'db>, TypeRelation, TypeVarEvaluation),
    ConstraintSet<'db, 'c>,
    1,
>;

impl<'db> HasIdentity<'db> for (Type<'db>, Type<'db>, TypeRelation, TypeVarEvaluation) {
    type Id = (
        TypeIdentity<'db>,
        TypeIdentity<'db>,
        TypeRelation,
        TypeVarEvaluation,
    );

    fn may_share_identity(&self, db: &'db dyn Db, other: &Self) -> bool {
        self.0.may_share_type_identity(db, other.0)
            && self.1.may_share_type_identity(db, other.1)
            && self.2 == other.2
            && self.3 == other.3
    }

    fn to_identity(&self, db: &'db dyn Db) -> Self::Id {
        (
            self.0.to_type_identity(db),
            self.1.to_type_identity(db),
            self.2,
            self.3,
        )
    }
}

impl<'db, 'c> HasRelationToVisitor<'db, 'c> {
    pub(crate) fn default(constraints: &'c ConstraintSetBuilder<'db>) -> Self {
        HasRelationToVisitor::new(ConstraintSet::from_bool(constraints, true))
    }
}

/// A [`PairVisitor`] that is used in `is_disjoint_from` methods.
pub(crate) type IsDisjointVisitor<'db, 'c> = PairVisitor<'db, IsDisjoint, ConstraintSet<'db, 'c>>;

#[derive(Debug)]
pub(crate) struct IsDisjoint;

impl<'db, 'c> IsDisjointVisitor<'db, 'c> {
    pub(crate) fn default(constraints: &'c ConstraintSetBuilder<'db>) -> Self {
        IsDisjointVisitor::new(ConstraintSet::from_bool(constraints, false))
    }
}

/// A finite observation of a comparison against part of a declared annotation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, get_size2::GetSize, salsa::SalsaValue)]
pub(super) struct RelationObservation<'db> {
    site: RelationObservationSite<'db>,
    pattern: Type<'db>,
    is_target: bool,
    outcome: RelationOutcome,
    /// Retain nesting progress when short-circuiting reaches the same failing obligations.
    /// Structure beyond the declaration's own size does not introduce additional states.
    size: usize,
    /// Arity checks can reject tuples or callables before comparing their elements. Count
    /// immediate children separately from their nested structure, up to the annotation's arity.
    arity: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, get_size2::GetSize, salsa::SalsaValue)]
enum RelationObservationSite<'db> {
    Argument(usize),
    TypeVar(BoundTypeVarIdentity<'db>),
}

/// Counts stored children without following their types or expanding declaration bodies.
/// In particular, a deeply nested tuple element still occupies only one tuple position.
struct TypeArity<'a, 'db> {
    env: &'a ProgramEnvironment<'db>,
    count: Cell<usize>,
}

impl TypeArity<'_, '_> {
    fn of<'db>(db: &'db dyn Db, env: &ProgramEnvironment<'db>, ty: Type<'db>) -> usize {
        let visitor = TypeArity {
            env,
            count: Cell::new(0),
        };
        if let TypeKind::NonAtomic(ty) = TypeKind::from(ty) {
            walk_non_atomic_type(db, ty, &visitor);
        }
        visitor.count.get()
    }
}

impl<'db> TypeVisitor<'db> for TypeArity<'_, 'db> {
    fn program_environment(&self) -> &ProgramEnvironment<'db> {
        self.env
    }
    fn should_visit_lazy_type_attributes(&self) -> bool {
        false
    }
    fn visit_type(&self, _db: &'db dyn Db, _ty: Type<'db>) {
        self.count.set(self.count.get() + 1);
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, get_size2::GetSize, salsa::SalsaValue)]
enum RelationOutcome {
    Never,
    Always,
    Conditional,
}

/// Results of relation obligations involving a fixed set of declared types.
/// Restricting the keys to declarations keeps this state finite even when the source grows.
struct RelationObservations<'a, 'db> {
    patterns: &'a FxHashSet<Type<'db>>,
    results: RefCell<FxHashSet<RelationObservation<'db>>>,
    site: Cell<RelationObservationSite<'db>>,
    inferred: RefCell<FxOrderMap<BoundTypeVarIdentity<'db>, FxOrderSet<Type<'db>>>>,
}

#[derive(Clone)]
pub(super) struct TypeRelationChecker<'a, 'c, 'db> {
    pub(super) env: &'a ProgramEnvironment<'db>,
    pub(super) constraints: &'c ConstraintSetBuilder<'db>,
    pub(super) inferable: TypeVarSet<'db>,
    pub(super) relation: TypeRelation,
    pub(super) typevar_evaluation: TypeVarEvaluation,
    context_tree: Option<ErrorContextTree<'db>>,
    observations: Option<&'a RelationObservations<'a, 'db>>,
    given: ConstraintSet<'db, 'c>,
    perform_expensive_checks: bool,

    // N.B. these fields are private to reduce the risk of
    // "double-visiting" a given pair of types. You should
    // generally only ever call `self.relation_visitor.visit()`
    // or `self.disjointness_visitor.visit()` from
    // `check_type_pair`, never from `check_typeddict_pair` or
    // any other more "low-level" method.
    relation_visitor: &'a HasRelationToVisitor<'db, 'c>,
    disjointness_visitor: &'a IsDisjointVisitor<'db, 'c>,
    pub(super) signature_relation_visitor: &'a SignatureRelationVisitor<'db>,
    pub(super) materialization_visitor: &'a ApplyTypeMappingVisitor<'a, 'db>,
}

impl<'a, 'c, 'db> TypeRelationChecker<'a, 'c, 'db> {
    /// Create a relation checker that eagerly evaluates type variables.
    #[expect(clippy::too_many_arguments)]
    pub(super) fn new(
        env: &'a ProgramEnvironment<'db>,
        relation: TypeRelation,
        constraints: &'c ConstraintSetBuilder<'db>,
        inferable: TypeVarSet<'db>,
        relation_visitor: &'a HasRelationToVisitor<'db, 'c>,
        disjointness_visitor: &'a IsDisjointVisitor<'db, 'c>,
        signature_relation_visitor: &'a SignatureRelationVisitor<'db>,
        materialization_visitor: &'a ApplyTypeMappingVisitor<'a, 'db>,
    ) -> Self {
        Self {
            env,
            constraints,
            inferable,
            relation,
            typevar_evaluation: TypeVarEvaluation::Eager,
            context_tree: None,
            observations: None,
            given: ConstraintSet::from_bool(constraints, false),
            perform_expensive_checks: true,
            relation_visitor,
            disjointness_visitor,
            signature_relation_visitor,
            materialization_visitor,
        }
    }

    pub(super) fn subtyping(
        env: &'a ProgramEnvironment<'db>,
        constraints: &'c ConstraintSetBuilder<'db>,
        inferable: TypeVarSet<'db>,
        relation_visitor: &'a HasRelationToVisitor<'db, 'c>,
        disjointness_visitor: &'a IsDisjointVisitor<'db, 'c>,
        signature_relation_visitor: &'a SignatureRelationVisitor<'db>,
        materialization_visitor: &'a ApplyTypeMappingVisitor<'a, 'db>,
    ) -> Self {
        Self::new(
            env,
            TypeRelation::Subtyping,
            constraints,
            inferable,
            relation_visitor,
            disjointness_visitor,
            signature_relation_visitor,
            materialization_visitor,
        )
    }

    pub(super) fn constraint_set_assignability(
        env: &'a ProgramEnvironment<'db>,
        constraints: &'c ConstraintSetBuilder<'db>,
        relation_visitor: &'a HasRelationToVisitor<'db, 'c>,
        disjointness_visitor: &'a IsDisjointVisitor<'db, 'c>,
        signature_relation_visitor: &'a SignatureRelationVisitor<'db>,
        materialization_visitor: &'a ApplyTypeMappingVisitor<'a, 'db>,
    ) -> Self {
        Self {
            typevar_evaluation: TypeVarEvaluation::Lazy,
            ..Self::new(
                env,
                TypeRelation::Assignability,
                constraints,
                TypeVarSet::None,
                relation_visitor,
                disjointness_visitor,
                signature_relation_visitor,
                materialization_visitor,
            )
        }
    }

    pub(super) fn constraint_set_assignability_with_context(
        env: &'a ProgramEnvironment<'db>,
        constraints: &'c ConstraintSetBuilder<'db>,
        relation_visitor: &'a HasRelationToVisitor<'db, 'c>,
        disjointness_visitor: &'a IsDisjointVisitor<'db, 'c>,
        signature_relation_visitor: &'a SignatureRelationVisitor<'db>,
        materialization_visitor: &'a ApplyTypeMappingVisitor<'a, 'db>,
    ) -> Self {
        Self {
            env,
            constraints,
            inferable: TypeVarSet::None,
            relation: TypeRelation::Assignability,
            typevar_evaluation: TypeVarEvaluation::Lazy,
            context_tree: Some(ErrorContextTree::new(TypeRelation::Assignability)),
            observations: None,
            given: ConstraintSet::from_bool(constraints, false),
            perform_expensive_checks: true,
            relation_visitor,
            disjointness_visitor,
            signature_relation_visitor,
            materialization_visitor,
        }
    }

    pub(super) fn assignability_with_context(
        env: &'a ProgramEnvironment<'db>,
        constraints: &'c ConstraintSetBuilder<'db>,
        relation_visitor: &'a HasRelationToVisitor<'db, 'c>,
        disjointness_visitor: &'a IsDisjointVisitor<'db, 'c>,
        signature_relation_visitor: &'a SignatureRelationVisitor<'db>,
        materialization_visitor: &'a ApplyTypeMappingVisitor<'a, 'db>,
    ) -> Self {
        Self {
            env,
            constraints,
            inferable: TypeVarSet::None,
            relation: TypeRelation::Assignability,
            typevar_evaluation: TypeVarEvaluation::Eager,
            context_tree: Some(ErrorContextTree::new(TypeRelation::Assignability)),
            observations: None,
            given: ConstraintSet::from_bool(constraints, false),
            perform_expensive_checks: true,
            relation_visitor,
            disjointness_visitor,
            signature_relation_visitor,
            materialization_visitor,
        }
    }

    pub(super) fn with_inferable_typevars(&self, inferable: TypeVarSet<'db>) -> Self {
        Self {
            inferable,
            ..self.clone()
        }
    }

    /// Checks class subtyping without discarding the active recursive relation state.
    pub(super) fn is_class_subtype(
        &self,
        db: &'db dyn Db,
        source: ClassType<'db>,
        target: ClassType<'db>,
    ) -> bool {
        let env = self.env;
        Self {
            observations: self.observations,
            ..Self::subtyping(
                env,
                self.constraints,
                TypeVarSet::None,
                self.relation_visitor,
                self.disjointness_visitor,
                self.signature_relation_visitor,
                self.materialization_visitor,
            )
        }
        .check_class_pair(db, source, target)
        .is_always_satisfied(db, env)
    }

    pub(super) const fn is_eager_assignability(&self) -> bool {
        self.relation.is_assignability()
            && matches!(self.typevar_evaluation, TypeVarEvaluation::Eager)
    }

    fn should_expand_intersection(
        &self,
        db: &'db dyn Db,
        intersection: IntersectionType<'db>,
    ) -> bool {
        source_intersection::should_expand_source_intersection_sync(
            intersection,
            &source_intersection::InlineSourceIntersectionEffects::new(db, self),
        )
        .unwrap_or_else(|never| match never {})
    }

    fn check_source_typevar_bounds(
        &self,
        db: &'db dyn Db,
        bound_or_constraints: TypeVarBoundOrConstraints<'db>,
        target: Type<'db>,
    ) -> ConstraintSet<'db, 'c> {
        match bound_or_constraints {
            TypeVarBoundOrConstraints::UpperBound(bound) => self.check_type_pair(db, bound, target),
            TypeVarBoundOrConstraints::Constraints(constraints) => constraints
                .elements(db)
                .iter()
                .when_all(db, self.constraints, |&constraint| {
                    self.check_type_pair(db, constraint, target)
                }),
        }
    }

    fn check_source_union(
        &self,
        db: &'db dyn Db,
        union: UnionType<'db>,
        target: Type<'db>,
    ) -> ConstraintSet<'db, 'c> {
        if let Some(supertype) = union.common_literal_supertype(db, self.env) {
            // Use the broader supertype only as a positive proof. If it has the requested
            // relation to the target, then every literal in the union does too. Otherwise,
            // check each literal individually.
            let supertype_result =
                self.without_context_collection(|| self.check_type_pair(db, supertype, target));
            if supertype_result.is_trivially_always_satisfied() {
                return supertype_result;
            }
        }

        union
            .elements(db)
            .iter()
            .when_all(db, self.constraints, |&element| {
                let constraint_set = self.check_type_pair(db, element, target);
                if let Some(context) = self.report_context()
                    && constraint_set.is_never_satisfied(db, self.env)
                {
                    context.push(ErrorContext::NotAllUnionElementsAssignable {
                        element,
                        union: Type::Union(union),
                        target,
                    });
                }
                constraint_set
            })
    }

    fn check_target_union(
        &self,
        db: &'db dyn Db,
        source: Type<'db>,
        union: UnionType<'db>,
    ) -> ConstraintSet<'db, 'c> {
        target_union::check_target_union_sync(
            source,
            union,
            &target_union::InlineTargetUnionEffects::new(db, self),
        )
        .unwrap_or_else(|never| match never {})
    }

    fn check_target_intersection(
        &self,
        db: &'db dyn Db,
        source: Type<'db>,
        intersection: IntersectionType<'db>,
    ) -> ConstraintSet<'db, 'c> {
        target_intersection::check_target_intersection_sync(
            source,
            intersection,
            self.relation,
            &target_intersection::InlineTargetIntersectionEffects::new(db, self),
        )
        .unwrap_or_else(|never| match never {})
    }

    fn check_source_intersection(
        &self,
        db: &'db dyn Db,
        intersection: IntersectionType<'db>,
        target: Type<'db>,
    ) -> ConstraintSet<'db, 'c> {
        source_intersection::check_source_intersection_sync(
            intersection,
            target,
            &source_intersection::InlineSourceIntersectionEffects::new(db, self),
        )
        .unwrap_or_else(|never| match never {})
    }

    /// Return the collected error context, or an empty tree if collection was disabled.
    pub(super) fn into_error_context(self) -> ErrorContextTree<'db> {
        self.context_tree
            .unwrap_or_else(|| ErrorContextTree::new(self.relation))
    }

    pub(super) fn always(&self) -> ConstraintSet<'db, 'c> {
        ConstraintSet::from_bool(self.constraints, true)
    }

    pub(super) fn never(&self) -> ConstraintSet<'db, 'c> {
        ConstraintSet::from_bool(self.constraints, false)
    }

    /// Overwrite the error context tree with a new root context and child nodes.
    fn set_context(
        &self,
        root: ErrorContext<'db>,
        children: impl IntoIterator<Item = ErrorContextTree<'db>>,
    ) {
        if let Some(context_tree) = &self.context_tree {
            context_tree.set(root, children);
        }
    }

    /// Return true if error context collection is currently enabled.
    pub(super) fn is_context_collection_enabled(&self) -> bool {
        self.report_context().is_some()
    }

    /// If error context collection is enabled, returns the current error context tree. You will
    /// typically use [`push`][ErrorContextTree::push] to add additional information to the error
    /// context. Returns `None` if error context collection is disabled, allowing you to skip
    /// expensive checks.
    pub(super) fn report_context(&self) -> Option<&ErrorContextTree<'db>> {
        self.context_tree
            .as_ref()
            .filter(|context| context.is_enabled())
    }

    /// Suppress context for a private comparison subtree without toggling shared state.
    pub(crate) fn with_context_collection_disabled(&self) -> Self {
        Self {
            context_tree: None,
            ..self.clone()
        }
    }

    /// Temporarily suppress error context collection for the duration of `f`.
    ///
    /// Note: we may eventually not need this method once we properly retain error
    /// context everywhere.
    pub(super) fn without_context_collection<R>(&self, f: impl FnOnce() -> R) -> R {
        let Some(context_tree) = &self.context_tree else {
            return f();
        };
        let was_enabled = context_tree.is_enabled();
        context_tree.set_enabled(false);
        let result = f();
        context_tree.set_enabled(was_enabled);
        result
    }

    fn should_provide_callable_upcast_context(&self, source: Type<'db>) -> bool {
        if !self.is_context_collection_enabled() {
            return false;
        }

        // These displays already expose the signature being compared; wrapping them would
        // duplicate the lower-level callable mismatch context.
        !matches!(
            source,
            Type::Callable(_)
                | Type::FunctionLiteral(_)
                | Type::BoundMethod(_)
                | Type::KnownInstance(KnownInstanceType::FunctoolsPartial(_))
        )
    }

    fn with_recursion_guard(
        &self,
        db: &'db dyn Db,
        source: Type<'db>,
        target: Type<'db>,
        work: impl FnOnce() -> ConstraintSet<'db, 'c>,
    ) -> ConstraintSet<'db, 'c> {
        let dependencies = dependencies::OrdinaryDependencies;
        let effects = guard::InlineGuard::new(db, &dependencies);
        legacy_inline(guard::with_relation_guard(
            self,
            source,
            target,
            || ready(Ok(work())),
            &effects,
        ))
    }

    fn recursive_type_pair_fallback(
        &self,
        db: &'db dyn Db,
        source: Type<'db>,
        target: Type<'db>,
    ) -> ConstraintSet<'db, 'c> {
        if let Some(nominally_satisfied) = self.try_check_nominal_protocol_cycle(db, source, target)
        {
            return nominally_satisfied;
        }

        // TODO: Recursively-specialized structural types can encode context-free languages,
        // whose inclusion and equivalence are undecidable. No complete fallback exists, but
        // more decidable cases can be recognized here before conservatively rejecting the pair.
        //
        // Strictly speaking, it is incorrect to use either `never` or `always` as a conservative result.
        // The correct choice here is a logical value that is "neither true nor false", and expressing this requires the introduction of 3-valued logic.
        // Discussion: https://github.com/astral-sh/ty/issues/4050
        self.never()
    }

    /// Is `target` a metaclass instance (a nominal instance of a subclass of `builtins.type`)?
    ///
    /// This does not include all types that are subtypes of `builtins.type`! The semantic
    /// distinction that matters here is not whether `target` is a subtype of `type`, but whether
    /// it constrains the class or the metaclass of its inhabitants.
    ///
    /// The type `type[C]` and the type `ABCMeta` are both subtypes of `builtins.type`, but they
    /// constrain their inhabitants in different domains. `type[C]` constrains in the regular-class
    /// domain (it describes a regular class object and all its subclasses). A metaclass instance
    /// like `ABCMeta` constrains in the metaclass domain: its inhabitants can be class objects
    /// that are unrelated to each other in the regular-class domain (they do not inherit each
    /// other or any other common base), but they are all constrained to have a metaclass that
    /// inherits from `ABCMeta`.
    fn is_metaclass_instance(&self, db: &'db dyn Db, target: Type<'db>) -> bool {
        target.as_nominal_instance().is_some_and(|instance| {
            let env = self.env;
            KnownClass::Type
                .try_to_class_literal(db, env)
                .is_some_and(|type_class| {
                    instance.class(db, env).is_subclass_of(
                        db,
                        env,
                        ClassType::NonGeneric(ClassLiteral::Static(type_class)),
                    )
                })
        })
    }

    /// Check the relation between a `type[T]` and a target type `A` when `A` can either be
    /// projected into the ordinary instance/object domain via `.to_instance()`, or is a plain
    /// metaclass object type.
    ///
    /// In the former case, we unwrap the source from `type[T]` to `T`, push the target down
    /// through `A.to_instance()`, and compare those types. This is the right interpretation for
    /// targets like `type[S]`: they constrain class objects via the instances they create, not via
    /// their metaclasses.
    ///
    /// For a metaclass instance type (see `is_metaclass_instance` for definition),
    /// `A.to_instance()` is too lossy: it collapses to `object`, because we have no precise
    /// instance-space representation for "all class objects whose metaclass inhabits `A`". For
    /// these types which constrain in the metaclass space, we instead need to resolve `type[T]` to
    /// the metaclass of the upper bound of `T`, and compare in the metaclass-instance domain
    /// directly.
    ///
    /// When `.to_instance()` is an over-approximation, compare the original target in the
    /// class-object domain instead. This preserves constraints that the projection discards, as
    /// well as source constraints so that `T: (Y, Z)` can still be related to
    /// `type[Y] | type[Z]`.
    ///
    /// Exact class objects also have an over-approximated instance projection. For `T: (Y, Z)`
    /// where `Z` extends `Y`, instance subtyping would incorrectly simplify
    /// `type[T] & <class 'Y'>` to `type[T]`: both `Y` and `Z` instances are subtypes of `Y`, but
    /// only the class object `Y` satisfies `klass is Y`. The exception is a type variable whose
    /// upper bound normalizes to this exact class object. That can only happen for a final class,
    /// so the exact object is the only valid specialization of the type variable.
    ///
    /// Return `None` for targets without a `.to_instance()` projection, allowing other type-pair
    /// branches to decide their relation.
    fn check_typevar_subclass_relation_to_target(
        &self,
        db: &'db dyn Db,
        source_subclass: SubclassOfType<'db>,
        target: Type<'db>,
    ) -> Option<ConstraintSet<'db, 'c>> {
        match typevar_subclass::check_typevar_subclass_sync(
            source_subclass,
            target,
            typevar_subclass::TypeVarSubclassFacts,
            &typevar_subclass::OrdinaryTypeVarSubclass { db, checker: self },
        ) {
            Ok(result) => result,
            Err(never) => match never {},
        }
    }

    /// Return a constraint set indicating the conditions under which `self.relation` holds between `source` and `target`.
    pub(super) fn check_type_pair(
        &self,
        db: &'db dyn Db,
        source: Type<'db>,
        target: Type<'db>,
    ) -> ConstraintSet<'db, 'c> {
        let dependencies = dependencies::OrdinaryDependencies;
        let pending = pair::PairEvaluation::start(db, self, source, target, &dependencies)
            .unwrap_or_else(|never| match never {});
        let result = self.check_type_pair_inner(db, source, target);
        pending
            .finish(db, result, &dependencies)
            .unwrap_or_else(|never| match never {})
    }

    #[ty_mapping_probe_macros::dual_relation]
    async fn check_type_pair_inner_with<E: pair_effects::PairEffects<'a, 'c, 'db>>(
        &self,
        source: Type<'db>,
        target: Type<'db>,
        effects: &E,
    ) -> Result<ConstraintSet<'db, 'c>, E::Error> {
        // Reflexivity and lazy constraints can bypass the RecursiveVar match arm below.
        source.assert_not_recursive_var();
        target.assert_not_recursive_var();
        if let Some(source) = source.materialized_divergent_fallback() {
            return effects.check_type_pair(self, source, target).await;
        }

        if let Some(target) = target.materialized_divergent_fallback() {
            return effects.check_type_pair(self, source, target).await;
        }

        // Subtyping implies assignability, so if subtyping is reflexive and the two types are
        // equal, it is both a subtype and assignable. Assignability is always reflexive.
        //
        // Note that we could do a full equivalence check here, but that would be both expensive
        // and unnecessary. This early return is only an optimisation.
        if source == target && self.relation.can_safely_assume_reflexivity(source) {
            return Ok(self.always());
        }

        // Handle constraint implication first. If either `source` or `target` is a typevar, check
        // the constraint set to see if the corresponding constraint is satisfied.
        if self.relation == TypeRelation::SubtypingAssuming
            && (source.is_type_var() || target.is_type_var())
        {
            return Ok(effects
                .implied_typevar_relation(self, source, target)
                .await?);
        }

        // With lazy evaluation, comparisons with a type variable are translated directly into a
        // constraint set.
        if self.typevar_evaluation == TypeVarEvaluation::Lazy {
            // A typevar satisfies a relation when...it satisfies the relation. Yes that's a
            // tautology! We're moving the caller's subtyping/assignability requirement into a
            // constraint set. If the typevar has an upper bound or constraints, then the relation
            // only has to hold when the typevar has a valid specialization (i.e., one that
            // satisfies the upper bound/constraints).
            if let Type::TypeVar(bound_typevar) = source {
                return effects
                    .lazy_typevar_upper_constraint(self, bound_typevar, target)
                    .await;
            } else if let Type::TypeVar(bound_typevar) = target {
                return effects
                    .lazy_typevar_lower_constraint(self, bound_typevar, source)
                    .await;
            }
        }

        Ok(match (source, target) {
            (Type::RecursiveVar(_), _) | (_, Type::RecursiveVar(_)) => {
                unreachable!("semantic operation on an unbound recursive variable")
            }
            // Everything is a subtype of `object`.
            (_, Type::NominalInstance(target)) if target.is_object() => self.always(),
            (_, Type::ProtocolInstance(target))
                if effects
                    .protocol_is_equivalent_to_object(self, target)
                    .await? =>
            {
                self.always()
            }

            // `Never` is the bottom type, the empty set.
            // It is a subtype of all other types.
            (Type::Never, _) => self.always(),

            (Type::TypeVar(source_typevar), Type::TypeVar(target_typevar))
                if effects
                    .same_typevar_occurrence(self, source_typevar, target_typevar)
                    .await? =>
            {
                self.always()
            }

            // In some specific situations, `Any`/`Unknown`/`@Todo` can be simplified out of unions and intersections,
            // but this is not true for divergent types (and moving this case any lower down appears to cause
            // "too many cycle iterations" panics).
            (Type::Divergent(_), _) | (_, Type::Divergent(_)) => {
                ConstraintSet::from_bool(self.constraints, self.relation.is_assignability())
            }

            (Type::Recursive(source_recursive), _) => {
                // Both comparing arguments and unfolding can revisit this pair.
                effects
                    .guard(self, source, target, || async {
                        Ok({
                            let by_arguments = if let Type::Recursive(target_recursive) = target {
                                effects
                                    .when_recursive_types_relate_by_arguments(
                                        self,
                                        source_recursive,
                                        target_recursive,
                                    )
                                    .await?
                            } else {
                                self.never()
                            };
                            by_arguments
                                .or_with(
                                    self.constraints,
                                    || async {
                                        Ok({
                                            if let Some(source_unfolded) = effects
                                                .unfold_recursive(self, source_recursive)
                                                .await?
                                            {
                                                effects
                                                    .check_type_pair(self, source_unfolded, target)
                                                    .await?
                                            } else {
                                                ConstraintSet::from_bool(
                                                    self.constraints,
                                                    self.relation.is_assignability(),
                                                )
                                            }
                                        })
                                    },
                                    effects,
                                )
                                .await?
                        })
                    })
                    .await?
            }

            (_, Type::Recursive(target_recursive)) => {
                effects
                    .guard(self, source, target, || async {
                        Ok({
                            if let Some(target_unfolded) =
                                effects.unfold_recursive(self, target_recursive).await?
                            {
                                effects
                                    .check_type_pair(self, source, target_unfolded)
                                    .await?
                            } else {
                                ConstraintSet::from_bool(
                                    self.constraints,
                                    self.relation.is_assignability(),
                                )
                            }
                        })
                    })
                    .await?
            }

            // Instances of classes that inherit from an explicit `Any` base retain their nominal
            // identity and precise members, but have the same assignability as `Any`.
            (Type::NominalInstance(source), _)
                if self.relation.is_assignability() && source.inherits_from_explicit_any() =>
            {
                self.always()
            }

            (Type::TypeAlias(source_alias), _) => {
                effects
                    .guard(self, source, target, || async {
                        Ok({
                            effects
                                .check_type_pair(
                                    self,
                                    effects.alias_value(self, source_alias).await?,
                                    target,
                                )
                                .await?
                        })
                    })
                    .await?
            }

            (_, Type::TypeAlias(target_alias)) => {
                effects
                    .guard(self, source, target, || async {
                        Ok({
                            effects
                                .check_type_pair(
                                    self,
                                    source,
                                    effects.alias_value(self, target_alias).await?,
                                )
                                .await?
                        })
                    })
                    .await?
            }

            // Annotation unions retain type aliases so recursive aliases can be represented.
            // Normalize direct alias elements together before checking the union so reductions
            // that depend on multiple elements, such as all members of an enum, are visible.
            (_, Type::Union(union)) if effects.union_has_aliases(self, union).await? => {
                effects
                    .guard(self, source, target, || async {
                        Ok({
                            effects
                                .check_type_pair(
                                    self,
                                    source,
                                    effects.expand_union_aliases(self, union).await?,
                                )
                                .await?
                        })
                    })
                    .await?
            }

            (Type::TypeForm(source_typeform), Type::TypeForm(target_typeform)) => {
                effects
                    .guard(self, source, target, || async {
                        Ok({
                            effects
                                .check_type_pair(
                                    self,
                                    effects.type_form_argument(source_typeform).await?,
                                    effects.type_form_argument(target_typeform).await?,
                                )
                                .await?
                        })
                    })
                    .await?
            }

            (Type::SubclassOf(source_subclass), Type::TypeForm(target_typeform)) => {
                effects
                    .check_type_pair(
                        self,
                        effects.subclass_instance(self, source_subclass).await?,
                        effects.type_form_argument(target_typeform).await?,
                    )
                    .await?
            }

            (Type::NominalInstance(source_instance), Type::TypeForm(target_typeform))
                if effects
                    .nominal_has_known_class(self, source_instance, KnownClass::Type)
                    .await? =>
            {
                effects
                    .check_type_pair(
                        self,
                        Type::object(),
                        effects.type_form_argument(target_typeform).await?,
                    )
                    .await?
            }

            (Type::ClassLiteral(source_class), Type::TypeForm(target_typeform)) => {
                effects
                    .check_type_pair(
                        self,
                        effects
                            .class_instance(
                                self,
                                effects
                                    .class_default_specialization(self, source_class)
                                    .await?,
                            )
                            .await?,
                        effects.type_form_argument(target_typeform).await?,
                    )
                    .await?
            }

            (Type::GenericAlias(source_alias), Type::TypeForm(target_typeform)) => {
                effects
                    .check_type_pair(
                        self,
                        effects
                            .class_instance(self, ClassType::Generic(source_alias))
                            .await?,
                        effects.type_form_argument(target_typeform).await?,
                    )
                    .await?
            }

            (Type::KnownInstance(source_instance), Type::TypeForm(target_typeform))
                if let Some(source_argument) = effects
                    .known_instance_type_form_argument(self, source_instance)
                    .await? =>
            {
                effects
                    .check_type_pair(
                        self,
                        source_argument,
                        effects.type_form_argument(target_typeform).await?,
                    )
                    .await?
            }

            (Type::SpecialForm(source_form), Type::TypeForm(target_typeform)) => {
                effects
                    .special_form_type_form_argument(self, source_form)
                    .await?
                    .when_some_and_with(
                        self.constraints,
                        |source_argument| async move {
                            Ok({
                                effects
                                    .check_type_pair(
                                        self,
                                        source_argument,
                                        effects.type_form_argument(target_typeform).await?,
                                    )
                                    .await?
                            })
                        },
                        effects,
                    )
                    .await?
            }

            (Type::GenericAlias(_), Type::NominalInstance(target_instance))
                if effects
                    .nominal_has_known_class(self, target_instance, KnownClass::GenericAlias)
                    .await? =>
            {
                self.always()
            }

            (Type::EnumComplement(complement), Type::LiteralValue(_) | Type::Union(_)) => {
                effects
                    .check_type_pair(
                        self,
                        effects.enum_remaining_literals(self, complement).await?,
                        target,
                    )
                    .await?
            }

            (Type::EnumComplement(complement), _) => {
                effects
                    .check_type_pair(
                        self,
                        effects.enum_intersection(self, complement).await?,
                        target,
                    )
                    .await?
            }

            (_, Type::EnumComplement(complement)) => {
                effects
                    .check_type_pair(
                        self,
                        source,
                        effects.enum_intersection(self, complement).await?,
                    )
                    .await?
            }

            // Field definitions in dataclasses and dataclass-transformers can involve calls to
            // `dataclasses.field` or custom field-specifier functions. The annotated return type
            // of these functions is often explicitly wrong to "help" type checkers. We therefore
            // overwrite their return type unconditionally and pretend that all field-specifier
            // calls return a `KnownInstanceType::Field`.
            //
            // Here, we model assignability of this special type to the declared field type. In
            // order to catch mistakes in the field definition, we only consider this known instance
            // type to be assignable if the default value and converter output type is compatible
            // with the declared field type.
            //
            // We consider three cases:
            //     1. If a converter is provided, we validate the output/return type of the converter
            //        function against the declared field type. The presence of a default value is
            //        irrelevant in this case, as the converter is expected to handle conversion from
            //        the default value's type to the declared field type. Incompatibilities between
            //        the two must be caught by the field-specifier function's signature.
            //     2. If no converter is provided, we validate the default value's type against the
            //        declared field type.
            //     3. If neither a converter nor a default value is provided, we allow the field to be
            //        considered assignable to any type.
            (Type::KnownInstance(KnownInstanceType::Field(field)), _)
                if self.relation.is_assignability() =>
            {
                (effects.field_default(field).await?)
                    .when_none_or_with(
                        self.constraints,
                        |default_type| async move {
                            effects.check_type_pair(self, default_type, target).await
                        },
                        effects,
                    )
                    .await?
                    .and_with(
                        self.constraints,
                        || async {
                            Ok({
                                (effects.field_converter(field).await?)
                                    .map(|(_, output_ty)| output_ty)
                                    .when_none_or_with(
                                        self.constraints,
                                        |converter_output_type| async move {
                                            Ok({
                                                effects
                                                    .check_type_pair(
                                                        self,
                                                        converter_output_type,
                                                        target,
                                                    )
                                                    .await?
                                            })
                                        },
                                        effects,
                                    )
                                    .await?
                            })
                        },
                        effects,
                    )
                    .await?
            }

            // The read-only `__func__` and `__wrapped__` attributes expose the complete wrapped
            // object, so its attributes matter here as well as its call signature.
            (
                Type::KnownInstance(KnownInstanceType::MethodWrapper(source_wrapper)),
                Type::KnownInstance(KnownInstanceType::MethodWrapper(target_wrapper)),
            ) if effects.method_wrapper_kind(source_wrapper).await?
                == effects.method_wrapper_kind(target_wrapper).await? =>
            {
                effects
                    .guard(self, source, target, || async {
                        Ok({
                            effects
                                .check_type_pair(
                                    self,
                                    effects.method_wrapper_type(source_wrapper).await?,
                                    effects.method_wrapper_type(target_wrapper).await?,
                                )
                                .await?
                        })
                    })
                    .await?
            }

            (
                Type::KnownInstance(KnownInstanceType::FunctoolsPartial(source_partial)),
                Type::KnownInstance(KnownInstanceType::FunctoolsPartial(target_partial)),
            )
            | (
                Type::KnownInstance(KnownInstanceType::FunctoolsPartialCall(source_partial)),
                Type::KnownInstance(KnownInstanceType::FunctoolsPartialCall(target_partial)),
            ) => {
                effects
                    .guard(self, source, target, || async {
                        Ok({
                            // The reduced signature of `partial(boolean)` is `() -> bool`, which is a
                            // subtype of the `() -> int` signature of `partial(integer)`. The wrapped
                            // functions still differ, and `.func` exposes which one was chosen:
                            //
                            // ```py
                            // from functools import partial
                            //
                            // def integer() -> int:
                            //     return 1
                            //
                            // def boolean() -> bool:
                            //     return True
                            //
                            // int_partial = partial(integer)
                            // bool_partial = partial(boolean)
                            //
                            // def choose(flag: bool) -> bool:
                            //     selected = bool_partial if flag else int_partial
                            //     return reveal_type(selected.func is boolean)  # revealed: bool
                            // ```
                            //
                            // The comparison is true when `flag` is true. Dropping `bool_partial` from the
                            // union based only on its reduced signature would make ty reveal `Literal[False]`
                            // instead of `bool`. Check the wrapped callable as well as the reduced signature.
                            effects
                                .check_type_pair(
                                    self,
                                    effects.interned_type(effects.partial_wrapped(source_partial).await?).await?,
                                    effects.interned_type(effects.partial_wrapped(target_partial).await?).await?,
                                )
                                .await?
                                .and_with(
                                    self.constraints,
                                    || async {
                                        Ok({
                                            effects
                                                .check_callable_pair(
                                                    self,
                                                    effects.partial_callable(source_partial).await?,
                                                    effects.partial_callable(target_partial).await?,
                                                )
                                                .await?
                                        })
                                    },
                                    effects,
                                )
                                .await?
                        })
                    })
                    .await?
            }

            (
                Type::KnownInstance(KnownInstanceType::Sentinel(source_sentinel)),
                Type::KnownInstance(KnownInstanceType::Sentinel(target_sentinel)),
            ) => ConstraintSet::from_bool(
                self.constraints,
                effects
                    .same_sentinel(self, source_sentinel, target_sentinel)
                    .await?,
            ),

            // A nominal descriptor annotation specifies the wrapped callable through `__func__`.
            // Comparing that contract directly preserves overloads and avoids replacing the
            // wrapped callable's parameter and return types with the default specialization.
            (
                Type::KnownInstance(KnownInstanceType::MethodWrapper(wrapper)),
                Type::NominalInstance(target_instance),
            ) if effects
                .wrapper_matches_nominal(self, wrapper, target_instance)
                .await? =>
            {
                effects
                    .guard(self, source, target, || async {
                        Ok({
                            let Some(target_function) =
                                effects.lookup_wrapped_function(self, target).await?
                            else {
                                return Ok(self.never());
                            };
                            effects
                                .check_type_pair(
                                    self,
                                    effects.method_wrapper_type(wrapper).await?,
                                    target_function,
                                )
                                .await?
                        })
                    })
                    .await?
            }

            // When checking `FunctoolsPartial <: functools.partial[T]`, we need to specialize
            // the nominal instance with the partial's return type so the check is precise.
            (
                Type::KnownInstance(KnownInstanceType::FunctoolsPartial(partial)),
                Type::NominalInstance(target_instance),
            ) if effects
                .nominal_class_is_known(self, target_instance, KnownClass::FunctoolsPartial)
                .await? =>
            {
                let specialized = effects
                    .specialize_partial_instance(self, effects.partial_callable(partial).await?)
                    .await?;
                effects.check_type_pair(self, specialized, target).await?
            }

            // Dynamic is only a subtype of `object` and only a supertype of `Never`; both were
            // handled above. It's always assignable, though.
            //
            // Redundancy sits in between subtyping and assignability. `Any <: T` only holds true
            // if `T` is also a dynamic type or a union that contains a dynamic type. Similarly,
            // `T <: Any` only holds true if `T` is a dynamic type or an intersection that
            // contains a dynamic type.
            (Type::Dynamic(_dynamic), _) => ConstraintSet::from_bool(
                self.constraints,
                match self.relation {
                    TypeRelation::Subtyping | TypeRelation::SubtypingAssuming => false,
                    TypeRelation::Assignability => true,
                    TypeRelation::Redundancy { .. } => match target {
                        Type::Dynamic(_) => true,
                        Type::Union(union) => effects.union_contains_dynamic(self, union).await?,
                        _ => false,
                    },
                },
            ),
            (_, Type::Dynamic(_)) => ConstraintSet::from_bool(
                self.constraints,
                match self.relation {
                    TypeRelation::Subtyping | TypeRelation::SubtypingAssuming => false,
                    TypeRelation::Assignability => true,
                    TypeRelation::Redundancy { .. } => match source {
                        Type::Dynamic(_) => true,
                        Type::Intersection(intersection) => {
                            // If a `Divergent` type is involved, it must not be eliminated.
                            effects
                                .intersection_contains_nondivergent_dynamic(self, intersection)
                                .await?
                        }
                        _ => false,
                    },
                },
            ),

            // In general, a TypeVar `T` is not redundant with a type `S` unless one of the two conditions is satisfied:
            // 1. `T` is a bound TypeVar and `T`'s upper bound is a subtype of `S`.
            //    TypeVars without an explicit upper bound are treated as having an implicit upper bound of `object`.
            // 2. `T` is a constrained TypeVar and all of `T`'s constraints are subtypes of `S`.
            //
            // However, there is one exception to this general rule: for any given typevar `T`,
            // `T` will always be a subtype of any union containing `T`.
            (_, Type::Union(union))
                if self.relation.can_safely_assume_reflexivity(source)
                    && effects.union_contains_type(self, union, source).await? =>
            {
                self.always()
            }

            // A similar rule applies in reverse to intersection types.
            (Type::Intersection(intersection), _)
                if self.relation.can_safely_assume_reflexivity(target)
                    && effects
                        .intersection_positive_contains(self, intersection, target)
                        .await? =>
            {
                self.always()
            }
            (Type::Intersection(intersection), _)
                if self.relation.is_assignability()
                    && effects
                        .intersection_contains_dynamic(self, intersection)
                        .await? =>
            {
                // If the intersection contains `Any`/`Unknown`/`@Todo`, it is assignable to any type.
                // `Any` could materialize to `Never`, `Never & T & ~S` simplifies to `Never` for any
                // `T` and any `S`, and `Never` is a subtype of all types.
                self.always()
            }
            (Type::Intersection(intersection), _)
                if self.relation.can_safely_assume_reflexivity(target)
                    && effects
                        .intersection_negative_contains(self, intersection, target)
                        .await? =>
            {
                self.never()
            }

            // `type[T]` is a subtype of the class object `A` if every instance of `T` is a subtype
            // of an instance of `A`. If `A` is a metaclass instance (instance of a specific
            // subclass of `type`), we instead compare in the metaclass-instance domain, since
            // collapsing `A` through `to_instance()` would erase it to `object` (we have no
            // precise representation for "all instances of any classes with a given metaclass").
            (Type::SubclassOf(subclass_of), _)
                if let Some(constraint_set) = effects
                    .check_typevar_subclass_relation_to_target(self, subclass_of, target)
                    .await? =>
            {
                constraint_set
            }

            // And vice versa. (No special metaclass handling is needed in this direction, since
            // "collapse to 'object'" in this case is a sound over-approximation.)
            (_, Type::SubclassOf(subclass_of))
                if let Some(type_var) = subclass_of.into_type_var()
                    && let Some(instance) =
                        effects.instance_approximation(self, source).await? =>
            {
                effects
                    .check_type_pair(self, instance, Type::TypeVar(type_var))
                    .await?
            }

            // A TypeVarTuple specialization is represented by one tuple value. Keep inferable
            // TypeVarTuples bare for constraint solving, but compare fixed symbolic values using
            // the same tuple relation as concrete specializations.
            (Type::TypeVar(bound_typevar), target)
                if !effects.typevar_is_inferable(self, bound_typevar).await?
                    && effects.typevar_is_typevartuple(self, bound_typevar).await?
                    && effects.is_exact_tuple_instance(self, target).await? =>
            {
                effects
                    .check_type_pair(
                        self,
                        effects.unpacked_typevartuple(self, bound_typevar).await?,
                        target,
                    )
                    .await?
            }
            // A fixed tuple cannot satisfy every specialization of a non-inferable TypeVarTuple.
            // Let it reach the ordinary rejection below; expanding the target would repeat the
            // same tuple comparison and cause the recursion guard to accept it.
            (source, Type::TypeVar(bound_typevar))
                if !effects.typevar_is_inferable(self, bound_typevar).await?
                    && effects.typevar_is_typevartuple(self, bound_typevar).await?
                    && effects
                        .is_variadic_exact_tuple_instance(self, source)
                        .await? =>
            {
                effects
                    .check_type_pair(
                        self,
                        source,
                        effects.unpacked_typevartuple(self, bound_typevar).await?,
                    )
                    .await?
            }

            // A gradual `ParamSpec` value (`...`) is assignability-consistent with any concrete
            // `ParamSpec` value. This only applies to fixed `ParamSpec` values in already-
            // specialized generic aliases; inferable `ParamSpec`s are handled by the inference
            // paths below.
            (Type::TypeVar(bound_typevar), Type::Callable(other))
            | (Type::Callable(other), Type::TypeVar(bound_typevar))
                if self.is_eager_assignability()
                    && !effects.typevar_is_inferable(self, bound_typevar).await?
                    && effects.typevar_domain(self, bound_typevar).await?
                        == TypeVarDomain::ParameterSignature
                    && effects
                        .callable_is_gradual_paramspec_value(self, other)
                        .await? =>
            {
                self.always()
            }

            // Compare fixed `ParamSpec`s with the endpoints of the materialization range of `...`:
            // its bottom is below every `ParamSpec`, and its top is above every `ParamSpec`.
            (Type::TypeVar(bound_typevar), Type::Callable(other))
                if !effects.typevar_is_inferable(self, bound_typevar).await?
                    && effects.typevar_domain(self, bound_typevar).await?
                        == TypeVarDomain::ParameterSignature
                    && effects.callable_is_top_paramspec_value(self, other).await? =>
            {
                self.always()
            }

            (Type::Callable(other), Type::TypeVar(bound_typevar))
                if !effects.typevar_is_inferable(self, bound_typevar).await?
                    && effects.typevar_domain(self, bound_typevar).await?
                        == TypeVarDomain::ParameterSignature
                    && effects
                        .callable_is_bottom_paramspec_value(self, other)
                        .await? =>
            {
                self.always()
            }

            // If the typevar is constrained, there must be multiple constraints, and the typevar
            // might be specialized to any one of them. However, the constraints do not have to be
            // disjoint, which means an lhs type might be a subtype of all of the constraints.
            (_, Type::TypeVar(bound_typevar))
                if !effects.typevar_is_inferable(self, bound_typevar).await?
                    && let constraints = effects
                        .typevar_constraints(self, bound_typevar)
                        .await?
                        .when_some_and_with(
                            self.constraints,
                            |constraints| async {
                                Ok({
                                    constraints
                                        .iter()
                                        .when_all_with(
                                            self.constraints,
                                            |c| async {
                                                Ok({
                                                    effects
                                                        .check_type_pair(self, source, *c)
                                                        .await?
                                                })
                                            },
                                            effects,
                                        )
                                        .await?
                                })
                            },
                            effects,
                        )
                        .await?
                    && !effects.is_never_satisfied(self, constraints).await? =>
            {
                constraints
            }

            (Type::TypeVar(bound_typevar), _)
                if effects.typevar_is_inferable(self, bound_typevar).await? =>
            {
                // The implicit lower bound of a typevar is `Never`, which means
                // that it is always assignable to any other type.

                // TODO: record the unification constraints

                self.always()
            }

            // Fast path for various types that we know `object` is never a subtype of.
            (
                Type::NominalInstance(source),
                Type::NominalInstance(_) | Type::SubclassOf(_) | Type::Callable(_),
            ) if source.is_object() => self.never(),

            // `object` is not a subtype of a non-universal protocol because some subclasses
            // might not implement it. For assignability, still inspect its actual members:
            // `object()` is hashable and commonly used as a sentinel for `Hashable` parameters.
            (Type::NominalInstance(source), Type::ProtocolInstance(_))
                if source.is_object() && !self.relation.is_assignability() =>
            {
                self.never()
            }

            // Fast path: `object` is not a subtype of any non-inferable type variable, since the
            // type variable could be specialized to a type smaller than `object`.
            (Type::NominalInstance(source), Type::TypeVar(typevar))
                if source.is_object() && !effects.typevar_is_inferable(self, typevar).await? =>
            {
                self.never()
            }

            (Type::NewTypeInstance(source_newtype), Type::NewTypeInstance(target_newtype)) => {
                effects
                    .check_newtype_pair(self, source_newtype, target_newtype)
                    .await?
            }

            (Type::Union(union), _) => effects.check_source_union(self, union, target).await?,
            (_, Type::Union(union)) => effects.check_target_union(self, source, union).await?,

            // If both sides are intersections we need to handle the right side first
            // (A & B & C) is a subtype of (A & B) because the left is a subtype of both A and B,
            // but none of A, B, or C is a subtype of (A & B).
            (_, Type::Intersection(intersection)) => {
                effects
                    .check_target_intersection(self, source, intersection)
                    .await?
            }

            // Check an inferable target's bound before splitting a source intersection.
            // For `T: A & B`, neither `A` nor `B` alone need satisfy the bound, but `A & B` does.
            (_, Type::TypeVar(typevar))
                if self.is_eager_assignability()
                    && effects.typevar_is_inferable(self, typevar).await? =>
            {
                // TODO: record the unification constraints
                effects
                    .typevar_upper_bound(self, typevar)
                    .await?
                    .when_none_or_with(
                        self.constraints,
                        |bound| async move { effects.check_type_pair(self, source, bound).await },
                        effects,
                    )
                    .await?
            }

            (Type::Intersection(intersection), _) => {
                effects
                    .check_source_intersection(self, intersection, target)
                    .await?
            }

            // A fully static typevar is a subtype of its upper bound, and to something similar to
            // the union of its constraints. An unbound, unconstrained, fully static typevar has an
            // implicit upper bound of `object` (which is handled above).
            (Type::TypeVar(bound_typevar), _)
                if !effects.typevar_is_inferable(self, bound_typevar).await?
                    && let Some(bound_or_constraints) = effects
                        .typevar_bound_or_constraints(self, bound_typevar)
                        .await? =>
            {
                effects
                    .check_source_typevar_bounds(self, bound_or_constraints, target)
                    .await?
            }

            // `Never` is the bottom type, the empty set.
            (_, Type::Never) => self.never(),

            // Other than the special cases checked above, no other types are a subtype of a
            // typevar, since there's no guarantee what type the typevar will be specialized to.
            // (If the typevar is bounded, it might be specialized to a smaller type than the
            // bound. This is true even if the bound is a final class, since the typevar can still
            // be specialized to `Never`.)
            (_, Type::TypeVar(bound_typevar))
                if !effects.typevar_is_inferable(self, bound_typevar).await? =>
            {
                self.never()
            }

            // TODO: Infer specializations here
            (_, Type::TypeVar(typevar)) if effects.typevar_is_inferable(self, typevar).await? => {
                self.never()
            }
            (Type::TypeVar(bound_typevar), _) => {
                // All inferable cases should have been handled above
                let inferable = effects.typevar_is_inferable(self, bound_typevar).await?;
                assert!(!inferable);
                self.never()
            }

            // All other `NewType` assignments fall back to the concrete base type.
            // This case must come after the TypeVar cases above, so that when checking
            // `NewType <: TypeVar`, we use the TypeVar handling rather than falling back
            // to the NewType's concrete base type.
            (Type::NewTypeInstance(source_newtype), _) => {
                effects
                    .check_type_pair(
                        self,
                        effects.newtype_concrete_base(self, source_newtype).await?,
                        target,
                    )
                    .await?
            }

            // Note that the definition of `Type::AlwaysFalsy` depends on the return value of `__bool__`.
            // If `__bool__` always returns True or False, it can be treated as a subtype of `AlwaysTruthy` or `AlwaysFalsy`, respectively.
            (_, Type::AlwaysFalsy) => ConstraintSet::from_bool(
                self.constraints,
                effects.type_is_always_falsy(self, source).await?,
            ),
            (_, Type::AlwaysTruthy) => ConstraintSet::from_bool(
                self.constraints,
                effects.type_is_always_truthy(self, source).await?,
            ),
            // Currently, the only supertype of `AlwaysFalsy` and `AlwaysTruthy` is the universal set (object instance).
            (Type::AlwaysFalsy | Type::AlwaysTruthy, _) => {
                effects
                    .guard(self, source, target, || async {
                        Ok({
                            effects
                                .check_type_pair(self, Type::object(), target)
                                .await?
                        })
                    })
                    .await?
            }

            // These clauses handle type variants that include function literals. A function
            // literal is the subtype of itself, and not of any other function literal. However,
            // our representation of a function literal includes any specialization that should be
            // applied to the signature. Different specializations of the same function literal are
            // only subtypes of each other if they result in the same signature.
            (Type::FunctionLiteral(source_function), Type::FunctionLiteral(target_function)) => {
                effects
                    .check_function_pair(self, source_function, target_function)
                    .await?
            }
            (
                Type::KnownInstance(
                    KnownInstanceType::FunctoolsPartial(source_partial)
                    | KnownInstanceType::FunctoolsPartialCall(source_partial),
                ),
                Type::FunctionLiteral(target_function),
            ) if matches!(self.relation, TypeRelation::Assignability) => {
                effects
                    .guard(self, source, target, || async {
                        Ok({
                            effects
                                .check_callable_signature_pair(
                                    self,
                                    effects
                                        .callable_signatures(
                                            self,
                                            effects.partial_callable(source_partial).await?,
                                        )
                                        .await?,
                                    effects
                                        .function_callable_signatures(self, target_function)
                                        .await?,
                                )
                                .await?
                        })
                    })
                    .await?
            }
            (Type::BoundMethod(source_method), Type::BoundMethod(target_method)) => {
                effects
                    .check_bound_method_pair(self, source_method, target_method)
                    .await?
            }
            (Type::KnownBoundMethod(source_method), Type::KnownBoundMethod(target_method)) => {
                effects
                    .check_known_bound_method_pair(self, source_method, target_method)
                    .await?
            }

            // All `StringLiteral` types are a subtype of `LiteralString`.
            (Type::LiteralValue(source), Type::LiteralValue(target))
                if source.is_string() && target.is_literal_string() =>
            {
                self.always()
            }

            // For union simplification, we want to preserve the unpromotable form of a literal value,
            // and so redundancy is not symmetric.
            (Type::LiteralValue(source), Type::LiteralValue(target))
                if matches!(self.relation, TypeRelation::Redundancy { pure: false }) =>
            {
                ConstraintSet::from_bool(
                    self.constraints,
                    source.kind() == target.kind() && source.is_promotable(),
                )
            }

            (Type::LiteralValue(source), Type::LiteralValue(target)) => {
                ConstraintSet::from_bool(self.constraints, source.kind() == target.kind())
            }

            // No literal type is a subtype of any other literal type, unless they are the same
            // type (which is handled above). This case is not necessary from a correctness
            // perspective (the fallback cases below will handle it correctly), but it is important
            // for performance of simplifying large unions of literal types.
            (
                Type::LiteralValue(_)
                | Type::ClassLiteral(_)
                | Type::FunctionLiteral(_)
                | Type::ModuleLiteral(_),
                Type::LiteralValue(_)
                | Type::ClassLiteral(_)
                | Type::FunctionLiteral(_)
                | Type::ModuleLiteral(_),
            ) => self.never(),

            (Type::Callable(source_callable), Type::Callable(target_callable)) => {
                effects
                    .guard(self, source, target, || async {
                        Ok({
                            effects
                                .check_callable_pair(self, source_callable, target_callable)
                                .await?
                        })
                    })
                    .await?
            }

            (
                Type::Callable(source_callable),
                Type::KnownInstance(KnownInstanceType::FunctoolsPartialCall(target_partial)),
            ) if self.relation.is_assignability() => {
                effects
                    .guard(self, source, target, || async {
                        Ok({
                            effects
                                .check_callable_pair(
                                    self,
                                    source_callable,
                                    effects.partial_callable(target_partial).await?,
                                )
                                .await?
                        })
                    })
                    .await?
            }

            (_, Type::Callable(target_callable)) => {
                effects
                    .check_callable_source(self, source, target_callable)
                    .await?
            }

            // `type[Any]` is assignable to arbitrary protocols as it has arbitrary attributes
            // (this is handled by a lower-down branch), but it is only a subtype of a given
            // protocol if `type` is a subtype of that protocol. Similarly, `type[T]` will
            // always be assignable to any protocol if `type[<upper bound of T>]` is assignable
            // to that protocol (handled lower down), but it is only a subtype of that protocol
            // if `type` is a subtype of that protocol.
            (Type::SubclassOf(source_subclass_ty), Type::ProtocolInstance(_))
                if (source_subclass_ty.is_dynamic() || source_subclass_ty.is_type_var())
                    && !self.relation.is_assignability() =>
            {
                effects
                    .check_type_pair(
                        self,
                        effects.known_class_instance(self, KnownClass::Type).await?,
                        target,
                    )
                    .await?
            }

            (_, Type::ProtocolInstance(target_proto)) => {
                effects
                    .guard(self, source, target, || async {
                        Ok({
                            effects
                                .check_type_satisfies_protocol(self, source, target_proto)
                                .await?
                        })
                    })
                    .await?
            }

            // A protocol instance can never be a subtype of a nominal type, with the *sole* exception of `object`.
            (Type::ProtocolInstance(_), _) => self.never(),

            (Type::TypedDict(source_td), Type::TypedDict(target_td)) => {
                effects
                    .guard(self, source, target, || async {
                        Ok({
                            effects
                                .check_typeddict_pair(self, source_td, target_td)
                                .await?
                        })
                    })
                    .await?
            }

            (Type::TypedDict(typed_dict), _) => {
                effects
                    .guard(self, source, target, || async {
                        Ok({
                            effects
                                .check_typeddict_fallback(self, typed_dict, target)
                                .await?
                        })
                    })
                    .await?
            }

            // A non-`TypedDict` cannot subtype a `TypedDict`
            (_, Type::TypedDict(_)) => self.never(),

            // A string literal `Literal["abc"]` is assignable to `str` *and* to
            // `Sequence[Literal["a", "b", "c"]]` because strings are sequences of their characters.
            (Type::LiteralValue(literal), Type::NominalInstance(instance))
                if let Some(value) = literal.as_string() =>
            {
                effects
                    .check_string_literal_nominal(self, value, instance)
                    .await?
            }

            (Type::LiteralValue(literal), _) if literal.is_string() => self.never(),

            // A bytes literal `Literal[b"abc"]` is assignable to `bytes` *and* to
            // `Sequence[Literal[97, 98, 99]]` because bytes are sequences of integers.
            (Type::LiteralValue(literal), Type::NominalInstance(instance))
                if let Some(value) = literal.as_bytes() =>
            {
                effects
                    .check_bytes_literal_nominal(self, value, instance)
                    .await?
            }

            (Type::LiteralValue(literal), _) if literal.is_bytes() => self.never(),

            // An instance is a subtype of an enum literal, if it is an instance of the enum class
            // and the enum has only one member.
            (Type::NominalInstance(_), Type::LiteralValue(literal))
                if let Some(target_enum_literal) = literal.as_enum() =>
            {
                effects
                    .check_enum_instance_literal(self, source, target_enum_literal)
                    .await?
            }

            // Except for the special `BytesLiteral`, `LiteralString`, and string literal cases above,
            // most `Literal` types delegate to their instance fallbacks
            // unless `source` is exactly equivalent to `target` (handled above)
            (Type::ModuleLiteral(_) | Type::LiteralValue(_) | Type::FunctionLiteral(_), _) => {
                effects
                    .literal_fallback_instance(self, source)
                    .await?
                    .when_some_and_with(
                        self.constraints,
                        |source_instance| async move {
                            effects.check_type_pair(self, source_instance, target).await
                        },
                        effects,
                    )
                    .await?
            }

            // The same reasoning applies for these special callable types:
            (Type::BoundMethod(_), _) => {
                effects
                    .check_type_pair(
                        self,
                        effects
                            .known_class_instance(self, KnownClass::MethodType)
                            .await?,
                        target,
                    )
                    .await?
            }
            (Type::KnownBoundMethod(method), _) => {
                effects
                    .check_type_pair(
                        self,
                        effects.known_class_instance(self, method.class()).await?,
                        target,
                    )
                    .await?
            }
            (Type::WrapperDescriptor(_), _) => {
                effects
                    .check_type_pair(
                        self,
                        effects
                            .known_class_instance(self, KnownClass::WrapperDescriptorType)
                            .await?,
                        target,
                    )
                    .await?
            }

            (Type::DataclassDecorator(_) | Type::DataclassTransformer(_), _) => {
                // TODO: Implement subtyping using an equivalent `Callable` type.
                self.never()
            }

            // `TypeIs` is invariant.
            (Type::TypeIs(source), Type::TypeIs(target)) => {
                let source_type = effects.type_is_argument(source).await?;
                let target_type = effects.type_is_argument(target).await?;
                effects
                    .check_type_pair(self, source_type, target_type)
                    .await?
                    .and_with(
                        self.constraints,
                        || async {
                            Ok({
                                effects
                                    .check_type_pair(self, target_type, source_type)
                                    .await?
                            })
                        },
                        effects,
                    )
                    .await?
            }

            // `TypeGuard` is covariant.
            (Type::TypeGuard(source), Type::TypeGuard(target)) => {
                effects
                    .check_type_pair(
                        self,
                        effects.type_guard_return(source).await?,
                        effects.type_guard_return(target).await?,
                    )
                    .await?
            }

            // `TypeIs[T]` and `TypeGuard[T]` are subtypes of `bool`.
            (Type::TypeIs(_) | Type::TypeGuard(_), _) => {
                effects
                    .check_type_pair(
                        self,
                        effects.known_class_instance(self, KnownClass::Bool).await?,
                        target,
                    )
                    .await?
            }

            (Type::Callable(callable), _)
                if let Some(class) = effects.callable_runtime_class(self, callable).await? =>
            {
                effects
                    .check_type_pair(
                        self,
                        effects.known_class_instance(self, class).await?,
                        target,
                    )
                    .await?
            }

            (Type::Callable(_), _) => self.never(),

            (Type::BoundSuper(source), Type::BoundSuper(target)) => {
                effects
                    .check_bound_super_pair(&self.as_equivalence_checker(), source, target)
                    .await?
            }

            (Type::BoundSuper(_), _) => {
                effects
                    .check_type_pair(
                        self,
                        effects
                            .known_class_instance(self, KnownClass::Super)
                            .await?,
                        target,
                    )
                    .await?
            }

            (Type::SubclassOf(subclass_of), _) | (_, Type::SubclassOf(subclass_of))
                if subclass_of.is_type_var() =>
            {
                self.never()
            }

            // `Literal[<class 'C'>]` is a subtype of `type[B]` if `C` is a subclass of `B`,
            // since `type[B]` describes all possible runtime subclasses of the class object `B`.
            (Type::ClassLiteral(source_cls), Type::SubclassOf(target_subclass_ty)) => {
                match target_subclass_ty.subclass_of() {
                    SubclassOfInner::Protocol(target_protocol) => {
                        effects
                            .check_meta_type_satisfies_protocol(
                                self,
                                Type::ClassLiteral(source_cls),
                                target_protocol,
                            )
                            .await?
                    }
                    target => {
                        if let Some(target_cls) = effects.subclass_inner_class(self, target).await?
                        {
                            effects
                                .check_class_pair(
                                    self,
                                    effects
                                        .class_default_specialization(self, source_cls)
                                        .await?,
                                    target_cls,
                                )
                                .await?
                        } else {
                            ConstraintSet::from_bool(
                                self.constraints,
                                self.relation.is_assignability(),
                            )
                        }
                    }
                }
            }

            // Similarly, `<class 'C'>` is assignable to `<class 'C[...]'>` (a generic-alias type)
            // if the default specialization of `C` is assignable to `C[...]`. This scenario occurs
            // with final generic types, where `type[C[...]]` is simplified to the generic-alias
            // type `<class 'C[...]'>`, due to the fact that `C[...]` has no subclasses.
            (Type::ClassLiteral(source_cls), Type::GenericAlias(target_alias)) => {
                effects
                    .check_class_pair(
                        self,
                        effects
                            .class_default_specialization(self, source_cls)
                            .await?,
                        ClassType::Generic(target_alias),
                    )
                    .await?
            }

            // For generic aliases, we delegate to the underlying class type.
            (Type::GenericAlias(source_alias), Type::GenericAlias(target_alias)) => {
                effects
                    .check_class_pair(
                        self,
                        ClassType::Generic(source_alias),
                        ClassType::Generic(target_alias),
                    )
                    .await?
            }

            (Type::GenericAlias(source_alias), Type::SubclassOf(target_subclass_ty)) => {
                match target_subclass_ty.subclass_of() {
                    SubclassOfInner::Protocol(target_protocol) => {
                        effects
                            .check_meta_type_satisfies_protocol(
                                self,
                                Type::GenericAlias(source_alias),
                                target_protocol,
                            )
                            .await?
                    }
                    target => {
                        if let Some(target_cls) = effects.subclass_inner_class(self, target).await?
                        {
                            effects
                                .check_class_pair(
                                    self,
                                    ClassType::Generic(source_alias),
                                    target_cls,
                                )
                                .await?
                        } else {
                            ConstraintSet::from_bool(
                                self.constraints,
                                self.relation.is_assignability(),
                            )
                        }
                    }
                }
            }

            // This branch asks: given two types `type[T]` and `type[S]`, is `type[T]` a subtype of `type[S]`?
            (Type::SubclassOf(source), Type::SubclassOf(target)) => {
                effects.check_subclassof_pair(self, source, target).await?
            }

            // `Literal[str]` is a subtype of `type` because the `str` class object is an instance of its metaclass `type`.
            // `Literal[abc.ABC]` is a subtype of `abc.ABCMeta` because the `abc.ABC` class object
            // is an instance of its metaclass `abc.ABCMeta`.
            (Type::ClassLiteral(source_class), _) => {
                effects
                    .check_type_pair(
                        self,
                        effects
                            .class_literal_metaclass_instance(self, source_class)
                            .await?,
                        target,
                    )
                    .await?
            }
            (Type::GenericAlias(source_alias), _) => {
                effects
                    .check_type_pair(
                        self,
                        effects
                            .class_metaclass_instance(self, ClassType::Generic(source_alias))
                            .await?,
                        target,
                    )
                    .await?
            }

            // `type[Any]` is a subtype of `type[object]`, and is assignable to any `type[...]`
            (Type::SubclassOf(subclass_of_ty), _) if subclass_of_ty.is_dynamic() => {
                effects
                    .check_type_pair(
                        self,
                        effects.known_class_instance(self, KnownClass::Type).await?,
                        target,
                    )
                    .await?
                    .or_with(
                        self.constraints,
                        || async {
                            Ok({
                                ConstraintSet::from_bool(
                                    self.constraints,
                                    self.relation.is_assignability(),
                                )
                                .and_with(
                                    self.constraints,
                                    || async {
                                        Ok({
                                            effects
                                                .check_type_pair(
                                                    self,
                                                    target,
                                                    effects
                                                        .known_class_instance(
                                                            self,
                                                            KnownClass::Type,
                                                        )
                                                        .await?,
                                                )
                                                .await?
                                        })
                                    },
                                    effects,
                                )
                                .await?
                            })
                        },
                        effects,
                    )
                    .await?
            }

            // Any `type[...]` type is assignable to `type[Any]`
            (_, Type::SubclassOf(subclass_of_ty))
                if subclass_of_ty.is_dynamic() && self.relation.is_assignability() =>
            {
                effects
                    .check_type_pair(
                        self,
                        source,
                        effects.known_class_instance(self, KnownClass::Type).await?,
                    )
                    .await?
            }

            // `type[str]` (== `SubclassOf("str")` in ty) describes all possible runtime subclasses
            // of the class object `str`. It is a subtype of `type` (== `Instance("type")`) because `str`
            // is an instance of `type`, and so all possible subclasses of `str` will also be instances of `type`.
            //
            // Similarly `type[enum.Enum]`  is a subtype of `enum.EnumMeta` because `enum.Enum`
            // is an instance of `enum.EnumMeta`. `type[Any]` and `type[Unknown]` do not participate in subtyping,
            // however, as they are not fully static types.
            (Type::SubclassOf(subclass_of_ty), _) => {
                effects
                    .check_type_pair(
                        self,
                        effects
                            .subclass_metaclass_instance(self, subclass_of_ty)
                            .await?,
                        target,
                    )
                    .await?
            }

            (Type::TypeForm(_), _) => {
                effects
                    .check_type_pair(self, Type::object(), target)
                    .await?
            }

            // For example: `Type::SpecialForm(SpecialFormType::Type)` is a subtype of `Type::NominalInstance(_SpecialForm)`,
            // because `Type::SpecialForm(SpecialFormType::Type)` is a set with exactly one runtime value in it
            // (the symbol `typing.Type`), and that symbol is known to be an instance of `typing._SpecialForm` at runtime.
            (Type::SpecialForm(source_form), _) => {
                effects
                    .check_type_pair(
                        self,
                        effects
                            .special_form_instance_fallback(self, source_form)
                            .await?,
                        target,
                    )
                    .await?
            }

            (Type::KnownInstance(source), _) => {
                effects
                    .check_type_pair(
                        self,
                        effects.known_instance_fallback(self, source).await?,
                        target,
                    )
                    .await?
            }

            // `bool` is a subtype of `int`, because `bool` subclasses `int`,
            // which means that all instances of `bool` are also instances of `int`
            (Type::NominalInstance(source_i), Type::NominalInstance(target_i)) => {
                effects
                    .guard(self, source, target, || async {
                        Ok({
                            effects
                                .check_nominal_instance_pair(self, source_i, target_i)
                                .await?
                        })
                    })
                    .await?
            }

            (Type::PropertyInstance(source_p), Type::PropertyInstance(target_p)) => {
                effects
                    .guard(self, source, target, || async {
                        Ok({
                            effects
                                .check_property_instance_pair(self, source_p, target_p)
                                .await?
                        })
                    })
                    .await?
            }

            (Type::PropertyInstance(property), _) => {
                effects
                    .check_type_pair(
                        self,
                        effects.property_instance_fallback(self, property).await?,
                        target,
                    )
                    .await?
            }
            (_, Type::PropertyInstance(property)) => {
                effects
                    .check_type_pair(
                        self,
                        source,
                        effects.property_instance_fallback(self, property).await?,
                    )
                    .await?
            }
            (Type::SlotDescriptor(_), _) => {
                effects
                    .check_type_pair(
                        self,
                        effects
                            .known_class_instance(self, KnownClass::MemberDescriptorType)
                            .await?,
                        target,
                    )
                    .await?
            }
            (_, Type::SlotDescriptor(_)) => {
                effects
                    .check_type_pair(
                        self,
                        source,
                        effects
                            .known_class_instance(self, KnownClass::MemberDescriptorType)
                            .await?,
                    )
                    .await?
            }
            // Other than the special cases enumerated above, nominal-instance types are never
            // subtypes of any other variants
            (Type::NominalInstance(_), _) => self.never(),
        })
    }

    fn check_typeddict_fallback(
        &self,
        db: &'db dyn Db,
        typed_dict: crate::types::TypedDictType<'db>,
        target: Type<'db>,
    ) -> ConstraintSet<'db, 'c> {
        let env = self.env;

        let dict_value_type = typed_dict.dict_value_type_if(db, |field_ty, extra_ty| {
            let result = if self.relation.is_assignability() {
                // Mutual assignability lets gradual field types satisfy the mutable
                // dict contract. Check the schema without inferring type variables or
                // contributing error context, but keep the active recursion guards.
                let checker = Self {
                    inferable: TypeVarSet::None,
                    typevar_evaluation: TypeVarEvaluation::Eager,
                    context_tree: None,
                    ..self.clone()
                };
                checker
                    .check_type_pair(db, field_ty, extra_ty)
                    .and(db, self.constraints, || {
                        checker.check_type_pair(db, extra_ty, field_ty)
                    })
            } else {
                self.as_equivalence_checker()
                    .check_type_pair(db, field_ty, extra_ty)
            };
            result.is_always_satisfied(db, env)
        });
        let fallback = if let Some(value_ty) = dict_value_type {
            KnownClass::Dict.to_specialized_instance(
                db,
                env,
                &[KnownClass::Str.to_instance(db, env), value_ty],
            )
        } else {
            KnownClass::Mapping.to_specialized_instance(
                db,
                env,
                &[
                    KnownClass::Str.to_instance(db, env),
                    typed_dict.value_type(db, env),
                ],
            )
        };
        let result = self.check_type_pair(db, fallback, target);

        if let Some(context) = self.report_context()
            && result.is_never_satisfied(db, env)
            && let Type::NominalInstance(instance) = target
        {
            match instance.class(db, env).known(db) {
                Some(KnownClass::Dict) => {
                    context.push(ErrorContext::TypedDictNotAssignableToDict(typed_dict));
                }
                Some(KnownClass::Mapping) if typed_dict.openness(db).is_implicitly_open() => {
                    let field_types = typed_dict.items(db).values().map(|field| field.declared_ty);
                    let mapping_fallback_spec = &[
                        KnownClass::Str.to_instance(db, env),
                        UnionType::from_elements(db, env, field_types),
                    ];

                    let closed_typeddict_fallback =
                        KnownClass::Mapping.to_specialized_instance(db, env, mapping_fallback_spec);

                    if self
                        .check_type_pair(db, closed_typeddict_fallback, target)
                        .is_always_satisfied(db, env)
                    {
                        let context_element = ErrorContext::OpenTypedDictNotAssignableToMapping {
                            source: typed_dict,
                            target,
                        };
                        context.push(context_element);
                    }
                }
                _ => {}
            }
        }

        result
    }

    pub(super) fn check_property_instance_pair(
        &self,
        db: &'db dyn Db,
        source: PropertyInstanceType<'db>,
        target: PropertyInstanceType<'db>,
    ) -> ConstraintSet<'db, 'c> {
        let env = self.env;
        let check_optional_methods = |source, target| match (source, target) {
            (None, None) => self.always(),
            (Some(source), Some(target)) => self.check_type_pair(db, source, target),
            (None | Some(_), None | Some(_)) => self.never(),
        };

        self.check_type_pair(
            db,
            source.instance_fallback(db, env),
            target.instance_fallback(db, env),
        )
        .and(db, self.constraints, || {
            check_optional_methods(source.getter(db), target.getter(db)).and(
                db,
                self.constraints,
                || {
                    check_optional_methods(source.setter(db), target.setter(db)).and(
                        db,
                        self.constraints,
                        || check_optional_methods(source.deleter(db), target.deleter(db)),
                    )
                },
            )
        })
    }

    pub(super) fn as_equivalence_checker(&self) -> EquivalenceChecker<'a, 'c, 'db> {
        EquivalenceChecker {
            observations: self.observations,
            env: self.env,
            constraints: self.constraints,
            given: self.given,
            perform_expensive_checks: self.perform_expensive_checks,
            typevar_evaluation: TypeVarEvaluation::Eager,
            relation_visitor: self.relation_visitor,
            disjointness_visitor: self.disjointness_visitor,
            signature_relation_visitor: self.signature_relation_visitor,
            materialization_visitor: self.materialization_visitor,
        }
    }

    pub(super) fn as_disjointness_checker(&self) -> DisjointnessChecker<'a, 'c, 'db> {
        DisjointnessChecker {
            env: self.env,
            constraints: self.constraints,
            inferable: self.inferable,
            context_tree: None,
            observations: self.observations,
            given: self.given,
            perform_expensive_checks: self.perform_expensive_checks,
            relation_visitor: self.relation_visitor,
            disjointness_visitor: self.disjointness_visitor,
            signature_relation_visitor: self.signature_relation_visitor,
            materialization_visitor: self.materialization_visitor,
        }
    }

    /// Return `true` if `callable` is the gradual `...` value for a `ParamSpec`.
    ///
    /// For example, in `Command[Any, ..., Any]`, the middle type argument is represented as a
    /// callable-shaped `ParamSpec` value with gradual parameters. That value is assignability-
    /// consistent with a concrete `ParamSpec` specialization such as the middle type argument in
    /// `Command[int, [str], object]`.
    ///
    /// This intentionally does not match arbitrary gradual callables like `Callable[..., object]`
    /// or prefixed gradual forms like `Callable[Concatenate[int, ...], object]`; it only matches
    /// the internal value used to represent a bare `...` `ParamSpec` specialization.
    fn is_gradual_paramspec_value(db: &'db dyn Db, callable: CallableType<'db>) -> bool {
        callable.kind(db) == CallableTypeKind::ParamSpecValue
            && callable
                .signatures(db)
                .iter()
                .all(|signature| signature.parameters().kind() == ParametersKind::Gradual)
    }
}

pub(super) struct EquivalenceChecker<'a, 'c, 'db> {
    observations: Option<&'a RelationObservations<'a, 'db>>,
    env: &'a ProgramEnvironment<'db>,
    pub(super) constraints: &'c ConstraintSetBuilder<'db>,
    given: ConstraintSet<'db, 'c>,
    perform_expensive_checks: bool,
    typevar_evaluation: TypeVarEvaluation,

    // N.B. these fields are private to reduce the risk of
    // "double-visiting" a given pair of types. You should
    // generally only ever call `self.relation_visitor.visit()`
    // or `self.disjointness_visitor.visit()` from
    // `check_type_pair`, never from `check_typeddict_pair` or
    // any other more "low-level" method.
    relation_visitor: &'a HasRelationToVisitor<'db, 'c>,
    disjointness_visitor: &'a IsDisjointVisitor<'db, 'c>,
    signature_relation_visitor: &'a SignatureRelationVisitor<'db>,
    materialization_visitor: &'a ApplyTypeMappingVisitor<'a, 'db>,
}

impl<'owner, 'c, 'db> EquivalenceChecker<'owner, 'c, 'db> {
    fn as_relation_checker<'a>(
        &self,
        materialization_visitor: &'a ApplyTypeMappingVisitor<'a, 'db>,
    ) -> TypeRelationChecker<'a, 'c, 'db>
    where
        'owner: 'a,
    {
        TypeRelationChecker {
            env: self.env,
            relation: TypeRelation::Redundancy { pure: true },
            typevar_evaluation: self.typevar_evaluation,
            constraints: self.constraints,
            context_tree: None,
            observations: self.observations,
            given: self.given,
            perform_expensive_checks: self.perform_expensive_checks,
            inferable: TypeVarSet::None,
            relation_visitor: self.relation_visitor,
            disjointness_visitor: self.disjointness_visitor,
            signature_relation_visitor: self.signature_relation_visitor,
            materialization_visitor,
        }
    }

    pub(super) fn always(&self) -> ConstraintSet<'db, 'c> {
        ConstraintSet::from_bool(self.constraints, true)
    }

    pub(super) fn never(&self) -> ConstraintSet<'db, 'c> {
        ConstraintSet::from_bool(self.constraints, false)
    }

    pub(super) fn check_type_pair(
        &self,
        db: &'db dyn Db,
        left: Type<'db>,
        right: Type<'db>,
    ) -> ConstraintSet<'db, 'c> {
        directional_equivalence_sync(self, left, right, &OrdinaryDirectionalEquivalence { db })
            .unwrap_or_else(|never| match never {})
    }
}

pub(super) struct DisjointnessChecker<'a, 'c, 'db> {
    pub(super) env: &'a ProgramEnvironment<'db>,
    pub(super) constraints: &'c ConstraintSetBuilder<'db>,
    inferable: TypeVarSet<'db>,
    context_tree: Option<ErrorContextTree<'db>>,
    observations: Option<&'a RelationObservations<'a, 'db>>,
    given: ConstraintSet<'db, 'c>,
    perform_expensive_checks: bool,

    // N.B. these fields are private to reduce the risk of
    // "double-visiting" a given pair of types. You should
    // generally only ever call `self.relation_visitor.visit()`
    // or `self.disjointness_visitor.visit()` from
    // `check_type_pair`, never from `check_typeddict_pair` or
    // any other more "low-level" method.
    disjointness_visitor: &'a IsDisjointVisitor<'db, 'c>,
    relation_visitor: &'a HasRelationToVisitor<'db, 'c>,
    signature_relation_visitor: &'a SignatureRelationVisitor<'db>,
    materialization_visitor: &'a ApplyTypeMappingVisitor<'a, 'db>,
}

impl<'a, 'c, 'db> DisjointnessChecker<'a, 'c, 'db> {
    pub(super) fn new(
        env: &'a ProgramEnvironment<'db>,
        constraints: &'c ConstraintSetBuilder<'db>,
        inferable: TypeVarSet<'db>,
        relation_visitor: &'a HasRelationToVisitor<'db, 'c>,
        disjointness_visitor: &'a IsDisjointVisitor<'db, 'c>,
        signature_relation_visitor: &'a SignatureRelationVisitor<'db>,
        materialization_visitor: &'a ApplyTypeMappingVisitor<'a, 'db>,
    ) -> Self {
        Self {
            env,
            constraints,
            inferable,
            context_tree: None,
            observations: None,
            given: ConstraintSet::from_bool(constraints, false),
            perform_expensive_checks: true,
            disjointness_visitor,
            relation_visitor,
            signature_relation_visitor,
            materialization_visitor,
        }
    }

    pub(super) fn as_relation_checker(
        &self,
        relation: TypeRelation,
    ) -> TypeRelationChecker<'a, 'c, 'db> {
        TypeRelationChecker {
            env: self.env,
            relation,
            typevar_evaluation: TypeVarEvaluation::Eager,
            constraints: self.constraints,
            inferable: self.inferable,
            context_tree: None,
            observations: self.observations,
            given: self.given,
            perform_expensive_checks: self.perform_expensive_checks,
            relation_visitor: self.relation_visitor,
            disjointness_visitor: self.disjointness_visitor,
            signature_relation_visitor: self.signature_relation_visitor,
            materialization_visitor: self.materialization_visitor,
        }
    }

    pub(super) fn report_context(&self) -> Option<&ErrorContextTree<'db>> {
        self.context_tree
            .as_ref()
            .filter(|context| context.is_enabled())
    }

    /// Retain a failed subtyping or assignability check that proves disjointness.
    pub(super) fn check_relation_with_context(
        &self,
        db: &'db dyn Db,
        mut checker: TypeRelationChecker<'_, 'c, 'db>,
        check: impl FnOnce(&TypeRelationChecker<'_, 'c, 'db>) -> ConstraintSet<'db, 'c>,
    ) -> ConstraintSet<'db, 'c> {
        checker.context_tree = self
            .report_context()
            .map(|_| ErrorContextTree::new(checker.relation));
        let result = check(&checker);
        if let Some(context) = self.report_context() {
            context.take();
            if result.is_never_satisfied(db, self.env) {
                context.replace(&checker.into_error_context());
            }
        }
        result
    }

    fn as_equivalence_checker(&self) -> EquivalenceChecker<'_, 'c, 'db> {
        EquivalenceChecker {
            observations: self.observations,
            env: self.env,
            constraints: self.constraints,
            given: self.given,
            perform_expensive_checks: self.perform_expensive_checks,
            typevar_evaluation: TypeVarEvaluation::Eager,
            relation_visitor: self.relation_visitor,
            disjointness_visitor: self.disjointness_visitor,
            signature_relation_visitor: self.signature_relation_visitor,
            materialization_visitor: self.materialization_visitor,
        }
    }

    fn with_recursion_guard(
        &self,
        db: &'db dyn Db,
        source: Type<'db>,
        target: Type<'db>,
        work: impl FnOnce() -> ConstraintSet<'db, 'c>,
    ) -> ConstraintSet<'db, 'c> {
        self.disjointness_visitor.visit(db, (source, target), work)
    }

    fn any_protocol_members_absent_or_disjoint(
        &self,
        db: &'db dyn Db,
        protocol: ProtocolInstanceType<'db>,
        other: Type<'db>,
    ) -> ConstraintSet<'db, 'c> {
        let env = self.env;
        protocol
            .interface(db)
            .members(db)
            .when_any(db, self.constraints, |member| {
                if let Some(context) = self.report_context() {
                    context.take();
                }
                let attribute = other
                    .member(db, env, member.name())
                    .place
                    .ignore_possibly_undefined();
                let Some(attribute_type) = attribute else {
                    if let Some(context) = self.report_context() {
                        context.push(ErrorContext::ProtocolMemberNotDefined {
                            member_name: member.name().into(),
                            ty: other,
                        });
                        if let Type::NominalInstance(nominal) = other
                            && nominal.class(db, env).is_final(db)
                        {
                            context.push(ErrorContext::FinalTypeMissingProtocolMembers {
                                final_type: other,
                                protocol: Type::ProtocolInstance(protocol),
                            });
                        }
                    }
                    return self.always();
                };
                let result = self
                    .protocol_member_has_disjoint_type_from_ty(db, &member, attribute_type)
                    .or(db, self.constraints, || {
                        self.protocol_member_write_is_definitely_missing_from_ty(db, &member, other)
                    })
                    .or(db, self.constraints, || {
                        let incompatible =
                            member.has_incompatible_class_variable_declaration(db, env, other);
                        if incompatible && let Some(context) = self.report_context() {
                            context.push(ErrorContext::ProtocolMemberClassVarMismatch {
                                member_name: member.name().into(),
                                ty: other,
                            });
                        }
                        ConstraintSet::from_bool(self.constraints, incompatible)
                    });
                if let Some(context) = self.report_context()
                    && result.is_always_satisfied(db, env)
                {
                    context.push(ErrorContext::ProtocolMemberIncompatible {
                        member_name: member.name().into(),
                    });
                }
                result
            })
    }

    pub(super) fn always(&self) -> ConstraintSet<'db, 'c> {
        ConstraintSet::from_bool(self.constraints, true)
    }

    pub(super) fn never(&self) -> ConstraintSet<'db, 'c> {
        ConstraintSet::from_bool(self.constraints, false)
    }

    #[ty_mapping_probe_macros::dual_relation]
    pub(super) async fn check_type_pair_with<
        E: disjointness_effects::DisjointnessEffects<'a, 'c, 'db>,
    >(
        &self,
        fields: RelationFieldReads<'db>,
        left: Type<'db>,
        right: Type<'db>,
        effects: &E,
    ) -> Result<ConstraintSet<'db, 'c>, E::Error> {
        let _ = fields;
        effects.disjointness_clear_context(self).await?;
        let result = effects.check_type_pair_impl(self, left, right).await?;
        if effects.disjointness_has_context(self).await?
            && !effects.is_always_satisfied(self, result).await?
        {
            // A failed alternative is not evidence for a later successful disjointness check.
            effects.disjointness_clear_context(self).await?;
        }
        Ok(result)
    }

    #[ty_mapping_probe_macros::dual_relation]
    async fn check_type_pair_impl_with<
        E: disjointness_effects::DisjointnessEffects<'a, 'c, 'db>,
    >(
        &self,
        fields: RelationFieldReads<'db>,
        left: Type<'db>,
        right: Type<'db>,
        effects: &E,
    ) -> Result<ConstraintSet<'db, 'c>, E::Error> {
        let _ = fields;
        // The checks below mark which match arms require a non-trivial amount of work
        // to calculate, without sacrificing match guard exhaustiveness checks. If we are not
        // performing expensive checks, then we will conservatively report that the two types are
        // not disjoint.
        if let Some(left) = left.materialized_divergent_fallback() {
            return effects.check_type_pair(self, left, right).await;
        }

        if let Some(right) = right.materialized_divergent_fallback() {
            return effects.check_type_pair(self, left, right).await;
        }

        Ok(match (left, right) {
            (Type::RecursiveVar(_), _) | (_, Type::RecursiveVar(_)) => {
                unreachable!("semantic operation on an unbound recursive variable")
            }
            (Type::Never, _) | (_, Type::Never) => effects.disjointness_boolean(self, true).await?,

            (Type::Dynamic(_), _) | (_, Type::Dynamic(_)) => effects.disjointness_boolean(self, false).await?,
            (Type::Divergent(_), _) | (_, Type::Divergent(_)) => effects.disjointness_boolean(self, false).await?,

            (Type::Recursive(left_recursive), _) => effects.disjointness_left_recursive(self, left, right, left_recursive).await?,

            (_, Type::Recursive(right_recursive)) => effects.disjointness_right_recursive(self, left, right, right_recursive).await?,

            (Type::TypeAlias(alias), _) => if self.perform_expensive_checks { effects.disjointness_left_alias(self, left, right, alias).await? } else { effects.disjointness_boolean(self, false).await? },

            (_, Type::TypeAlias(alias)) => if self.perform_expensive_checks { effects.disjointness_right_alias(self, left, right, alias).await? } else { effects.disjointness_boolean(self, false).await? },

            (Type::EnumComplement(complement), other) => if self.perform_expensive_checks { effects.disjointness_left_enum_complement(self, complement, other).await? } else { effects.disjointness_boolean(self, false).await? },

            (other, Type::EnumComplement(complement)) => if self.perform_expensive_checks { effects.disjointness_right_enum_complement(self, other, complement).await? } else { effects.disjointness_boolean(self, false).await? },

            // `type[T]` and `TypeForm[S]` overlap whenever their represented instance types do.
            (Type::SubclassOf(subclass_of), Type::TypeForm(typeform))
            | (Type::TypeForm(typeform), Type::SubclassOf(subclass_of)) => {
                if self.perform_expensive_checks { effects.disjointness_subclass_typeform(self, subclass_of, typeform).await? } else { effects.disjointness_boolean(self, false).await? }
            }

            // `type[T]` is disjoint from a callable or protocol instance if its upper bound or constraints are.
            (
                Type::SubclassOf(subclass_of),
                other @ (Type::Callable(_) | Type::ProtocolInstance(_)),
            )
            | (
                other @ (Type::Callable(_) | Type::ProtocolInstance(_)),
                Type::SubclassOf(subclass_of),
            ) if let Some(type_var) = effects.disjointness_transposed_typevar(self, subclass_of).await? =>
            {
                if self.perform_expensive_checks { effects.disjointness_typevar_other(self, type_var, other).await? } else { effects.disjointness_boolean(self, false).await? }
            }

            // `type[T]` is disjoint from a class object `A` if every instance of `T` is disjoint from an instance of `A`.
            (Type::SubclassOf(subclass_of), other) | (other, Type::SubclassOf(subclass_of))
                if let Some(type_var) = subclass_of.into_type_var()
                    && let Some(instance) = effects.disjointness_instance_approximation(self, other).await? =>
            {
                if self.perform_expensive_checks { effects.disjointness_typevar_instance(self, type_var, instance).await? } else { effects.disjointness_boolean(self, false).await? }
            }

            // A typevar is never disjoint from itself, since all occurrences of the typevar must
            // be specialized to the same type. (This is an important difference between typevars
            // and `Any`!) Different typevars might be disjoint, depending on their bounds and
            // constraints, which are handled below.
            (Type::TypeVar(left_tvar), Type::TypeVar(right_tvar))
                if !effects.disjointness_typevar_is_inferable(self, left_tvar).await?
                    && effects.disjointness_same_typevar(self, left_tvar, right_tvar).await? =>
            {
                effects.disjointness_boolean(self, false).await?
            }

            (Type::TypeVar(tvar), Type::Intersection(intersection))
            | (Type::Intersection(intersection), Type::TypeVar(tvar))
                if !effects.disjointness_typevar_is_inferable(self, tvar).await?
                    && effects.disjointness_negative_contains_typevar(self, intersection, tvar).await? =>
            {
                effects.disjointness_boolean(self, true).await?
            }

            // An unbounded typevar is never disjoint from any other type, since it might be
            // specialized to any type. A bounded typevar is not disjoint from its bound, and is
            // only disjoint from other types if its bound is. A constrained typevar is disjoint
            // from a type if all of its constraints are.
            (Type::TypeVar(tvar), other) | (other, Type::TypeVar(tvar))
                if !effects.disjointness_typevar_is_inferable(self, tvar).await? =>
            {
                if self.perform_expensive_checks { effects.disjointness_typevar_bounds(self, tvar, other).await? } else { effects.disjointness_boolean(self, false).await? }
            }

            // TODO: Infer specializations here
            (Type::TypeVar(_), _) | (_, Type::TypeVar(_)) => effects.disjointness_boolean(self, false).await?,

            (Type::Union(union), other) | (other, Type::Union(union)) => {
                if self.perform_expensive_checks { effects.disjointness_union(self, union, other).await? } else { effects.disjointness_boolean(self, false).await? }
            }

            // If we have two intersections, we test the positive elements of each one against the other intersection
            // Negative elements need a positive element on the other side in order to be disjoint.
            // This is similar to what would happen if we tried to build a new intersection that combines the two
            (Type::Intersection(left_intersection), Type::Intersection(right_intersection)) => {
                if self.perform_expensive_checks { effects.disjointness_intersections(self, left, right, left_intersection, right_intersection).await? } else { effects.disjointness_boolean(self, false).await? }
            }

            (Type::Intersection(intersection), other) => if self.perform_expensive_checks { effects.disjointness_left_intersection(self, left, right, intersection, other).await? } else { effects.disjointness_boolean(self, false).await? },

            (other, Type::Intersection(intersection)) => if self.perform_expensive_checks { effects.disjointness_right_intersection(self, left, right, intersection, other).await? } else { effects.disjointness_boolean(self, false).await? },

            (Type::LiteralValue(left), Type::LiteralValue(right))
                if left.is_literal_string() && right.is_literal_string()
                    || (left.is_string() && right.is_literal_string())
                    || (left.is_literal_string() && right.is_string()) =>
            {
                effects.disjointness_boolean(self, false).await?
            }

            (Type::LiteralValue(left), Type::LiteralValue(right)) => {
                if let (Some(left), Some(right)) = (left.as_enum(), right.as_enum())
                    && effects.disjointness_enum_class(self, left).await? == effects.disjointness_enum_class(self, right).await?
                    && !effects.disjointness_enum_aliases_known(self, effects.disjointness_enum_class(self, left).await?).await?
                {
                    effects.disjointness_boolean(self, false).await?
                } else {
                    effects.disjointness_boolean(self, effects.disjointness_literal_kinds_differ(self, left, right).await?).await?
                }
            }

            (Type::PropertyInstance(left), Type::PropertyInstance(right)) => {
                if self.perform_expensive_checks { effects.check_property_instance_pair(self, left, right).await? } else { effects.disjointness_boolean(self, false).await? }
            }

            (
                Type::KnownBoundMethod(KnownBoundMethodType::PropertyDunderGet(left)),
                Type::KnownBoundMethod(KnownBoundMethodType::PropertyDunderGet(right)),
            )
            | (
                Type::KnownBoundMethod(KnownBoundMethodType::PropertyDunderSet(left)),
                Type::KnownBoundMethod(KnownBoundMethodType::PropertyDunderSet(right)),
            )
            | (
                Type::KnownBoundMethod(KnownBoundMethodType::PropertyDunderDelete(left)),
                Type::KnownBoundMethod(KnownBoundMethodType::PropertyDunderDelete(right)),
            ) => if self.perform_expensive_checks { effects.check_property_instance_pair(self, left, right).await? } else { effects.disjointness_boolean(self, false).await? },

            (
                Type::KnownBoundMethod(KnownBoundMethodType::FunctionTypeDunderGet(left)),
                Type::KnownBoundMethod(KnownBoundMethodType::FunctionTypeDunderGet(right)),
            )
            | (
                Type::KnownBoundMethod(KnownBoundMethodType::DunderCall(left)),
                Type::KnownBoundMethod(KnownBoundMethodType::DunderCall(right)),
            ) => if self.perform_expensive_checks { effects.disjointness_interned_pair(self, left, right).await? } else { effects.disjointness_boolean(self, false).await? },

            (
                Type::KnownBoundMethod(KnownBoundMethodType::MethodTypeDunderGet(left)),
                Type::KnownBoundMethod(KnownBoundMethodType::MethodTypeDunderGet(right)),
            ) => if self.perform_expensive_checks { effects.disjointness_method_pair(self, left, right).await? } else { effects.disjointness_boolean(self, false).await? },

            (
                Type::KnownInstance(KnownInstanceType::Sentinel(left_sentinel)),
                Type::KnownInstance(KnownInstanceType::Sentinel(right_sentinel)),
            ) => effects.disjointness_boolean(self,
                !effects.disjointness_same_sentinel(self, left_sentinel, right_sentinel).await?,
            ).await?,

            // Distinct wrapper types can describe the same descriptor when their wrapped types
            // overlap; they do not necessarily represent distinct objects.
            (
                Type::KnownInstance(KnownInstanceType::MethodWrapper(left_wrapper)),
                Type::KnownInstance(KnownInstanceType::MethodWrapper(right_wrapper)),
            ) if effects.disjointness_same_wrapper_kind(self, left_wrapper, right_wrapper).await? => if self.perform_expensive_checks { effects.disjointness_wrappers(self, left, right, left_wrapper, right_wrapper).await? } else { effects.disjointness_boolean(self, false).await? },

            (
                Type::KnownInstance(KnownInstanceType::FunctoolsPartial(left_partial)),
                Type::KnownInstance(KnownInstanceType::FunctoolsPartial(right_partial)),
            )
            | (
                Type::KnownInstance(KnownInstanceType::FunctoolsPartialCall(left_partial)),
                Type::KnownInstance(KnownInstanceType::FunctoolsPartialCall(right_partial)),
            ) => if self.perform_expensive_checks { effects.disjointness_partials(self, left, right, left_partial, right_partial).await? } else { effects.disjointness_boolean(self, false).await? },

            // These types are disjoint whenever their represented objects differ.
            (
                // `LiteralString` can represent different strings and is handled above.
                left @ (Type::FunctionLiteral(..)
                | Type::KnownBoundMethod(..)
                | Type::WrapperDescriptor(..)
                | Type::ModuleLiteral(..)
                | Type::ClassLiteral(..)
                | Type::SpecialForm(..)
                | Type::KnownInstance(..)),
                right @ (Type::FunctionLiteral(..)
                | Type::KnownBoundMethod(..)
                | Type::WrapperDescriptor(..)
                | Type::ModuleLiteral(..)
                | Type::ClassLiteral(..)
                | Type::SpecialForm(..)
                | Type::KnownInstance(..)),
            ) => effects.disjointness_boolean(self, effects.disjointness_types_differ(self, left, right).await?).await?,

            (
                Type::SubclassOf(_),
                Type::LiteralValue(..)
                | Type::FunctionLiteral(..)
                | Type::BoundMethod(..)
                | Type::KnownBoundMethod(..)
                | Type::WrapperDescriptor(..)
                | Type::ModuleLiteral(..),
            )
            | (
                Type::LiteralValue(..)
                | Type::FunctionLiteral(..)
                | Type::BoundMethod(..)
                | Type::KnownBoundMethod(..)
                | Type::WrapperDescriptor(..)
                | Type::ModuleLiteral(..),
                Type::SubclassOf(_),
            ) => effects.disjointness_boolean(self, true).await?,

            (Type::AlwaysTruthy, ty) | (ty, Type::AlwaysTruthy) => {
                // `Truthiness::Ambiguous` may include `AlwaysTrue` as a subset, so it's not guaranteed to be disjoint.
                // Thus, they are only disjoint if `ty.bool() == AlwaysFalse`.
                if self.perform_expensive_checks { effects.disjointness_boolean(self, effects.type_is_always_falsy(self, ty).await?).await? } else { effects.disjointness_boolean(self, false).await? }
            }
            (Type::AlwaysFalsy, ty) | (ty, Type::AlwaysFalsy) => {
                // Similarly, they are only disjoint if `ty.bool() == AlwaysTrue`.
                if self.perform_expensive_checks { effects.disjointness_boolean(self, effects.type_is_always_truthy(self, ty).await?).await? } else { effects.disjointness_boolean(self, false).await? }
            }

            (Type::ProtocolInstance(left_proto), Type::ProtocolInstance(right_proto)) => {
                if self.perform_expensive_checks { effects.disjointness_protocols(self, left, right, left_proto, right_proto).await? } else { effects.disjointness_boolean(self, false).await? }
            }

            (Type::ProtocolInstance(protocol), Type::SpecialForm(special_form))
            | (Type::SpecialForm(special_form), Type::ProtocolInstance(protocol)) => {
                if self.perform_expensive_checks { effects.disjointness_protocol_special_form(self, left, right, protocol, special_form).await? } else { effects.disjointness_boolean(self, false).await? }
            }

            (Type::ProtocolInstance(protocol), Type::KnownInstance(known_instance))
            | (Type::KnownInstance(known_instance), Type::ProtocolInstance(protocol)) => {
                if self.perform_expensive_checks { effects.disjointness_protocol_known_instance(self, left, right, protocol, known_instance).await? } else { effects.disjointness_boolean(self, false).await? }
            }

            // The absence of a protocol member on one of these types guarantees
            // that the type will be disjoint from the protocol,
            // but the type will not be disjoint from the protocol if it has a member
            // that is of the correct type but is possibly unbound.
            // If accessing a member on this type returns a possibly unbound `Place`,
            // the type will not be a subtype of the protocol but it will also not be
            // disjoint from the protocol, since there are possible subtypes of the type
            // that could satisfy the protocol.
            //
            // ```py
            // class Foo:
            //     if coinflip():
            //         X = 42
            //
            // class HasX(Protocol):
            //     @property
            //     def x(self) -> int: ...
            //
            // # `TypeOf[Foo]` (a class-literal type) is not a subtype of `HasX`,
            // # but `TypeOf[Foo]` & HasX` should not simplify to `Never`,
            // # or this branch would be incorrectly understood to be unreachable,
            // # since we would understand the type of `Foo` in this branch to be
            // # `TypeOf[Foo] & HasX` due to `hasattr()` narrowing.
            //
            // if hasattr(Foo, "X"):
            //     print(Foo.X)
            // ```
            (
                ty @ (Type::LiteralValue(..)
                | Type::ClassLiteral(..)
                | Type::FunctionLiteral(..)
                | Type::ModuleLiteral(..)
                | Type::GenericAlias(..)),
                Type::ProtocolInstance(protocol),
            )
            | (
                Type::ProtocolInstance(protocol),
                ty @ (Type::LiteralValue(..)
                | Type::ClassLiteral(..)
                | Type::FunctionLiteral(..)
                | Type::ModuleLiteral(..)
                | Type::GenericAlias(..)),
            ) => if self.perform_expensive_checks { effects.disjointness_protocol_members(self, left, right, protocol, ty).await? } else { effects.disjointness_boolean(self, false).await? },

            // This is the same as the branch above --
            // once guard patterns are stabilised, it could be unified with that branch
            // (<https://github.com/rust-lang/rust/issues/129967>)
            (Type::ProtocolInstance(protocol), Type::NominalInstance(nominal))
            | (Type::NominalInstance(nominal), Type::ProtocolInstance(protocol))
                if self.perform_expensive_checks && effects.disjointness_nominal_is_final(self, nominal).await? =>
            {
                if self.perform_expensive_checks { effects.disjointness_protocol_nominal(self, left, right, protocol, nominal).await? } else { effects.disjointness_boolean(self, false).await? }
            }

            (Type::ProtocolInstance(protocol), other)
            | (other, Type::ProtocolInstance(protocol)) => if self.perform_expensive_checks { effects.disjointness_protocol_other(self, left, right, protocol, other).await? } else { effects.disjointness_boolean(self, false).await? },

            (Type::SubclassOf(subclass_of_ty), _) | (_, Type::SubclassOf(subclass_of_ty))
                if subclass_of_ty.is_type_var() =>
            {
                effects.disjointness_boolean(self, true).await?
            }

            (Type::GenericAlias(left_alias), Type::GenericAlias(right_alias)) => {
                effects.or(self, effects.disjointness_boolean(
                    self,
                    effects.disjointness_alias_origin(self, left_alias).await? != effects.disjointness_alias_origin(self, right_alias).await?,
                ).await?, || async { Ok(if self.perform_expensive_checks { effects.disjointness_alias_specializations(self, left_alias, right_alias).await? } else { effects.disjointness_boolean(self, false).await? }) }).await?
            }

            (Type::ClassLiteral(class), Type::GenericAlias(alias_b))
            | (Type::GenericAlias(alias_b), Type::ClassLiteral(class)) => {
                if self.perform_expensive_checks { effects.disjointness_class_alias(self, class, alias_b).await? } else { effects.disjointness_boolean(self, false).await? }
            }

            (Type::SubclassOf(subclass_of_ty), Type::ClassLiteral(class_b))
            | (Type::ClassLiteral(class_b), Type::SubclassOf(subclass_of_ty)) => {
                match subclass_of_ty.subclass_of() {
                    SubclassOfInner::Dynamic(_) => effects.disjointness_boolean(self, false).await?,
                    SubclassOfInner::Protocol(_) => effects.disjointness_boolean(self, false).await?,
                    SubclassOfInner::Class(class_a) => if self.perform_expensive_checks { effects.disjointness_subclass_class(self, class_a, class_b).await? } else { effects.disjointness_boolean(self, false).await? },
                    SubclassOfInner::TypeVar(_) => unreachable!(),
                }
            }

            (Type::SubclassOf(subclass_of_ty), Type::GenericAlias(alias_b))
            | (Type::GenericAlias(alias_b), Type::SubclassOf(subclass_of_ty)) => {
                match subclass_of_ty.subclass_of() {
                    SubclassOfInner::Dynamic(_) => effects.disjointness_boolean(self, false).await?,
                    SubclassOfInner::Protocol(_) => effects.disjointness_boolean(self, false).await?,
                    SubclassOfInner::Class(class_a) => if self.perform_expensive_checks { effects.disjointness_subclass_alias(self, class_a, alias_b).await? } else { effects.disjointness_boolean(self, false).await? },
                    SubclassOfInner::TypeVar(_) => unreachable!(),
                }
            }

            (Type::SubclassOf(left), Type::SubclassOf(right)) => {
                if self.perform_expensive_checks { effects.check_subclassof_pair(self, left, right).await? } else { effects.disjointness_boolean(self, false).await? }
            }

            // for `type[Any]`/`type[Unknown]`/`type[Todo]`, we know the type cannot be any larger than `type`,
            // so although the type is dynamic we can still determine disjointedness in some situations
            (Type::SubclassOf(subclass_of_ty), other)
            | (other, Type::SubclassOf(subclass_of_ty)) => {
                if self.perform_expensive_checks { effects.disjointness_subclass_other(self, subclass_of_ty, other).await? } else { effects.disjointness_boolean(self, false).await? }
            }

            (Type::SpecialForm(special_form), Type::NominalInstance(instance))
            | (Type::NominalInstance(instance), Type::SpecialForm(special_form)) => {
                if self.perform_expensive_checks { effects.disjointness_special_form_nominal(self, special_form, instance).await? } else { effects.disjointness_boolean(self, false).await? }
            }

            (Type::KnownInstance(known_instance), Type::NominalInstance(instance))
            | (Type::NominalInstance(instance), Type::KnownInstance(known_instance)) => {
                if self.perform_expensive_checks { effects.disjointness_known_instance_nominal(self, known_instance, instance).await? } else { effects.disjointness_boolean(self, false).await? }
            }

            (Type::LiteralValue(literal), Type::NominalInstance(instance))
            | (Type::NominalInstance(instance), Type::LiteralValue(literal)) => {
                if self.perform_expensive_checks { effects.disjointness_literal_nominal(self, literal, instance).await? } else { effects.disjointness_boolean(self, false).await? }
            }

            (Type::TypeIs(_) | Type::TypeGuard(_), Type::LiteralValue(literal))
            | (Type::LiteralValue(literal), Type::TypeIs(_) | Type::TypeGuard(_)) => {
                effects.disjointness_boolean(self, !literal.is_bool()).await?
            }

            (Type::TypeIs(_) | Type::TypeGuard(_), Type::NominalInstance(instance))
            | (Type::NominalInstance(instance), Type::TypeIs(_) | Type::TypeGuard(_)) => {
                // A boolean literal must be an instance of exactly `bool`
                // (it cannot be an instance of a `bool` subclass)
                if self.perform_expensive_checks { effects.disjointness_bool_nominal(self, instance).await? } else { effects.disjointness_boolean(self, false).await? }
            }

            (
                Type::NewTypeInstance(newtype),
                other @ (Type::LiteralValue(_) | Type::TypeIs(_) | Type::TypeGuard(_)),
            )
            | (
                other @ (Type::LiteralValue(_) | Type::TypeIs(_) | Type::TypeGuard(_)),
                Type::NewTypeInstance(newtype),
            ) => if self.perform_expensive_checks { effects.disjointness_newtype_other(self, newtype, other).await? } else { effects.disjointness_boolean(self, false).await? },

            (Type::TypeIs(_) | Type::TypeGuard(_), _)
            | (_, Type::TypeIs(_) | Type::TypeGuard(_)) => effects.disjointness_boolean(self, true).await?,

            (Type::LiteralValue(_), _) | (_, Type::LiteralValue(_)) => effects.disjointness_boolean(self, true).await?,

            // A class-literal type `X` is always disjoint from an instance type `Y`,
            // unless the type expressing "all instances of `Z`" is a subtype of of `Y`,
            // where `Z` is `X`'s metaclass.
            (Type::ClassLiteral(class), Type::NominalInstance(instance))
            | (Type::NominalInstance(instance), Type::ClassLiteral(class)) => {
                if self.perform_expensive_checks { effects.disjointness_class_nominal(self, class, instance).await? } else { effects.disjointness_boolean(self, false).await? }
            }

            (Type::GenericAlias(alias), Type::NominalInstance(instance))
            | (Type::NominalInstance(instance), Type::GenericAlias(alias)) => {
                if self.perform_expensive_checks { effects.disjointness_alias_nominal(self, alias, instance).await? } else { effects.disjointness_boolean(self, false).await? }
            }

            (Type::FunctionLiteral(function), Type::NominalInstance(instance))
            | (Type::NominalInstance(instance), Type::FunctionLiteral(function)) => {
                // Function literals and their descriptor wrappers have an exact runtime class.
                if self.perform_expensive_checks { effects.disjointness_function_nominal(self, function, instance).await? } else { effects.disjointness_boolean(self, false).await? }
            }

            (Type::Callable(callable), other) | (other, Type::Callable(callable))
                if let Some(class) = effects.disjointness_callable_runtime_class(self, callable).await? =>
            {
                let other = match other {
                    Type::Callable(other_callable) => {
                        let Some(other_class) = effects.disjointness_callable_runtime_class(self, other_callable).await? else {
                            return effects.disjointness_boolean(self, false).await;
                        };
                        effects.disjointness_known_instance(self, other_class).await?
                    }
                    _ => other,
                };
                if self.perform_expensive_checks { effects.disjointness_callable_other(self, class, other).await? } else { effects.disjointness_boolean(self, false).await? }
            }

            // A `BoundMethod` type includes instances of the same method bound to a
            // subtype/subclass of the self type.
            (Type::BoundMethod(a), Type::BoundMethod(b)) => {
                let (Some(a_function), Some(b_function)) = (effects.disjointness_bound_function(self, a).await?, effects.disjointness_bound_function(self, b).await?) else {
                    return Ok(if self.perform_expensive_checks { effects.disjointness_bound_method_fallback(self, a, b).await? } else { effects.disjointness_boolean(self, false).await? });
                };
                if effects.disjointness_function_names_differ(self, a_function, b_function).await? {
                    // We typically ask about `BoundMethod` disjointness when we're looking at a
                    // method call on an intersection type like `A & B`. In that case, the same
                    // method name would show up on both sides of this check. However for
                    // completeness, if we're ever comparing `BoundMethod` types with different
                    // method names, then they're clearly disjoint.
                    return effects.disjointness_boolean(self, true).await;
                }

                if self.perform_expensive_checks { effects.disjointness_bound_method_functions(self, a, b, a_function, b_function).await? } else { effects.disjointness_boolean(self, false).await? }
            }

            (Type::BoundMethod(_), other) | (other, Type::BoundMethod(_)) => {
                if self.perform_expensive_checks { effects.disjointness_bound_method_other(self, other).await? } else { effects.disjointness_boolean(self, false).await? }
            }

            (Type::KnownBoundMethod(method), other) | (other, Type::KnownBoundMethod(method)) => {
                if self.perform_expensive_checks { effects.disjointness_known_method_other(self, method, other).await? } else { effects.disjointness_boolean(self, false).await? }
            }

            (Type::WrapperDescriptor(_), other) | (other, Type::WrapperDescriptor(_)) => {
                if self.perform_expensive_checks { effects.disjointness_descriptor_other(self, other).await? } else { effects.disjointness_boolean(self, false).await? }
            }

            (Type::Callable(_) | Type::FunctionLiteral(_), Type::Callable(_))
            | (Type::Callable(_), Type::FunctionLiteral(_)) => {
                // No two callable types are ever disjoint because
                // `(*args: object, **kwargs: object) -> Never` is a subtype of all fully static
                // callable types.
                effects.disjointness_boolean(self, false).await?
            }

            (Type::Callable(_), Type::SpecialForm(special_form))
            | (Type::SpecialForm(special_form), Type::Callable(_)) => {
                // A callable type is disjoint from special form types, except for special forms
                // that are callable (like TypedDict and collection constructors).
                // Most special forms are type constructors/annotations (like `typing.Literal`,
                // `typing.Union`, etc.) that are subscripted, not called.
                effects.disjointness_boolean(self, !special_form.is_callable()).await?
            }

            (
                Type::Callable(_) | Type::DataclassDecorator(_) | Type::DataclassTransformer(_),
                Type::NominalInstance(nominal),
            )
            | (
                Type::NominalInstance(nominal),
                Type::Callable(_) | Type::DataclassDecorator(_) | Type::DataclassTransformer(_),
            ) if self.perform_expensive_checks && effects.disjointness_nominal_is_final(self, nominal).await? => {
                if self.perform_expensive_checks { effects.disjointness_callable_final_nominal(self, nominal).await? } else { effects.disjointness_boolean(self, false).await? }
            }

            (
                Type::Callable(_) | Type::DataclassDecorator(_) | Type::DataclassTransformer(_),
                _,
            )
            | (
                _,
                Type::Callable(_) | Type::DataclassDecorator(_) | Type::DataclassTransformer(_),
            ) => {
                // TODO: Implement disjointness for general callable type with other types
                effects.disjointness_boolean(self, false).await?
            }

            (Type::ModuleLiteral(..), Type::NominalInstance(instance))
            | (Type::NominalInstance(instance), Type::ModuleLiteral(..)) => {
                // Modules *can* actually be instances of `ModuleType` subclasses
                if self.perform_expensive_checks { effects.disjointness_module_nominal(self, instance).await? } else { effects.disjointness_boolean(self, false).await? }
            }

            (Type::NominalInstance(left_i), Type::NominalInstance(right_i)) => {
                if self.perform_expensive_checks { effects.disjointness_nominal_pair(self, left, right, left_i, right_i).await? } else { effects.disjointness_boolean(self, false).await? }
            }

            (Type::NewTypeInstance(left), Type::NewTypeInstance(right)) => {
                if self.perform_expensive_checks { effects.check_newtype_pair(self, left, right).await? } else { effects.disjointness_boolean(self, false).await? }
            }
            (Type::NewTypeInstance(newtype), other) | (other, Type::NewTypeInstance(newtype)) => {
                if self.perform_expensive_checks { effects.disjointness_newtype_other(self, newtype, other).await? } else { effects.disjointness_boolean(self, false).await? }
            }

            (Type::PropertyInstance(property), other)
            | (other, Type::PropertyInstance(property)) => if self.perform_expensive_checks { effects.disjointness_property_other(self, property, other).await? } else { effects.disjointness_boolean(self, false).await? },

            (Type::SlotDescriptor(_), other) | (other, Type::SlotDescriptor(_)) => {
                if self.perform_expensive_checks { effects.disjointness_slot_other(self, other).await? } else { effects.disjointness_boolean(self, false).await? }
            }

            (Type::BoundSuper(left), Type::BoundSuper(right)) => if self.perform_expensive_checks { effects.disjointness_bound_super_pair(self, left, right).await? } else { effects.disjointness_boolean(self, false).await? },

            (Type::BoundSuper(_), other) | (other, Type::BoundSuper(_)) => {
                if self.perform_expensive_checks { effects.disjointness_super_other(self, other).await? } else { effects.disjointness_boolean(self, false).await? }
            }

            (Type::TypeForm(_), _) | (_, Type::TypeForm(_)) => effects.disjointness_boolean(self, false).await?,

            (Type::GenericAlias(_), _) | (_, Type::GenericAlias(_)) => effects.disjointness_boolean(self, true).await?,

            (Type::TypedDict(left_td), Type::TypedDict(right_td)) => if self.perform_expensive_checks { effects.disjointness_typeddicts(self, left, right, left_td, right_td).await? } else { effects.disjointness_boolean(self, false).await? },

            // For any type `T`, if `dict[str, Any]` is not assignable to `T`, then all `TypedDict`
            // types will always be disjoint from `T`. This doesn't cover all cases -- in fact
            // `dict` *itself* is almost always disjoint from `TypedDict` -- but it's a good
            // approximation, and some false negatives are acceptable.
            (Type::TypedDict(_), other) | (other, Type::TypedDict(_)) => {
                if self.perform_expensive_checks { effects.disjointness_typeddict_other(self, other).await? } else { effects.disjointness_boolean(self, false).await? }
            }
        })
    }

    fn disjointness_left_alias(
        &self,
        db: &'db dyn Db,
        left: Type<'db>,
        right: Type<'db>,
        alias: TypeAliasType<'db>,
    ) -> ConstraintSet<'db, 'c> {
        let left_alias_ty = alias.value_type(db);
        self.with_recursion_guard(db, left, right, || {
            self.check_type_pair(db, left_alias_ty, right)
        })
    }

    fn disjointness_right_alias(
        &self,
        db: &'db dyn Db,
        left: Type<'db>,
        right: Type<'db>,
        alias: TypeAliasType<'db>,
    ) -> ConstraintSet<'db, 'c> {
        let right_alias_ty = alias.value_type(db);
        self.with_recursion_guard(db, left, right, || {
            self.check_type_pair(db, left, right_alias_ty)
        })
    }

    fn disjointness_left_enum_complement(
        &self,
        db: &'db dyn Db,
        complement: EnumComplementType<'db>,
        other: Type<'db>,
    ) -> ConstraintSet<'db, 'c> {
        let env = self.env;
        self.check_type_pair(db, complement.remaining_literal_union(db, env), other)
    }

    fn disjointness_right_enum_complement(
        &self,
        db: &'db dyn Db,
        other: Type<'db>,
        complement: EnumComplementType<'db>,
    ) -> ConstraintSet<'db, 'c> {
        let env = self.env;
        self.check_type_pair(db, other, complement.remaining_literal_union(db, env))
    }

    fn disjointness_subclass_typeform(
        &self,
        db: &'db dyn Db,
        subclass_of: SubclassOfType<'db>,
        typeform: TypeFormType<'db>,
    ) -> ConstraintSet<'db, 'c> {
        let env = self.env;
        self.check_type_pair(
            db,
            subclass_of.to_instance(db, env),
            typeform.type_argument(db),
        )
    }

    fn disjointness_typevar_other(
        &self,
        db: &'db dyn Db,
        type_var: BoundTypeVarInstance<'db>,
        other: Type<'db>,
    ) -> ConstraintSet<'db, 'c> {
        self.check_type_pair(db, Type::TypeVar(type_var), other)
    }

    fn disjointness_typevar_instance(
        &self,
        db: &'db dyn Db,
        type_var: BoundTypeVarInstance<'db>,
        instance: Type<'db>,
    ) -> ConstraintSet<'db, 'c> {
        self.check_type_pair(db, Type::TypeVar(type_var), instance)
    }

    fn disjointness_typevar_bounds(
        &self,
        db: &'db dyn Db,
        tvar: BoundTypeVarInstance<'db>,
        other: Type<'db>,
    ) -> ConstraintSet<'db, 'c> {
        let env = self.env;
        match tvar.typevar(db).bound_or_constraints(db, env) {
            None => self.never(),
            Some(TypeVarBoundOrConstraints::UpperBound(bound)) => {
                self.check_type_pair(db, bound, other)
            }
            Some(TypeVarBoundOrConstraints::Constraints(typevar_constraints)) => {
                typevar_constraints.elements(db).iter().when_all(
                    db,
                    self.constraints,
                    |constraint| self.check_type_pair(db, *constraint, other),
                )
            }
        }
    }

    fn disjointness_union(
        &self,
        db: &'db dyn Db,
        union: UnionType<'db>,
        other: Type<'db>,
    ) -> ConstraintSet<'db, 'c> {
        let env = self.env;
        let mut children = Vec::new();
        let result = union
            .elements(db)
            .iter()
            .when_all(db, self.constraints, |e| {
                let result = self.check_type_pair(db, *e, other);
                if let Some(context) = self.report_context() {
                    if context.is_empty() {
                        context.push(ErrorContext::DisjointTypes {
                            left: *e,
                            right: other,
                        });
                    }
                    children.push(context.take());
                }
                result
            });
        if let Some(context) = self.report_context()
            && result.is_always_satisfied(db, env)
        {
            context.set(
                ErrorContext::DisjointUnion {
                    union: Type::Union(union),
                    other,
                },
                children,
            );
        }
        result
    }

    fn disjointness_intersections(
        &self,
        db: &'db dyn Db,
        left: Type<'db>,
        right: Type<'db>,
        left_intersection: IntersectionType<'db>,
        right_intersection: IntersectionType<'db>,
    ) -> ConstraintSet<'db, 'c> {
        disjoint_intersection::check_disjoint_intersection_sync(
            left,
            right,
            disjoint_intersection::DisjointIntersectionOperands::Both {
                left: left_intersection,
                right: right_intersection,
            },
            &disjoint_intersection::InlineDisjointIntersectionEffects::new(db, self),
        )
        .unwrap_or_else(|never| match never {})
    }

    fn disjointness_left_intersection(
        &self,
        db: &'db dyn Db,
        left: Type<'db>,
        right: Type<'db>,
        intersection: IntersectionType<'db>,
        other: Type<'db>,
    ) -> ConstraintSet<'db, 'c> {
        disjoint_intersection::check_disjoint_intersection_sync(
            left,
            right,
            disjoint_intersection::DisjointIntersectionOperands::Left {
                intersection,
                other,
            },
            &disjoint_intersection::InlineDisjointIntersectionEffects::new(db, self),
        )
        .unwrap_or_else(|never| match never {})
    }

    fn disjointness_right_intersection(
        &self,
        db: &'db dyn Db,
        left: Type<'db>,
        right: Type<'db>,
        intersection: IntersectionType<'db>,
        other: Type<'db>,
    ) -> ConstraintSet<'db, 'c> {
        disjoint_intersection::check_disjoint_intersection_sync(
            left,
            right,
            disjoint_intersection::DisjointIntersectionOperands::Right {
                intersection,
                other,
            },
            &disjoint_intersection::InlineDisjointIntersectionEffects::new(db, self),
        )
        .unwrap_or_else(|never| match never {})
    }

    fn disjointness_interned_pair(
        &self,
        db: &'db dyn Db,
        left: InternedType<'db>,
        right: InternedType<'db>,
    ) -> ConstraintSet<'db, 'c> {
        self.check_type_pair(db, left.inner(db), right.inner(db))
    }

    fn disjointness_method_pair(
        &self,
        db: &'db dyn Db,
        left: BoundMethodType<'db>,
        right: BoundMethodType<'db>,
    ) -> ConstraintSet<'db, 'c> {
        self.check_type_pair(db, Type::BoundMethod(left), Type::BoundMethod(right))
    }

    fn disjointness_wrappers(
        &self,
        db: &'db dyn Db,
        left: Type<'db>,
        right: Type<'db>,
        left_wrapper: MethodWrapper<'db>,
        right_wrapper: MethodWrapper<'db>,
    ) -> ConstraintSet<'db, 'c> {
        self.with_recursion_guard(db, left, right, || {
            self.check_type_pair(db, left_wrapper.wrapped(db), right_wrapper.wrapped(db))
        })
    }

    fn disjointness_partials(
        &self,
        db: &'db dyn Db,
        left: Type<'db>,
        right: Type<'db>,
        left_partial: FunctoolsPartialInstance<'db>,
        right_partial: FunctoolsPartialInstance<'db>,
    ) -> ConstraintSet<'db, 'c> {
        self.with_recursion_guard(db, left, right, || {
            self.check_type_pair(
                db,
                left_partial.wrapped(db).inner(db),
                right_partial.wrapped(db).inner(db),
            )
        })
    }

    fn type_is_always_falsy(&self, db: &'db dyn Db, ty: Type<'db>) -> bool {
        ty.bool(db, self.env).is_always_false()
    }

    fn type_is_always_truthy(&self, db: &'db dyn Db, ty: Type<'db>) -> bool {
        ty.bool(db, self.env).is_always_true()
    }

    fn disjointness_protocols(
        &self,
        db: &'db dyn Db,
        left: Type<'db>,
        right: Type<'db>,
        left_proto: ProtocolInstanceType<'db>,
        right_proto: ProtocolInstanceType<'db>,
    ) -> ConstraintSet<'db, 'c> {
        self.with_recursion_guard(db, left, right, || {
            self.check_protocol_instance_pair(db, left_proto, right_proto)
        })
    }

    fn disjointness_protocol_special_form(
        &self,
        db: &'db dyn Db,
        left: Type<'db>,
        right: Type<'db>,
        protocol: ProtocolInstanceType<'db>,
        special_form: SpecialFormType,
    ) -> ConstraintSet<'db, 'c> {
        let env = self.env;
        self.with_recursion_guard(db, left, right, || {
            self.any_protocol_members_absent_or_disjoint(
                db,
                protocol,
                special_form.instance_fallback(db, env),
            )
        })
    }

    fn disjointness_protocol_known_instance(
        &self,
        db: &'db dyn Db,
        left: Type<'db>,
        right: Type<'db>,
        protocol: ProtocolInstanceType<'db>,
        known_instance: KnownInstanceType<'db>,
    ) -> ConstraintSet<'db, 'c> {
        let env = self.env;
        self.with_recursion_guard(db, left, right, || {
            self.any_protocol_members_absent_or_disjoint(
                db,
                protocol,
                known_instance.instance_fallback(db, env),
            )
        })
    }

    fn disjointness_protocol_members(
        &self,
        db: &'db dyn Db,
        left: Type<'db>,
        right: Type<'db>,
        protocol: ProtocolInstanceType<'db>,
        ty: Type<'db>,
    ) -> ConstraintSet<'db, 'c> {
        self.with_recursion_guard(db, left, right, || {
            self.any_protocol_members_absent_or_disjoint(db, protocol, ty)
        })
    }

    fn disjointness_protocol_nominal(
        &self,
        db: &'db dyn Db,
        left: Type<'db>,
        right: Type<'db>,
        protocol: ProtocolInstanceType<'db>,
        nominal: NominalInstanceType<'db>,
    ) -> ConstraintSet<'db, 'c> {
        self.with_recursion_guard(db, left, right, || {
            self.any_protocol_members_absent_or_disjoint(
                db,
                protocol,
                Type::NominalInstance(nominal),
            )
        })
    }

    fn disjointness_protocol_other(
        &self,
        db: &'db dyn Db,
        left: Type<'db>,
        right: Type<'db>,
        protocol: ProtocolInstanceType<'db>,
        other: Type<'db>,
    ) -> ConstraintSet<'db, 'c> {
        let env = self.env;
        self.with_recursion_guard(db, left, right, || {
            protocol
                .interface(db)
                .members(db)
                .when_any(db, self.constraints, |member| {
                    if let Some(context) = self.report_context() {
                        context.take();
                    }
                    let result = match other.member(db, env, member.name()).place {
                        Place::Defined(DefinedPlace {
                            ty: attribute_type, ..
                        }) => self.protocol_member_has_disjoint_type_from_ty(
                            db,
                            &member,
                            attribute_type,
                        ),
                        Place::Undefined => self.never(),
                    };
                    if let Some(context) = self.report_context()
                        && result.is_always_satisfied(db, env)
                    {
                        context.push(ErrorContext::ProtocolMemberIncompatible {
                            member_name: member.name().into(),
                        });
                    }
                    result
                })
        })
    }

    fn disjointness_alias_specializations(
        &self,
        db: &'db dyn Db,
        left_alias: GenericAlias<'db>,
        right_alias: GenericAlias<'db>,
    ) -> ConstraintSet<'db, 'c> {
        self.check_specialization_pair(
            db,
            left_alias.specialization(db),
            right_alias.specialization(db),
        )
    }

    fn disjointness_class_alias(
        &self,
        db: &'db dyn Db,
        class: ClassLiteral<'db>,
        alias_b: GenericAlias<'db>,
    ) -> ConstraintSet<'db, 'c> {
        class
            .default_specialization(db)
            .into_generic_alias()
            .when_none_or(db, self.constraints, |alias| {
                self.check_type_pair(db, Type::GenericAlias(alias_b), Type::GenericAlias(alias))
            })
    }

    fn disjointness_subclass_class(
        &self,
        db: &'db dyn Db,
        class_a: ClassType<'db>,
        class_b: ClassLiteral<'db>,
    ) -> ConstraintSet<'db, 'c> {
        let env = self.env;
        ConstraintSet::from_bool(
            self.constraints,
            !class_a.could_exist_in_mro_of_with_disjointness_checker(
                db,
                env,
                ClassType::NonGeneric(class_b),
                self,
            ),
        )
    }

    fn disjointness_subclass_alias(
        &self,
        db: &'db dyn Db,
        class_a: ClassType<'db>,
        alias_b: GenericAlias<'db>,
    ) -> ConstraintSet<'db, 'c> {
        let env = self.env;
        ConstraintSet::from_bool(
            self.constraints,
            !class_a.could_exist_in_mro_of_with_disjointness_checker(
                db,
                env,
                ClassType::Generic(alias_b),
                self,
            ),
        )
    }

    fn disjointness_subclass_other(
        &self,
        db: &'db dyn Db,
        subclass_of_ty: SubclassOfType<'db>,
        other: Type<'db>,
    ) -> ConstraintSet<'db, 'c> {
        let env = self.env;
        match subclass_of_ty.subclass_of() {
            SubclassOfInner::Dynamic(_) | SubclassOfInner::Protocol(_) => {
                self.check_type_pair(db, KnownClass::Type.to_instance(db, env), other)
            }
            SubclassOfInner::Class(_) => {
                self.check_type_pair(db, subclass_of_ty.to_metaclass_instance(db, env), other)
            }
            SubclassOfInner::TypeVar(_) => unreachable!(),
        }
    }

    fn disjointness_special_form_nominal(
        &self,
        db: &'db dyn Db,
        special_form: SpecialFormType,
        instance: NominalInstanceType<'db>,
    ) -> ConstraintSet<'db, 'c> {
        let env = self.env;
        ConstraintSet::from_bool(
            self.constraints,
            !special_form.is_instance_of(db, env, instance.class(db, env)),
        )
    }

    fn disjointness_known_instance_nominal(
        &self,
        db: &'db dyn Db,
        known_instance: KnownInstanceType<'db>,
        instance: NominalInstanceType<'db>,
    ) -> ConstraintSet<'db, 'c> {
        let env = self.env;
        ConstraintSet::from_bool(
            self.constraints,
            !known_instance.is_instance_of(db, env, instance.class(db, env)),
        )
    }

    fn disjointness_literal_nominal(
        &self,
        db: &'db dyn Db,
        literal: LiteralValueType<'db>,
        instance: NominalInstanceType<'db>,
    ) -> ConstraintSet<'db, 'c> {
        let env = self.env;
        let positive_relation_holds = match literal.kind() {
            LiteralValueTypeKind::Int(_) => {
                KnownClass::Int.when_subclass_of(db, env, instance.class(db, env), self.constraints)
            }
            LiteralValueTypeKind::Bool(_) => KnownClass::Bool.when_subclass_of(
                db,
                env,
                instance.class(db, env),
                self.constraints,
            ),
            LiteralValueTypeKind::LiteralString | LiteralValueTypeKind::String(_) => {
                KnownClass::Str.when_subclass_of(db, env, instance.class(db, env), self.constraints)
            }
            LiteralValueTypeKind::Bytes(_) => KnownClass::Bytes.when_subclass_of(
                db,
                env,
                instance.class(db, env),
                self.constraints,
            ),
            LiteralValueTypeKind::Enum(enum_literal) => self
                .as_relation_checker(TypeRelation::Subtyping)
                .check_type_pair(
                    db,
                    enum_literal.enum_class_instance(db, env),
                    Type::NominalInstance(instance),
                ),
        };
        positive_relation_holds.negate(db, self.constraints)
    }

    fn disjointness_bool_nominal(
        &self,
        db: &'db dyn Db,
        instance: NominalInstanceType<'db>,
    ) -> ConstraintSet<'db, 'c> {
        let env = self.env;
        KnownClass::Bool
            .when_subclass_of(db, env, instance.class(db, env), self.constraints)
            .negate(db, self.constraints)
    }

    fn disjointness_newtype_other(
        &self,
        db: &'db dyn Db,
        newtype: NewType<'db>,
        other: Type<'db>,
    ) -> ConstraintSet<'db, 'c> {
        self.check_type_pair(db, newtype.concrete_base_type(db), other)
    }

    fn disjointness_class_nominal(
        &self,
        db: &'db dyn Db,
        class: ClassLiteral<'db>,
        instance: NominalInstanceType<'db>,
    ) -> ConstraintSet<'db, 'c> {
        let env = self.env;
        class
            .metaclass_instance_type(db, env)
            .when_subtype_of(
                db,
                env,
                Type::NominalInstance(instance),
                self.constraints,
                self.inferable,
            )
            .negate(db, self.constraints)
    }

    fn disjointness_alias_nominal(
        &self,
        db: &'db dyn Db,
        alias: GenericAlias<'db>,
        instance: NominalInstanceType<'db>,
    ) -> ConstraintSet<'db, 'c> {
        let env = self.env;
        self.as_relation_checker(TypeRelation::Subtyping)
            .check_type_pair(
                db,
                ClassType::Generic(alias).metaclass_instance_type(db, env),
                Type::NominalInstance(instance),
            )
            .negate(db, self.constraints)
    }

    fn disjointness_function_nominal(
        &self,
        db: &'db dyn Db,
        function: FunctionType<'db>,
        instance: NominalInstanceType<'db>,
    ) -> ConstraintSet<'db, 'c> {
        let env = self.env;
        function
            .runtime_class(db)
            .when_subclass_of(db, env, instance.class(db, env), self.constraints)
            .negate(db, self.constraints)
    }

    fn disjointness_callable_other(
        &self,
        db: &'db dyn Db,
        class: KnownClass,
        other: Type<'db>,
    ) -> ConstraintSet<'db, 'c> {
        let env = self.env;
        self.check_type_pair(db, class.to_instance(db, env), other)
    }

    fn disjointness_bound_method_fallback(
        &self,
        db: &'db dyn Db,
        a: BoundMethodType<'db>,
        b: BoundMethodType<'db>,
    ) -> ConstraintSet<'db, 'c> {
        self.check_type_pair(db, a.func(db), b.func(db))
            .or(db, self.constraints, || {
                self.check_type_pair(db, a.self_instance(db), b.self_instance(db))
            })
    }

    fn disjointness_bound_method_functions(
        &self,
        db: &'db dyn Db,
        a: BoundMethodType<'db>,
        b: BoundMethodType<'db>,
        a_function: FunctionType<'db>,
        b_function: FunctionType<'db>,
    ) -> ConstraintSet<'db, 'c> {
        if a_function != b_function
            && a_function.has_known_decorator(db, FunctionDecorators::FINAL)
            && b_function.has_known_decorator(db, FunctionDecorators::FINAL)
        {
            // If *both* methods are `@final` (and they're not literally the same
            // definition), they must be disjoint.
            //
            // Note that we can't establish disjointness when only one side is `@final`,
            // because we have to worry about cases like this:
            //
            // ```
            // class A:
            //      def f(self): ...
            // class B:
            //      @final
            //      def f(self): ...
            // # Valid in this order, though `C(A, B)` would be invalid.
            // class C(B, A): ...
            // ```
            self.always()
        } else {
            // The names match, so `BoundMethod` disjointness depends on whether the bound
            // self types are disjoint. Note that this can produce confusing results in the
            // face of Liskov violations. For example:
            // ```
            // class A:
            //     def f(self) -> int: ...
            // class B:
            //     def f(self) -> str: ...
            // def _(x: Intersection[A, B]):
            //     x.f()
            // ```
            // `class C(A, B)` could inhabit that intersection, but `int` and `str` are
            // disjoint, so the type of `x.f()` there is going to be inferred as `Never`.
            // That's probably not correct in practice, but the right way to address it is
            // to emit a diagnostic on the definition of `C.f`.
            self.check_type_pair(db, a.self_instance(db), b.self_instance(db))
        }
    }

    fn disjointness_bound_method_other(
        &self,
        db: &'db dyn Db,
        other: Type<'db>,
    ) -> ConstraintSet<'db, 'c> {
        let env = self.env;
        self.check_type_pair(db, KnownClass::MethodType.to_instance(db, env), other)
    }

    fn disjointness_known_method_other(
        &self,
        db: &'db dyn Db,
        method: KnownBoundMethodType<'db>,
        other: Type<'db>,
    ) -> ConstraintSet<'db, 'c> {
        let env = self.env;
        self.check_type_pair(db, method.class().to_instance(db, env), other)
    }

    fn disjointness_descriptor_other(
        &self,
        db: &'db dyn Db,
        other: Type<'db>,
    ) -> ConstraintSet<'db, 'c> {
        let env = self.env;
        self.check_type_pair(
            db,
            KnownClass::WrapperDescriptorType.to_instance(db, env),
            other,
        )
    }

    fn disjointness_callable_final_nominal(
        &self,
        db: &'db dyn Db,
        nominal: NominalInstanceType<'db>,
    ) -> ConstraintSet<'db, 'c> {
        let env = self.env;
        Type::NominalInstance(nominal)
            .member_lookup_with_policy(
                db,
                env,
                "__call__",
                MemberLookupPolicy::NO_INSTANCE_FALLBACK,
            )
            .place
            .ignore_possibly_undefined()
            .when_none_or(db, self.constraints, |dunder_call| {
                self.as_relation_checker(TypeRelation::Assignability)
                    .check_type_pair(db, dunder_call, Type::Callable(CallableType::unknown(db)))
                    .negate(db, self.constraints)
            })
    }

    fn disjointness_module_nominal(
        &self,
        db: &'db dyn Db,
        instance: NominalInstanceType<'db>,
    ) -> ConstraintSet<'db, 'c> {
        let env = self.env;
        self.check_type_pair(
            db,
            Type::NominalInstance(instance),
            KnownClass::ModuleType.to_instance(db, env),
        )
    }

    fn disjointness_nominal_pair(
        &self,
        db: &'db dyn Db,
        left: Type<'db>,
        right: Type<'db>,
        left_i: NominalInstanceType<'db>,
        right_i: NominalInstanceType<'db>,
    ) -> ConstraintSet<'db, 'c> {
        self.with_recursion_guard(db, left, right, || {
            self.check_nominal_instance_pair(db, left_i, right_i)
        })
    }

    fn disjointness_property_other(
        &self,
        db: &'db dyn Db,
        property: PropertyInstanceType<'db>,
        other: Type<'db>,
    ) -> ConstraintSet<'db, 'c> {
        let env = self.env;
        self.check_type_pair(db, property.instance_fallback(db, env), other)
    }

    fn disjointness_slot_other(&self, db: &'db dyn Db, other: Type<'db>) -> ConstraintSet<'db, 'c> {
        let env = self.env;
        self.check_type_pair(
            db,
            KnownClass::MemberDescriptorType.to_instance(db, env),
            other,
        )
    }

    fn disjointness_bound_super_pair(
        &self,
        db: &'db dyn Db,
        left: BoundSuperType<'db>,
        right: BoundSuperType<'db>,
    ) -> ConstraintSet<'db, 'c> {
        self.as_equivalence_checker()
            .check_bound_super_pair(db, left, right)
            .negate(db, self.constraints)
    }

    fn disjointness_super_other(
        &self,
        db: &'db dyn Db,
        other: Type<'db>,
    ) -> ConstraintSet<'db, 'c> {
        let env = self.env;
        self.check_type_pair(db, KnownClass::Super.to_instance(db, env), other)
    }

    fn disjointness_typeddicts(
        &self,
        db: &'db dyn Db,
        left: Type<'db>,
        right: Type<'db>,
        left_td: TypedDictType<'db>,
        right_td: TypedDictType<'db>,
    ) -> ConstraintSet<'db, 'c> {
        self.with_recursion_guard(db, left, right, || {
            self.check_typeddict_pair(db, left_td, right_td)
        })
    }

    fn disjointness_typeddict_other(
        &self,
        db: &'db dyn Db,
        other: Type<'db>,
    ) -> ConstraintSet<'db, 'c> {
        let env = self.env;
        let dict_str_any = KnownClass::Dict.to_specialized_instance(
            db,
            env,
            &[KnownClass::Str.to_instance(db, env), Type::any()],
        );

        self.as_relation_checker(TypeRelation::Assignability)
            .check_type_pair(db, dict_str_any, other)
            .negate(db, self.constraints)
    }

    fn disjointness_left_recursive(
        &self,
        db: &'db dyn Db,
        left: Type<'db>,
        right: Type<'db>,
        left_recursive: RecursiveType<'db>,
    ) -> ConstraintSet<'db, 'c> {
        let env = self.env;
        left_recursive
            .unfold(db, env)
            .map(|left_unfolded| {
                self.with_recursion_guard(db, left, right, || {
                    self.check_type_pair(db, left_unfolded, right)
                })
            })
            .unwrap_or(self.never())
    }

    fn disjointness_right_recursive(
        &self,
        db: &'db dyn Db,
        left: Type<'db>,
        right: Type<'db>,
        right_recursive: RecursiveType<'db>,
    ) -> ConstraintSet<'db, 'c> {
        let env = self.env;
        right_recursive
            .unfold(db, env)
            .map(|right_unfolded| {
                self.with_recursion_guard(db, left, right, || {
                    self.check_type_pair(db, left, right_unfolded)
                })
            })
            .unwrap_or(self.never())
    }

    fn disjointness_transposed_typevar(
        &self,
        db: &'db dyn Db,
        subclass_of: SubclassOfType<'db>,
    ) -> Option<BoundTypeVarInstance<'db>> {
        let env = self.env;
        subclass_of
            .subclass_of()
            .with_transposed_type_var(db, env)
            .into_type_var()
    }

    fn disjointness_instance_approximation(
        &self,
        db: &'db dyn Db,
        other: Type<'db>,
    ) -> Option<Type<'db>> {
        let env = self.env;
        other.to_instance_approximation(db, env)
    }

    fn disjointness_typevar_is_inferable(
        &self,
        db: &'db dyn Db,
        left_tvar: BoundTypeVarInstance<'db>,
    ) -> bool {
        left_tvar.is_inferable(db, self.inferable)
    }

    fn disjointness_same_typevar(
        &self,
        db: &'db dyn Db,
        left_tvar: BoundTypeVarInstance<'db>,
        right_tvar: BoundTypeVarInstance<'db>,
    ) -> bool {
        left_tvar.is_same_typevar_as(db, right_tvar)
    }

    fn disjointness_negative_contains_typevar(
        &self,
        db: &'db dyn Db,
        intersection: IntersectionType<'db>,
        tvar: BoundTypeVarInstance<'db>,
    ) -> bool {
        intersection.negative(db).contains(&Type::TypeVar(tvar))
    }

    fn disjointness_same_wrapper_kind(
        &self,
        db: &'db dyn Db,
        left_wrapper: MethodWrapper<'db>,
        right_wrapper: MethodWrapper<'db>,
    ) -> bool {
        left_wrapper.kind(db) == right_wrapper.kind(db)
    }

    fn disjointness_nominal_is_final(
        &self,
        db: &'db dyn Db,
        nominal: NominalInstanceType<'db>,
    ) -> bool {
        let env = self.env;
        nominal.class(db, env).is_final(db)
    }

    fn disjointness_callable_runtime_class(
        &self,
        db: &'db dyn Db,
        callable: CallableType<'db>,
    ) -> Option<KnownClass> {
        callable.runtime_class(db)
    }

    fn disjointness_known_instance(&self, db: &'db dyn Db, other_class: KnownClass) -> Type<'db> {
        let env = self.env;
        other_class.to_instance(db, env)
    }

    fn disjointness_bound_function(
        &self,
        db: &'db dyn Db,
        a: BoundMethodType<'db>,
    ) -> Option<FunctionType<'db>> {
        a.function(db)
    }

    fn disjointness_function_names_differ(
        &self,
        db: &'db dyn Db,
        a_function: FunctionType<'db>,
        b_function: FunctionType<'db>,
    ) -> bool {
        a_function.name(db) != b_function.name(db)
    }

    fn disjointness_same_sentinel(
        &self,
        db: &'db dyn Db,
        left_sentinel: SentinelInstance<'db>,
        right_sentinel: SentinelInstance<'db>,
    ) -> bool {
        left_sentinel.is_same_sentinel(db, right_sentinel)
    }

    fn disjointness_enum_class(
        &self,
        db: &'db dyn Db,
        left: EnumLiteralType<'db>,
    ) -> EnumClassLiteral<'db> {
        left.enum_class_literal(db)
    }

    fn disjointness_enum_aliases_known(
        &self,
        db: &'db dyn Db,
        class: EnumClassLiteral<'db>,
    ) -> bool {
        class.aliases_are_known(db)
    }

    fn disjointness_literal_kinds_differ(
        &self,
        _db: &'db dyn Db,
        left: LiteralValueType<'db>,
        right: LiteralValueType<'db>,
    ) -> bool {
        left.kind() != right.kind()
    }

    fn disjointness_types_differ(
        &self,
        _db: &'db dyn Db,
        left: Type<'db>,
        right: Type<'db>,
    ) -> bool {
        left != right
    }

    fn disjointness_alias_origin(
        &self,
        db: &'db dyn Db,
        left_alias: GenericAlias<'db>,
    ) -> StaticClassLiteral<'db> {
        left_alias.origin(db)
    }

    fn disjointness_boolean(&self, _db: &'db dyn Db, value: bool) -> ConstraintSet<'db, 'c> {
        ConstraintSet::from_bool(self.constraints, value)
    }

    fn disjointness_clear_context(&self, _db: &'db dyn Db) {
        if let Some(context) = self.report_context() {
            context.take();
        }
    }

    fn disjointness_has_context(&self, _db: &'db dyn Db) -> bool {
        self.report_context().is_some()
    }

    fn check_property_instance_pair(
        &self,
        db: &'db dyn Db,
        left: PropertyInstanceType<'db>,
        right: PropertyInstanceType<'db>,
    ) -> ConstraintSet<'db, 'c> {
        let check_optional_methods = |left, right| match (left, right) {
            (None, None) => self.never(),
            (Some(left), Some(right)) => self.check_type_pair(db, left, right),
            (None | Some(_), None | Some(_)) => self.always(),
        };

        check_optional_methods(left.getter(db), right.getter(db)).or(db, self.constraints, || {
            check_optional_methods(left.setter(db), right.setter(db)).or(
                db,
                self.constraints,
                || check_optional_methods(left.deleter(db), right.deleter(db)),
            )
        })
    }
}
