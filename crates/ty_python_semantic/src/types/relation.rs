use crate::ProgramEnvironment;
use std::borrow::Cow;
use std::cell::{Cell, RefCell};
use std::rc::Rc;

use itertools::Itertools;
use rustc_hash::{FxHashMap, FxHashSet};

use crate::place::{DefinedPlace, Place};
use crate::types::callable::CallableTypeKind;
use crate::types::constraints::{
    ConstraintProvenance, ConstraintSetBuilder, IteratorConstraintsExtension,
    OptionConstraintsExtension, OwnedConstraintSet,
};
use crate::types::enums::is_single_member_enum;
use crate::types::function::FunctionDecorators;
use crate::types::projection::{
    CallableSelfBinding, ObservationEdge, ObservedType, ObservedTypeOrigin as RelationTypeOrigin,
    ObservedTypePair,
};
use crate::types::relation_error::ErrorRelation;
use crate::types::set_theoretic::{RecursivelyDefined, UnionBuilder};
use crate::types::signatures::{ParametersKind, SignatureRelationKey};
use crate::types::tuple::TupleType;
use crate::types::typevar::TypeVarDomain;
use crate::types::{
    ApplyTypeMappingVisitor, CallableType, ClassBase, ClassLiteral, ClassType, IntersectionType,
    KnownBoundMethodType, KnownClass, KnownInstanceType, LiteralValueTypeKind, MemberLookupPolicy,
    PropertyInstanceType, ProtocolInstanceType, SubclassOfInner, SubclassOfType,
    TypeVarBoundOrConstraints, UnionType, UpcastPolicy,
};
use crate::{
    Db,
    types::{
        ErrorContext, ErrorContextTree, Type, TypePair, constraints::ConstraintSet,
        typevar::TypeVarSet,
    },
};

mod frame;
mod schema;

pub(super) use frame::RelationContext;

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
            TypeRelation::Subtyping => ty.subtyping_is_always_reflexive(),
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
            Type::Deferred(_) => false,
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
                | KnownBoundMethodType::ConstraintSetIsAlwaysSatisfied(_)
                | KnownBoundMethodType::ConstraintSetIsNeverSatisfied(_)
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
            .is_always_satisfied(db, env, TypeVarSet::None)
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

    pub(super) fn when_constraint_set_subtype_of<'c>(
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
            TypeRelation::Subtyping,
            TypeVarEvaluation::Lazy,
            ConstraintProvenance::Evidence,
        )
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
        self.when_assignable_to_owned(db, env, target, TypeVarSet::None)
            .query(|_constraints, when| when.is_always_satisfied(db, env, TypeVarSet::None))
    }

    /// Whether an attribute accepts every value of `value_ty` through ordinary assignment.
    pub(super) fn is_attribute_writable_with(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        name: &str,
        value_ty: Type<'db>,
    ) -> bool {
        let constraints = ConstraintSetBuilder::new();
        TypeRelationChecker::new(
            env,
            TypeRelation::Assignability,
            &constraints,
            TypeVarSet::None,
            &ApplyTypeMappingVisitor::new(env),
            ObservedTypePair::roots(self, value_ty),
        )
        .check_attribute_write(db, self, name, value_ty)
        .is_always_satisfied(db, env, TypeVarSet::None)
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
            provenance: ConstraintProvenance::Evidence,
            perform_expensive_checks: true,
            materialization_visitor: &ApplyTypeMappingVisitor::new(env),
            observations: ObservedTypePair::roots(self, target),
        };
        checker.check_type_pair(db, self, target);
        checker.into_error_context()
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

    pub(super) fn when_assignable_to_owned(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        target: Type<'db>,
        inferable: TypeVarSet<'db>,
    ) -> Cow<'db, OwnedConstraintSet<'db>> {
        #[salsa::tracked(
            returns(ref),
            cycle_initial=|_, _, _, _| OwnedConstraintSet::always(),
            heap_size=ruff_memory_usage::heap_size,
        )]
        fn when_assignable_to_owned_impl<'db>(
            db: &'db dyn Db,
            types: TypePair<'db>,
            inferable: TypeVarSet<'db>,
        ) -> OwnedConstraintSet<'db> {
            let program = types.program(db);
            let env = ProgramEnvironment::from_program(program);
            let constraints = ConstraintSetBuilder::new();
            constraints.into_owned(|constraints| {
                let source = types.first(db);
                let target = types.second(db);

                source.has_relation_to(
                    db,
                    &env,
                    target,
                    constraints,
                    inferable,
                    TypeRelation::Assignability,
                )
            })
        }

        self.assert_not_recursive_var();
        target.assert_not_recursive_var();
        if self.is_trivially_constraint_set_assignable_to(db, target) {
            return Cow::Owned(OwnedConstraintSet::always());
        }

        let program = env.program(db);
        Cow::Borrowed(when_assignable_to_owned_impl(
            db,
            TypePair::new(db, program, self, target),
            inferable,
        ))
    }

    /// Returns whether constraint-set assignability is known to be unconditionally satisfied
    /// before constructing the relation checker.
    fn is_trivially_constraint_set_assignable_to(self, db: &'db dyn Db, target: Type<'db>) -> bool {
        if self.materialized_divergent_fallback().is_none() && self == target {
            return true;
        }

        // Type variables must be converted into constraints before applying the remaining
        // relation shortcuts.
        if self.is_type_var() || target.is_type_var() {
            return false;
        }

        match (self, target) {
            (Type::Never | Type::Dynamic(_), _) | (_, Type::Dynamic(_)) => true,
            (_, Type::NominalInstance(target)) if target.is_object() => true,
            (_, Type::Union(union)) => {
                self.materialized_divergent_fallback().is_none()
                    && union.elements(db).contains(&self)
            }
            (Type::Intersection(intersection), _) => {
                target.materialized_divergent_fallback().is_none()
                    && intersection.positive(db).contains(&target)
            }
            _ => false,
        }
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
        #[salsa::tracked(
            returns(ref),
            cycle_initial=|_, _, _| OwnedConstraintSet::always(),
            heap_size=ruff_memory_usage::heap_size,
        )]
        fn when_constraint_set_assignable_to_owned_impl<'db>(
            db: &'db dyn Db,
            types: TypePair<'db>,
        ) -> OwnedConstraintSet<'db> {
            let program = types.program(db);
            let env = ProgramEnvironment::from_program(program);
            let constraints = ConstraintSetBuilder::new();
            constraints.into_owned(|constraints| {
                let source = types.first(db);
                let target = types.second(db);

                source.has_relation_to_with_typevar_evaluation(
                    db,
                    &env,
                    target,
                    constraints,
                    TypeVarSet::None,
                    TypeRelation::Assignability,
                    TypeVarEvaluation::Lazy,
                    ConstraintProvenance::Evidence,
                )
            })
        }

        if self.is_trivially_constraint_set_assignable_to(db, target) {
            return Cow::Owned(OwnedConstraintSet::always());
        }

        let program = env.program(db);
        Cow::Borrowed(when_constraint_set_assignable_to_owned_impl(
            db,
            TypePair::new(db, program, self, target),
        ))
    }

    pub(super) fn when_constraint_set_assignable_to<'c>(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        target: Type<'db>,
        constraints: &'c ConstraintSetBuilder<'db>,
    ) -> ConstraintSet<'db, 'c> {
        self.when_constraint_set_assignable_to_with_provenance(
            db,
            env,
            target,
            constraints,
            ConstraintProvenance::Evidence,
        )
    }

    pub(super) fn when_constraint_set_assignable_to_with_provenance<'c>(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        target: Type<'db>,
        constraints: &'c ConstraintSetBuilder<'db>,
        provenance: ConstraintProvenance,
    ) -> ConstraintSet<'db, 'c> {
        self.has_relation_to_with_typevar_evaluation(
            db,
            env,
            target,
            constraints,
            TypeVarSet::None,
            TypeRelation::Assignability,
            TypeVarEvaluation::Lazy,
            provenance,
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
        #[salsa::tracked(returns(copy), cycle_initial=|_, _, _| true, heap_size=ruff_memory_usage::heap_size)]
        fn is_redundant_with_impl<'db>(db: &'db dyn Db, types: TypePair<'db>) -> bool {
            let program = types.program(db);
            let env = ProgramEnvironment::from_program(program);
            types
                .first(db)
                .has_relation_to(
                    db,
                    &env,
                    types.second(db),
                    &ConstraintSetBuilder::new(),
                    TypeVarSet::None,
                    TypeRelation::Redundancy { pure: false },
                )
                .is_always_satisfied(db, &env, TypeVarSet::None)
        }

        if self == other {
            return true;
        }

        let program = env.program(db);
        is_redundant_with_impl(db, TypePair::new(db, program, self, other))
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
        .is_always_satisfied(db, &env, TypeVarSet::None)
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
            ConstraintProvenance::Evidence,
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
        provenance: ConstraintProvenance,
    ) -> ConstraintSet<'db, 'c> {
        let materialization_visitor = ApplyTypeMappingVisitor::new(env);
        let checker = TypeRelationChecker {
            env,
            constraints,
            inferable,
            relation,
            typevar_evaluation,
            context_tree: None,
            provenance,
            perform_expensive_checks: true,
            materialization_visitor: &materialization_visitor,
            observations: ObservedTypePair::roots(self, target),
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
            .is_always_satisfied(db, env, TypeVarSet::None)
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

    fn when_equivalent_to_with_materialization_visitor<'c>(
        self,
        db: &'db dyn Db,
        other: Type<'db>,
        constraints: &'c ConstraintSetBuilder<'db>,
        materialization_visitor: &ApplyTypeMappingVisitor<'_, 'db>,
        typevar_evaluation: TypeVarEvaluation,
    ) -> ConstraintSet<'db, 'c> {
        let checker = EquivalenceChecker {
            env: materialization_visitor.env,
            constraints,
            provenance: ConstraintProvenance::Evidence,
            perform_expensive_checks: true,
            typevar_evaluation,
            materialization_visitor,
            observations: ObservedTypePair::roots(self, other),
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
            .is_always_satisfied(db, env, TypeVarSet::None)
    }

    pub(crate) fn when_disjoint_from<'c>(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        other: Type<'db>,
        constraints: &'c ConstraintSetBuilder<'db>,
        inferable: TypeVarSet<'db>,
    ) -> ConstraintSet<'db, 'c> {
        let materialization_visitor = ApplyTypeMappingVisitor::new(env);
        let checker = DisjointnessChecker {
            env,
            constraints,
            inferable,
            context_tree: None,
            provenance: ConstraintProvenance::Evidence,
            perform_expensive_checks: true,
            materialization_visitor: &materialization_visitor,
            observations: ObservedTypePair::roots(self, other),
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
            provenance: ConstraintProvenance::Evidence,
            perform_expensive_checks: true,
            materialization_visitor: &ApplyTypeMappingVisitor::new(env),
            observations: ObservedTypePair::roots(self, other),
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
        let materialization_visitor = ApplyTypeMappingVisitor::new(env);
        let checker = DisjointnessChecker {
            env,
            constraints,
            inferable,
            context_tree: None,
            provenance: ConstraintProvenance::Evidence,
            perform_expensive_checks: false,
            materialization_visitor: &materialization_visitor,
            observations: ObservedTypePair::roots(self, other),
        };
        checker.check_type_pair(db, self, other)
    }
}

/// The active obligations of one type-relation operation, independent of the arenas used to
/// store its constraints. Checking inferred bounds can require a new arena while the caller's
/// arena is borrowed by the solver; it still belongs to the same proof.
///
/// Completed obligations are reusable only when their evaluation used no coinductive assumption
/// and encountered no unresolved proof. Owned constraints can then be imported into another arena
/// without retaining a dependency on an active ancestor.
#[derive(Debug, Default)]
pub(super) struct RelationSession<'db> {
    active: RefCell<Vec<ProofObligation<'db>>>,
    completed: RefCell<FxHashMap<ObservedRelationObligation<'db>, OwnedConstraintSet<'db>>>,
    negative: Cell<bool>,
    assumption_epoch: Cell<usize>,
    incomplete_epoch: Cell<usize>,
    parametric_schemas: RefCell<Vec<schema::ParametricSchema<'db>>>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum ProofObligation<'db> {
    Types(ObservedRelationObligation<'db>),
    Signature {
        key: SignatureRelationKey<'db>,
        negative: bool,
    },
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
enum RelationGoal {
    Relation(TypeRelation),
    Disjointness,
    Observation,
    CallBinding,
    CallableUpcast,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
struct RelationObligation<'db> {
    source: Type<'db>,
    target: Type<'db>,
    relation: RelationGoal,
    evaluation: TypeVarEvaluation,
    inferable: TypeVarSet<'db>,
    provenance: ConstraintProvenance,
    perform_expensive_checks: bool,
    negative: bool,
}

/// Exact proofs use the complete closed operand types. Origins identify observed expressions;
/// dependencies retain the known parents of unresolved observations. Both can expose a growing
/// recursive dependency, but neither can establish compatibility between distinct closed types.
#[derive(Debug, Clone, Eq, Hash, PartialEq)]
struct ObservedRelationObligation<'db> {
    obligation: RelationObligation<'db>,
    source_origin: Option<Rc<RelationTypeOrigin<'db>>>,
    target_origin: Option<Rc<RelationTypeOrigin<'db>>>,
    source_dependency: Vec<Rc<RelationTypeOrigin<'db>>>,
    target_dependency: Vec<Rc<RelationTypeOrigin<'db>>>,
}

impl<'db> From<RelationObligation<'db>> for ObservedRelationObligation<'db> {
    fn from(obligation: RelationObligation<'db>) -> Self {
        Self {
            obligation,
            source_origin: None,
            target_origin: None,
            source_dependency: Vec::new(),
            target_dependency: Vec::new(),
        }
    }
}

fn same_expression<'db>(
    db: &'db dyn Db,
    left: Type<'db>,
    left_origin: Option<Rc<RelationTypeOrigin<'db>>>,
    right: Type<'db>,
    right_origin: Option<Rc<RelationTypeOrigin<'db>>>,
) -> bool {
    if left == right {
        return true;
    }
    // A substituted parameter can expose a smaller application of the same constructor.
    // Its expression node distinguishes that finite descent from a recursive reference;
    // declaration identity must not override the recorded observation.
    match (left_origin, right_origin) {
        (Some(left), Some(right)) => {
            return !left.node.template.is_type_var()
                && !right.node.template.is_type_var()
                && left.constructor == right.constructor
                && left.node == right.node
                && (left.application != right.application || left.operations != right.operations);
        }
        (Some(_), None) | (None, Some(_)) => return false,
        (None, None) => {}
    }
    left.may_share_type_identity(db, right)
        && left.to_type_identity(db) == right.to_type_identity(db)
}

pub(super) enum RelationReentry {
    Exact,
    Negative,
    Expanding,
}

fn same_observed_expression<'db>(
    db: &'db dyn Db,
    left: Type<'db>,
    left_origin: Option<Rc<RelationTypeOrigin<'db>>>,
    right: Type<'db>,
    right_origin: Option<Rc<RelationTypeOrigin<'db>>>,
    left_dependencies: &[Rc<RelationTypeOrigin<'db>>],
    right_dependencies: &[Rc<RelationTypeOrigin<'db>>],
) -> bool {
    if left_dependencies.is_empty() && right_dependencies.is_empty() {
        return same_expression(db, left, left_origin, right, right_origin);
    }
    left_origin
        .iter()
        .chain(left_dependencies)
        .any(|left_origin| {
            right_origin
                .iter()
                .chain(right_dependencies)
                .any(|right_origin| {
                    same_expression(
                        db,
                        left,
                        Some(Rc::clone(left_origin)),
                        right,
                        Some(Rc::clone(right_origin)),
                    )
                })
        })
}

impl<'db> RelationSession<'db> {
    pub(super) fn is_active(&self) -> bool {
        !self.active.borrow().is_empty()
    }

    pub(super) fn is_negative(&self) -> bool {
        self.negative.get()
    }

    pub(super) fn incomplete_epoch(&self) -> usize {
        self.incomplete_epoch.get()
    }

    pub(super) fn mark_incomplete(&self) {
        self.incomplete_epoch
            .set(self.incomplete_epoch.get().wrapping_add(1));
    }

    /// Evaluate an obligation whose result will be negated by its caller. A recursive dependency
    /// through an odd number of negations cannot use the positive coinductive assumption.
    fn with_negation<R>(&self, work: impl FnOnce() -> R) -> R {
        let previous = self.negative.replace(!self.negative.get());
        let _scope = RelationPolarityScope {
            negative: &self.negative,
            previous,
        };
        work()
    }

    fn with_polarity<R>(&self, negative: bool, work: impl FnOnce() -> R) -> R {
        let previous = self.negative.replace(negative);
        let _scope = RelationPolarityScope {
            negative: &self.negative,
            previous,
        };
        work()
    }

    fn visit<R>(
        &self,
        db: &'db dyn Db,
        observed: impl Into<ObservedRelationObligation<'db>>,
        work: impl FnOnce() -> R,
    ) -> Result<R, RelationReentry> {
        let observed = observed.into();
        let obligation = observed.obligation;
        // Computing a recursive constructor's identity can itself require type queries.
        let active: Vec<_> = self
            .active
            .borrow()
            .iter()
            .filter_map(|obligation| match obligation {
                ProofObligation::Types(obligation) => Some(obligation.clone()),
                ProofObligation::Signature { .. } => None,
            })
            .collect();
        if active
            .iter()
            .any(|previous| previous.obligation == obligation)
        {
            self.assumption_epoch
                .set(self.assumption_epoch.get().wrapping_add(1));
            return Err(RelationReentry::Exact);
        }
        if active.iter().any(|previous| {
            let previous = previous.obligation;
            previous.source == obligation.source
                && previous.target == obligation.target
                && previous.relation == obligation.relation
                && previous.evaluation == obligation.evaluation
                && previous.inferable == obligation.inferable
                && previous.provenance == obligation.provenance
                && previous.perform_expensive_checks == obligation.perform_expensive_checks
                && previous.negative != obligation.negative
        }) {
            return Err(RelationReentry::Negative);
        }
        if active.iter().any(|previous_observed| {
            let previous = previous_observed.obligation;
            (previous.source != obligation.source || previous.target != obligation.target)
                && previous.relation == obligation.relation
                && previous.evaluation == obligation.evaluation
                && previous.provenance == obligation.provenance
                && previous.perform_expensive_checks == obligation.perform_expensive_checks
                && same_observed_expression(
                    db,
                    obligation.source,
                    observed.source_origin.clone(),
                    previous.source,
                    previous_observed.source_origin.clone(),
                    &observed.source_dependency,
                    &previous_observed.source_dependency,
                )
                && same_observed_expression(
                    db,
                    obligation.target,
                    observed.target_origin.clone(),
                    previous.target,
                    previous_observed.target_origin.clone(),
                    &observed.target_dependency,
                    &previous_observed.target_dependency,
                )
        }) {
            return Err(RelationReentry::Expanding);
        }
        Ok(self.enter(ProofObligation::Types(observed), work))
    }

    /// Share completed finite subproofs without sharing an ancestor's recursive assumptions.
    /// Failed subtype comparisons are reevaluated when collecting their diagnostic context.
    /// Disjointness diagnostics instead explain a successful proof, so they always reevaluate.
    fn visit_type_pair<'c>(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        builder: &'c ConstraintSetBuilder<'db>,
        observed: impl Into<ObservedRelationObligation<'db>>,
        collect_context: bool,
        work: impl FnOnce() -> ConstraintSet<'db, 'c>,
    ) -> Result<ConstraintSet<'db, 'c>, RelationReentry> {
        let observed = observed.into();
        let obligation = observed.obligation;
        let cached = self.completed.borrow().get(&observed).cloned();
        if let Some(cached) = cached {
            let result = builder.load(db, env, &cached);
            if !collect_context
                || (matches!(obligation.relation, RelationGoal::Relation(_))
                    && result.has_satisfying_specialization(db, env, obligation.inferable))
            {
                return Ok(result);
            }
        }
        let assumptions = self.assumption_epoch.get();
        let incomplete = self.incomplete_epoch.get();
        let result = self.visit(db, observed.clone(), work)?;
        if result.is_complete()
            && self.assumption_epoch.get() == assumptions
            && self.incomplete_epoch.get() == incomplete
        {
            self.completed
                .borrow_mut()
                .insert(observed, result.to_owned());
        }
        Ok(result)
    }

    /// A named overload is a signature constructor, and its captures form its application.
    /// Revisiting an exact application can close a positive recursive proof. Returning to the
    /// constructor with fresh local variables or different captures instead leaves an unresolved
    /// obligation; declaration identity alone cannot establish compatibility.
    pub(super) fn visit_signature<R>(
        &self,
        db: &'db dyn Db,
        key: SignatureRelationKey<'db>,
        work: impl FnOnce() -> R,
    ) -> Result<R, RelationReentry> {
        let negative = self.is_negative();
        {
            let active = self.active.borrow();
            let signatures = || {
                active.iter().filter_map(|previous| match previous {
                    ProofObligation::Signature { key, negative } => Some((key, *negative)),
                    ProofObligation::Types(_) => None,
                })
            };
            if signatures().any(|(previous, previous_negative)| {
                *previous == key && previous_negative == negative
            }) {
                self.assumption_epoch
                    .set(self.assumption_epoch.get().wrapping_add(1));
                return Err(RelationReentry::Exact);
            }
            if signatures().any(|(previous, _)| *previous == key) {
                return Err(RelationReentry::Negative);
            }
            if signatures().any(|(previous, _)| previous.has_same_constructor(db, &key)) {
                return Err(RelationReentry::Expanding);
            }
        }
        Ok(self.enter(ProofObligation::Signature { key, negative }, work))
    }

    fn enter<R>(&self, obligation: ProofObligation<'db>, work: impl FnOnce() -> R) -> R {
        let depth = self.active.borrow().len();
        self.active.borrow_mut().push(obligation);
        let _visit = ActiveRelation {
            session: self,
            depth,
        };
        work()
    }
}

struct RelationPolarityScope<'a> {
    negative: &'a Cell<bool>,
    previous: bool,
}

impl Drop for RelationPolarityScope<'_> {
    fn drop(&mut self) {
        self.negative.set(self.previous);
    }
}

struct ActiveRelation<'a, 'db> {
    session: &'a RelationSession<'db>,
    depth: usize,
}

impl Drop for ActiveRelation<'_, '_> {
    fn drop(&mut self) {
        let mut active = self.session.active.borrow_mut();
        debug_assert_eq!(active.len(), self.depth + 1);
        active.pop();
    }
}

#[derive(Clone)]
pub(super) struct TypeRelationChecker<'a, 'c, 'db> {
    pub(super) env: &'a ProgramEnvironment<'db>,
    pub(super) constraints: &'c ConstraintSetBuilder<'db>,
    pub(super) inferable: TypeVarSet<'db>,
    pub(super) relation: TypeRelation,
    pub(super) typevar_evaluation: TypeVarEvaluation,
    pub(super) provenance: ConstraintProvenance,
    context_tree: Option<ErrorContextTree<'db>>,
    pub(super) perform_expensive_checks: bool,

    pub(super) materialization_visitor: &'a ApplyTypeMappingVisitor<'a, 'db>,
    observations: ObservedTypePair<'db>,
}

impl<'a, 'c, 'db> TypeRelationChecker<'a, 'c, 'db> {
    /// Create a relation checker that eagerly evaluates type variables.
    pub(super) fn new(
        env: &'a ProgramEnvironment<'db>,
        relation: TypeRelation,
        constraints: &'c ConstraintSetBuilder<'db>,
        inferable: TypeVarSet<'db>,
        materialization_visitor: &'a ApplyTypeMappingVisitor<'a, 'db>,
        observations: ObservedTypePair<'db>,
    ) -> Self {
        Self {
            env,
            constraints,
            inferable,
            relation,
            typevar_evaluation: TypeVarEvaluation::Eager,
            context_tree: None,
            provenance: constraints.relation_context().provenance(),
            perform_expensive_checks: constraints.relation_context().perform_expensive_checks(),
            materialization_visitor,
            observations,
        }
    }

    pub(super) fn subtyping(
        env: &'a ProgramEnvironment<'db>,
        constraints: &'c ConstraintSetBuilder<'db>,
        inferable: TypeVarSet<'db>,
        materialization_visitor: &'a ApplyTypeMappingVisitor<'a, 'db>,
        observations: ObservedTypePair<'db>,
    ) -> Self {
        Self::new(
            env,
            TypeRelation::Subtyping,
            constraints,
            inferable,
            materialization_visitor,
            observations,
        )
    }

    pub(super) fn constraint_set_assignability(
        env: &'a ProgramEnvironment<'db>,
        constraints: &'c ConstraintSetBuilder<'db>,
        materialization_visitor: &'a ApplyTypeMappingVisitor<'a, 'db>,
        observations: ObservedTypePair<'db>,
    ) -> Self {
        Self {
            typevar_evaluation: TypeVarEvaluation::Lazy,
            ..Self::new(
                env,
                TypeRelation::Assignability,
                constraints,
                TypeVarSet::None,
                materialization_visitor,
                observations,
            )
        }
    }

    pub(super) fn assignability(
        env: &'a ProgramEnvironment<'db>,
        constraints: &'c ConstraintSetBuilder<'db>,
        materialization_visitor: &'a ApplyTypeMappingVisitor<'a, 'db>,
        observations: ObservedTypePair<'db>,
    ) -> Self {
        Self::new(
            env,
            TypeRelation::Assignability,
            constraints,
            TypeVarSet::None,
            materialization_visitor,
            observations,
        )
    }

    pub(super) fn constraint_set_assignability_with_context(
        env: &'a ProgramEnvironment<'db>,
        constraints: &'c ConstraintSetBuilder<'db>,
        materialization_visitor: &'a ApplyTypeMappingVisitor<'a, 'db>,
        observations: ObservedTypePair<'db>,
    ) -> Self {
        Self {
            env,
            constraints,
            inferable: TypeVarSet::None,
            relation: TypeRelation::Assignability,
            typevar_evaluation: TypeVarEvaluation::Lazy,
            context_tree: Some(ErrorContextTree::new(TypeRelation::Assignability)),
            provenance: constraints.relation_context().provenance(),
            perform_expensive_checks: constraints.relation_context().perform_expensive_checks(),
            materialization_visitor,
            observations,
        }
    }

    pub(super) fn assignability_with_context(
        env: &'a ProgramEnvironment<'db>,
        constraints: &'c ConstraintSetBuilder<'db>,
        materialization_visitor: &'a ApplyTypeMappingVisitor<'a, 'db>,
        observations: ObservedTypePair<'db>,
    ) -> Self {
        Self {
            env,
            constraints,
            inferable: TypeVarSet::None,
            relation: TypeRelation::Assignability,
            typevar_evaluation: TypeVarEvaluation::Eager,
            context_tree: Some(ErrorContextTree::new(TypeRelation::Assignability)),
            provenance: constraints.relation_context().provenance(),
            perform_expensive_checks: constraints.relation_context().perform_expensive_checks(),
            materialization_visitor,
            observations,
        }
    }

    pub(super) fn with_inferable_typevars(&self, inferable: TypeVarSet<'db>) -> Self {
        Self {
            inferable,
            ..self.clone()
        }
    }

    pub(super) fn operands(&self) -> &ObservedTypePair<'db> {
        &self.observations
    }

    pub(super) fn with_operands(&self, observations: ObservedTypePair<'db>) -> Self {
        Self {
            observations,
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
        let Some(source_observed) =
            self.observations
                .source
                .project(db, env, ObservationEdge::ClassView)
        else {
            return false;
        };
        let Some(target_observed) =
            self.observations
                .target
                .project(db, env, ObservationEdge::ClassView)
        else {
            return false;
        };
        let checker = Self {
            provenance: self.provenance,
            ..Self::subtyping(
                env,
                self.constraints,
                TypeVarSet::None,
                self.materialization_visitor,
                ObservedTypePair::new(source_observed, target_observed),
            )
        };
        checker
            .check_class_pair(db, source, target)
            .is_always_satisfied(db, env, TypeVarSet::None)
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
        intersection
            .positive(db)
            .iter()
            .any(|element| match element {
                Type::TypeVar(tvar) => !tvar.is_inferable(db, self.inferable),
                Type::NewTypeInstance(newtype) => newtype.concrete_base_type(db).is_union(),
                _ => false,
            })
    }

    fn check_source_typevar_bounds(
        &self,
        db: &'db dyn Db,
        bound_or_constraints: TypeVarBoundOrConstraints<'db>,
        target: Type<'db>,
    ) -> ConstraintSet<'db, 'c> {
        match bound_or_constraints {
            TypeVarBoundOrConstraints::UpperBound(bound) => {
                self.check_child_pair(db, bound, target)
            }
            TypeVarBoundOrConstraints::Constraints(constraints) => constraints
                .elements(db)
                .iter()
                .when_all(db, self.constraints, |&constraint| {
                    self.check_child_pair(db, constraint, target)
                }),
        }
    }

    /// Expose union alternatives as observed expressions. Normalization is part of this
    /// proof, so it preserves both the operand graph and the current recursion session.
    fn observe_union(&self, db: &'db dyn Db, operand: &ObservedType<'db>) -> ObservedType<'db> {
        let mut pending = vec![operand.clone()];
        let mut leaves = Vec::new();
        let mut applications: Vec<ObservedType<'db>> = Vec::new();
        while let Some(current) = pending.pop() {
            match current.ty {
                Type::Union(_) => {
                    pending.extend(current.union_children(db, self.env).into_iter().rev());
                }
                Type::TypeAlias(_) | Type::Recursive(_) => {
                    // Union flattening follows constructor edges just like a relation does.
                    // A backedge is retained as unresolved, leaving finite alternatives available.
                    if applications.iter().any(|previous| {
                        same_expression(
                            db,
                            current.ty,
                            current.origin(),
                            previous.ty,
                            previous.origin(),
                        )
                    }) {
                        leaves.push(current.unresolved());
                    } else if let Some(body) = current.unfold(db, self.env) {
                        applications.push(current);
                        pending.push(body);
                    } else {
                        leaves.push(current.unresolved());
                    }
                }
                _ => leaves.push(current),
            }
        }
        let mut builder = UnionBuilder::new(db, self.env)
            .with_observed_context(self.context())
            .unpack_aliases(false);
        for leaf in leaves {
            builder.add_observed_in_place(leaf);
        }
        builder.build_observed()
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
                self.without_context_collection(|| self.check_child_pair(db, supertype, target));
            if supertype_result.is_trivially_always_satisfied() {
                return supertype_result;
            }
        }

        union
            .elements(db)
            .iter()
            .enumerate()
            .when_all(db, self.constraints, |(index, &element)| {
                let constraint_set = self.check_child_pair_at(
                    db,
                    element,
                    target,
                    ObservationEdge::UnionElement(index),
                    ObservationEdge::Identity,
                );
                if let Some(context) = self.report_context()
                    && constraint_set.is_never_satisfied(db, self.env, self.inferable)
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
        let target = Type::Union(union);
        if let Type::Intersection(intersection) = source
            && let Some(alternatives) = intersection.finite_alternative_union(db, self.env)
        {
            return self.check_child_pair(db, alternatives, target);
        }

        let check_expanded_source = || {
            // Normally non-unions cannot directly contain unions in our model due to the fact that
            // we enforce a DNF structure on our set-theoretic types. However, it *is* possible for
            // there to be a newtype of a union, for an intersection to contain a newtype of a
            // union, or for a non-inferable typevar (possibly inside an intersection) to widen to a
            // bound or set of constraints that exposes a union; this requires special handling.
            match source {
                Type::TypeVar(typevar)
                    if !typevar.is_inferable(db, self.inferable)
                        && let Some(bound_or_constraints) =
                            typevar.typevar(db).bound_or_constraints(db, self.env) =>
                {
                    self.check_source_typevar_bounds(db, bound_or_constraints, target)
                }
                Type::Intersection(intersection)
                    if self.should_expand_intersection(db, intersection) =>
                {
                    self.check_child_pair(
                        db,
                        intersection.with_expanded_typevars_and_newtypes(db, self.env),
                        target,
                    )
                }
                Type::NewTypeInstance(newtype) => {
                    let concrete_base = newtype.concrete_base_type(db);
                    if concrete_base.is_union() {
                        self.check_child_pair(db, concrete_base, target)
                    } else {
                        self.never()
                    }
                }
                _ => self.never(),
            }
        };

        let mut elements_context = vec![];
        let context_tree = self.context_tree.as_ref().filter(|tree| tree.is_enabled());

        let elements = union.elements(db);
        let result = elements
            .iter()
            .enumerate()
            .when_any(db, self.constraints, |(index, &element)| {
                let result = self.check_child_pair_at(
                    db,
                    source,
                    element,
                    ObservationEdge::Identity,
                    ObservationEdge::UnionElement(index),
                );
                if let Some(context_tree) = context_tree {
                    let context = context_tree.take();
                    if !context.is_empty() {
                        elements_context.push(context);
                    }
                }
                result
            })
            .or(db, self.constraints, check_expanded_source);

        if context_tree.is_some()
            && !elements_context.is_empty()
            && result.is_never_satisfied(db, self.env, self.inferable)
        {
            let elements_without_context = elements.len() - elements_context.len();
            if elements_without_context > 0 && elements_without_context < elements.len() {
                elements_context.push(ErrorContextTree::from_context(
                    ErrorContext::NotAssignableToNOtherUnionElements {
                        n: elements_without_context,
                    },
                    self.relation,
                ));
            }
            self.set_context(
                ErrorContext::NotAssignableToAnyUnionElement {
                    source,
                    union: target,
                },
                elements_context,
            );
        }

        result
    }

    fn check_target_intersection(
        &self,
        db: &'db dyn Db,
        source: Type<'db>,
        intersection: IntersectionType<'db>,
    ) -> ConstraintSet<'db, 'c> {
        intersection
            .positive(db)
            .iter()
            .enumerate()
            .when_all(db, self.constraints, |(index, &positive)| {
                let constraint_set = self.check_child_pair_at(
                    db,
                    source,
                    positive,
                    ObservationEdge::Identity,
                    ObservationEdge::IntersectionPositive(index),
                );
                if let Some(context) = self.report_context()
                    && constraint_set.is_never_satisfied(db, self.env, self.inferable)
                {
                    context.push(ErrorContext::NotAssignableToIntersectionElement {
                        source,
                        element: positive,
                        intersection: Type::Intersection(intersection),
                    });
                }
                constraint_set
            })
            .and(db, self.constraints, || {
                // For subtyping, we would want to check whether the *top materialization* of
                // `source` is disjoint from the *top materialization* of `negative`. As an
                // optimization, however, we can avoid this explicit transformation here, since
                // our `Type::is_disjoint_from` implementation already only returns true for
                // `T.is_disjoint_from(U)` if the *top materialization* of `T` is disjoint from the
                // *top materialization* of `U`.
                //
                // Note that the implementation of redundancy here may be too strict from a
                // theoretical perspective: under redundancy, `T <: ~U` if `Bottom[T]` is disjoint
                // from `Top[U]` and `Bottom[U]` is disjoint from `Top[T]`. It's possible that this
                // could be improved. For now, however, we err on the side of strictness for our
                // redundancy implementation: a fully complete implementation of redundancy may
                // lead to non-transitivity (highly undesirable); and pragmatically, a full
                // implementation of redundancy may not generally lead to simpler types in many
                // situations.
                let source_ty = match self.relation {
                    TypeRelation::Subtyping | TypeRelation::Redundancy { .. } => source,
                    TypeRelation::Assignability => source.bottom_materialization(db, self.env),
                };
                intersection.negative(db).iter().enumerate().when_all(
                    db,
                    self.constraints,
                    |(index, &negative)| {
                        let negative = match self.relation {
                            TypeRelation::Subtyping | TypeRelation::Redundancy { .. } => negative,
                            TypeRelation::Assignability => {
                                negative.bottom_materialization(db, self.env)
                            }
                        };
                        self.as_disjointness_checker().check_child_pair_at(
                            db,
                            source_ty,
                            negative,
                            ObservationEdge::Identity,
                            ObservationEdge::IntersectionNegative(index),
                        )
                    },
                )
            })
    }

    fn check_source_intersection(
        &self,
        db: &'db dyn Db,
        intersection: IntersectionType<'db>,
        target: Type<'db>,
    ) -> ConstraintSet<'db, 'c> {
        if matches!(target, Type::LiteralValue(_))
            && let Some(alternatives) = intersection.finite_alternative_union(db, self.env)
        {
            return self.check_child_pair(db, alternatives, target);
        }

        // An intersection type is a subtype of another type if at least one of its positive
        // elements is a subtype of that type. If there are no positive elements, we treat `object`
        // as the implicit positive element (e.g., `~str` is semantically `object & ~str`).
        let mut elements_context = vec![];
        let context_tree = self.context_tree.as_ref().filter(|tree| tree.is_enabled());

        let result = intersection
            .positive_elements_or_object(db)
            .enumerate()
            .when_any(db, self.constraints, |(index, element)| {
                let result = self.check_child_pair_at(
                    db,
                    element,
                    target,
                    ObservationEdge::IntersectionPositive(index),
                    ObservationEdge::Identity,
                );
                if let Some(context_tree) = context_tree {
                    let context = context_tree.take();
                    if !context.is_empty() {
                        elements_context.push(context);
                    }
                }
                result
            })
            .or(db, self.constraints, || {
                if self.should_expand_intersection(db, intersection) {
                    self.check_child_pair(
                        db,
                        intersection.with_expanded_typevars_and_newtypes(db, self.env),
                        target,
                    )
                } else {
                    self.never()
                }
            });

        if context_tree.is_some()
            && !elements_context.is_empty()
            && result.is_never_satisfied(db, self.env, self.inferable)
        {
            self.set_context(
                ErrorContext::NoIntersectionElementAssignableToTarget {
                    intersection: Type::Intersection(intersection),
                    target,
                },
                elements_context,
            );
        }

        result
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
        let (source_observed, target_observed) = self.observations.children(source, target);
        let obligation = RelationObligation {
            source,
            target,
            relation: RelationGoal::Relation(self.relation),
            evaluation: self.typevar_evaluation,
            inferable: self.inferable,
            provenance: self.provenance,
            perform_expensive_checks: self.perform_expensive_checks,
            negative: self.constraints.relation_session().is_negative(),
        };
        let result = self.constraints.relation_session().visit_type_pair(
            db,
            self.env,
            self.constraints,
            ObservedRelationObligation {
                obligation,
                source_origin: source_observed.origin(),
                target_origin: target_observed.origin(),
                source_dependency: source_observed.dependency_origins(),
                target_dependency: target_observed.dependency_origins(),
            },
            self.is_context_collection_enabled(),
            work,
        );
        match result {
            Ok(result) => result,
            Err(RelationReentry::Exact) => self.always(),
            Err(RelationReentry::Negative) => ConstraintSet::incomplete(self.constraints),
            Err(RelationReentry::Expanding) => self.recursive_type_pair_fallback(),
        }
    }

    fn recursive_type_pair_fallback(&self) -> ConstraintSet<'db, 'c> {
        // TODO: Recursively-specialized structural types can encode context-free languages,
        // whose inclusion and equivalence are undecidable. No complete fallback exists, but
        // more decidable cases can be recognized here before leaving the obligation unresolved.
        //
        // The growing arguments retain their distinct scopes. Failure to complete their proof
        // establishes neither the relation nor its negation.
        ConstraintSet::incomplete(self.constraints)
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
    /// Class literals have an over-approximated instance projection unless the class is final,
    /// nominal, and non-generic. For `T: (Y, Z)` where `Z` extends `Y`, instance subtyping would
    /// incorrectly simplify `type[T] & <class 'Y'>` to `type[T]`: both `Y` and `Z` instances are
    /// subtypes of `Y`, but only the class object `Y` satisfies `klass is Y`.
    ///
    /// Return `None` for targets without a `.to_instance()` projection, allowing other type-pair
    /// branches to decide their relation.
    fn check_typevar_subclass_relation_to_target(
        &self,
        db: &'db dyn Db,
        source_subclass: SubclassOfType<'db>,
        target: Type<'db>,
    ) -> Option<ConstraintSet<'db, 'c>> {
        let source_i = source_subclass.into_type_var()?;
        let env = self.env;
        if self.is_metaclass_instance(db, target) {
            return Some(self.check_child_pair(
                db,
                source_subclass.to_metaclass_instance(db, env),
                target,
            ));
        }

        let projection = target.to_instance(db, env)?;
        if projection.is_exact() {
            return Some(self.check_child_pair(
                db,
                Type::TypeVar(source_i),
                projection.into_inner(),
            ));
        }

        let source = source_subclass
            .subclass_of()
            .with_transposed_type_var(db, env)
            .into_type_var()?;
        Some(self.check_child_pair(db, Type::TypeVar(source), target))
    }

    /// Compare values derived from the current operands without creating independent proof roots.
    pub(super) fn check_type_pair(
        &self,
        db: &'db dyn Db,
        source: Type<'db>,
        target: Type<'db>,
    ) -> ConstraintSet<'db, 'c> {
        self.check_child_pair(db, source, target)
    }

    /// Descend through the expressions owned by the current proof operands.
    pub(super) fn check_child_pair(
        &self,
        db: &'db dyn Db,
        source: Type<'db>,
        target: Type<'db>,
    ) -> ConstraintSet<'db, 'c> {
        let (source, target) = self.observations.children(source, target);
        self.check_observed_pair(db, source, target)
    }

    /// Compare children selected by structural edges of the current operands.
    pub(super) fn check_child_pair_at(
        &self,
        db: &'db dyn Db,
        source: Type<'db>,
        target: Type<'db>,
        source_edge: ObservationEdge,
        target_edge: ObservationEdge,
    ) -> ConstraintSet<'db, 'c> {
        let (source, target) =
            self.observations
                .children_at(db, self.env, source, target, source_edge, target_edge);
        self.check_observed_pair(db, source, target)
    }

    /// Enter selected structural children while preserving the relation mode and proof session.
    pub(super) fn with_child_operands_at(
        &self,
        db: &'db dyn Db,
        source: Type<'db>,
        target: Type<'db>,
        source_edge: ObservationEdge,
        target_edge: ObservationEdge,
    ) -> Self {
        let (source, target) =
            self.observations
                .children_at(db, self.env, source, target, source_edge, target_edge);
        Self {
            observations: ObservedTypePair::new(source, target),
            ..self.clone()
        }
    }

    /// Bind runtime receivers and lexical `Self` in their declared callable positions.
    pub(super) fn with_callable_self_bindings(
        &self,
        db: &'db dyn Db,
        source: Option<CallableSelfBinding<'db>>,
        target: Option<CallableSelfBinding<'db>>,
    ) -> Self {
        Self {
            observations: self
                .observations
                .bind_callable_self(db, self.env, source, target),
            ..self.clone()
        }
    }

    /// Map an already selected child view without losing its declaration environment.
    pub(super) fn with_operand_mappings(
        &self,
        db: &'db dyn Db,
        source_mapping: Option<&super::TypeMapping<'_, 'db>>,
        target_mapping: Option<&super::TypeMapping<'_, 'db>>,
    ) -> Self {
        Self {
            observations: self.observations.map(
                db,
                source_mapping,
                target_mapping,
                self.materialization_visitor,
            ),
            ..self.clone()
        }
    }

    pub(super) fn reversed(&self) -> Self {
        Self {
            observations: self.observations.reversed(),
            ..self.clone()
        }
    }

    pub(super) fn with_source_operands(&self) -> Self {
        Self {
            observations: self.observations.source_twice(),
            ..self.clone()
        }
    }

    pub(super) fn with_target_operands(&self) -> Self {
        Self {
            observations: self.observations.target_twice(),
            ..self.clone()
        }
    }

    pub(super) fn check_observed_pair(
        &self,
        db: &'db dyn Db,
        source: ObservedType<'db>,
        target: ObservedType<'db>,
    ) -> ConstraintSet<'db, 'c> {
        let source_ty = source.ty;
        let target_ty = target.ty;
        let checker = Self {
            observations: ObservedTypePair::new(source, target),
            ..self.clone()
        };
        checker.check_type_pair_observed_impl(db, source_ty, target_ty)
    }

    fn check_type_pair_observed_impl(
        &self,
        db: &'db dyn Db,
        source: Type<'db>,
        target: Type<'db>,
    ) -> ConstraintSet<'db, 'c> {
        // Reflexivity and lazy constraints can bypass the RecursiveVar match arm below.
        source.assert_not_recursive_var();
        target.assert_not_recursive_var();
        if matches!(source, Type::Deferred(_)) {
            return self
                .observations
                .source
                .unfold_in_context(db, self.env, &self.context())
                .map_or_else(
                    || ConstraintSet::incomplete(self.constraints),
                    |source| self.check_observed_pair(db, source, self.observations.target.clone()),
                );
        }
        if matches!(target, Type::Deferred(_)) {
            return self
                .observations
                .target
                .unfold_in_context(db, self.env, &self.context())
                .map_or_else(
                    || ConstraintSet::incomplete(self.constraints),
                    |target| self.check_observed_pair(db, self.observations.source.clone(), target),
                );
        }
        if let Some(source) = source.materialized_divergent_fallback() {
            return self.check_child_pair(db, source, target);
        }

        if let Some(target) = target.materialized_divergent_fallback() {
            return self.check_child_pair(db, source, target);
        }

        // Subtyping implies assignability, so if subtyping is reflexive and the two types are
        // equal, it is both a subtype and assignable. Assignability is always reflexive.
        //
        // Note that we could do a full equivalence check here, but that would be both expensive
        // and unnecessary. This early return is only an optimisation.
        if source == target && self.relation.can_safely_assume_reflexivity(source) {
            return self.always();
        }

        if let (Some(source), Some(target)) = (
            source.as_protocol_instance(db),
            target.as_protocol_instance(db),
        ) && self.protocol_materializations_relate(db, source, target)
        {
            return self.always();
        }

        let env = self.env;

        // With lazy evaluation, comparisons with a type variable are translated directly into a
        // constraint set.
        if self.typevar_evaluation == TypeVarEvaluation::Lazy {
            // A typevar satisfies a relation when...it satisfies the relation. Yes that's a
            // tautology! We're moving the caller's subtyping/assignability requirement into a
            // constraint set. If the typevar has an upper bound or constraints, then the relation
            // only has to hold when the typevar has a valid specialization (i.e., one that
            // satisfies the upper bound/constraints).
            if let Type::TypeVar(bound_typevar) = source {
                let upper = if self.relation.is_subtyping() {
                    self.observations.target.apply_mapping(
                        db,
                        &super::TypeMapping::Materialize(super::MaterializationKind::Bottom),
                        self.materialization_visitor,
                    )
                } else {
                    self.observations.target.clone()
                };
                return ConstraintSet::constrain_typevar_upper_bound_observed(
                    db,
                    env,
                    self.constraints,
                    self.provenance,
                    bound_typevar,
                    &upper,
                );
            } else if let Type::TypeVar(bound_typevar) = target {
                let lower = if self.relation.is_subtyping() {
                    self.observations.source.apply_mapping(
                        db,
                        &super::TypeMapping::Materialize(super::MaterializationKind::Top),
                        self.materialization_visitor,
                    )
                } else {
                    self.observations.source.clone()
                };
                return ConstraintSet::constrain_typevar_lower_bound_observed(
                    db,
                    env,
                    self.constraints,
                    self.provenance,
                    bound_typevar,
                    &lower,
                );
            }
        }

        match (source, target) {
            (Type::Deferred(_), _) | (_, Type::Deferred(_)) => {
                ConstraintSet::incomplete(self.constraints)
            }
            (Type::RecursiveVar(_), _) | (_, Type::RecursiveVar(_)) => {
                unreachable!("semantic operation on an unbound recursive variable")
            }
            // Everything is a subtype of `object`.
            (_, Type::NominalInstance(target)) if target.is_object() => self.always(),
            (_, Type::ProtocolInstance(target)) if target.is_equivalent_to_object(db) => {
                self.always()
            }

            // `Never` is the bottom type, the empty set.
            // It is a subtype of all other types.
            (Type::Never, _) => self.always(),

            (Type::TypeVar(source_typevar), Type::TypeVar(target_typevar))
                if source_typevar.is_same_typevar_as(db, target_typevar) =>
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
                self.with_recursion_guard(db, source, target, || {
                    let by_arguments = if let Type::Recursive(target_recursive) = target {
                        self.when_recursive_types_relate_by_arguments(
                            db,
                            source_recursive,
                            target_recursive,
                        )
                    } else {
                        self.never()
                    };
                    by_arguments.or(db, self.constraints, || {
                        self.observations
                            .children(source, target)
                            .0
                            .unfold(db, self.env)
                            .map(|source_unfolded| {
                                let target = self.observations.children(source, target).1;
                                self.check_observed_pair(db, source_unfolded, target)
                            })
                            .unwrap_or(ConstraintSet::from_bool(
                                self.constraints,
                                self.relation.is_assignability(),
                            ))
                    })
                })
            }

            (_, Type::Recursive(_)) => self.with_recursion_guard(db, source, target, || {
                self.observations
                    .children(source, target)
                    .1
                    .unfold(db, self.env)
                    .map(|target_unfolded| {
                        let source = self.observations.children(source, target).0;
                        self.check_observed_pair(db, source, target_unfolded)
                    })
                    .unwrap_or(ConstraintSet::from_bool(
                        self.constraints,
                        self.relation.is_assignability(),
                    ))
            }),

            // Instances of classes that inherit from an explicit `Any` base retain their nominal
            // identity and precise members, but have the same assignability as `Any`.
            (Type::NominalInstance(source), _)
                if self.relation.is_assignability() && source.inherits_from_explicit_any() =>
            {
                self.always()
            }

            (Type::TypeAlias(_), _) => self.with_recursion_guard(db, source, target, || {
                self.observations
                    .children(source, target)
                    .0
                    .unfold(db, self.env)
                    .map(|source_unfolded| {
                        let target = self.observations.children(source, target).1;
                        self.check_observed_pair(db, source_unfolded, target)
                    })
                    .unwrap_or_else(|| ConstraintSet::incomplete(self.constraints))
            }),

            (_, Type::TypeAlias(_)) => self.with_recursion_guard(db, source, target, || {
                self.observations
                    .children(source, target)
                    .1
                    .unfold(db, self.env)
                    .map(|target_unfolded| {
                        let source = self.observations.children(source, target).0;
                        self.check_observed_pair(db, source, target_unfolded)
                    })
                    .unwrap_or_else(|| ConstraintSet::incomplete(self.constraints))
            }),

            // Annotation unions retain type aliases so recursive aliases can be represented.
            // Normalize direct alias elements together before checking the union so reductions
            // that depend on multiple elements, such as all members of an enum, are visible.
            (_, Type::Union(union))
                if union.has_aliases(db)
                    && !self
                        .observations
                        .children(source, target)
                        .1
                        .is_normalized_union() =>
            {
                self.with_recursion_guard(db, source, target, || {
                    let (source, target) = self.observations.children(source, target);
                    let target = self.observe_union(db, &target);
                    self.check_observed_pair(db, source, target)
                })
            }

            (Type::TypeForm(source_typeform), Type::TypeForm(target_typeform)) => self
                .with_recursion_guard(db, source, target, || {
                    self.check_child_pair(
                        db,
                        source_typeform.type_argument(db),
                        target_typeform.type_argument(db),
                    )
                }),

            (Type::SubclassOf(source_subclass), Type::TypeForm(target_typeform)) => self
                .check_child_pair(
                    db,
                    source_subclass.to_instance(db, env),
                    target_typeform.type_argument(db),
                ),

            (Type::NominalInstance(source_instance), Type::TypeForm(target_typeform))
                if source_instance.has_known_class(db, KnownClass::Type) =>
            {
                self.check_child_pair(db, Type::object(), target_typeform.type_argument(db))
            }

            (Type::ClassLiteral(source_class), Type::TypeForm(target_typeform)) => self
                .check_child_pair(
                    db,
                    Type::instance(db, env, source_class.default_specialization(db)),
                    target_typeform.type_argument(db),
                ),

            (Type::GenericAlias(source_alias), Type::TypeForm(target_typeform)) => self
                .check_child_pair(
                    db,
                    Type::instance(db, env, ClassType::Generic(source_alias)),
                    target_typeform.type_argument(db),
                ),

            (Type::KnownInstance(source_instance), Type::TypeForm(target_typeform))
                if let Some(source_argument) = source_instance.type_form_argument(db, env) =>
            {
                self.check_child_pair(db, source_argument, target_typeform.type_argument(db))
            }

            (Type::SpecialForm(source_form), Type::TypeForm(target_typeform)) => source_form
                .type_form_argument(db, env)
                .when_some_and(db, self.constraints, |source_argument| {
                    self.check_child_pair(db, source_argument, target_typeform.type_argument(db))
                }),

            (Type::GenericAlias(_), Type::NominalInstance(target_instance))
                if target_instance.has_known_class(db, KnownClass::GenericAlias) =>
            {
                self.always()
            }

            (Type::EnumComplement(complement), Type::LiteralValue(_) | Type::Union(_)) => {
                self.check_child_pair(db, complement.remaining_literal_union(db, env), target)
            }

            (Type::EnumComplement(complement), _) => {
                self.check_child_pair(db, complement.to_intersection(db, env), target)
            }

            (_, Type::EnumComplement(complement)) => {
                self.check_child_pair(db, source, complement.to_intersection(db, env))
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
                field
                    .default_type(db)
                    .when_none_or(db, self.constraints, |default_type| {
                        self.check_child_pair(db, default_type, target)
                    })
                    .and(db, self.constraints, || {
                        field
                            .converter(db)
                            .map(|(_, output_ty)| output_ty)
                            .when_none_or(db, self.constraints, |converter_output_type| {
                                self.check_child_pair(db, converter_output_type, target)
                            })
                    })
            }

            (
                Type::KnownInstance(KnownInstanceType::Annotated(source)),
                Type::KnownInstance(KnownInstanceType::Annotated(target)),
            ) if source.inner(db).is_recursive_divergent()
                || target.inner(db).is_recursive_divergent() =>
            {
                // Recursive inference preserves an Annotated object while replacing its wrapped
                // type with Divergent. That approximation must accept another unfolding of the
                // object, without making distinct concrete Annotated values interchangeable.
                self.check_relation_in_invariant_position(
                    db,
                    source.inner(db),
                    None,
                    target.inner(db),
                    None,
                )
            }

            // The read-only `__func__` and `__wrapped__` attributes expose the complete wrapped
            // object, so its attributes matter here as well as its call signature.
            (
                Type::KnownInstance(KnownInstanceType::MethodWrapper(source_wrapper)),
                Type::KnownInstance(KnownInstanceType::MethodWrapper(target_wrapper)),
            ) if source_wrapper.kind(db) == target_wrapper.kind(db) => {
                self.with_recursion_guard(db, source, target, || {
                    self.check_child_pair(
                        db,
                        source_wrapper.wrapped(db),
                        target_wrapper.wrapped(db),
                    )
                })
            }

            (
                Type::KnownInstance(KnownInstanceType::FunctoolsPartial(source_partial)),
                Type::KnownInstance(KnownInstanceType::FunctoolsPartial(target_partial)),
            )
            | (
                Type::KnownInstance(KnownInstanceType::FunctoolsPartialCall(source_partial)),
                Type::KnownInstance(KnownInstanceType::FunctoolsPartialCall(target_partial)),
            ) => self.with_recursion_guard(db, source, target, || {
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
                self.check_child_pair(
                    db,
                    source_partial.wrapped(db).inner(db),
                    target_partial.wrapped(db).inner(db),
                )
                .and(db, self.constraints, || {
                    self.check_callable_pair(
                        db,
                        source_partial.partial(db),
                        target_partial.partial(db),
                    )
                })
            }),

            (
                Type::KnownInstance(KnownInstanceType::Sentinel(source_sentinel)),
                Type::KnownInstance(KnownInstanceType::Sentinel(target_sentinel)),
            ) => ConstraintSet::from_bool(
                self.constraints,
                source_sentinel.is_same_sentinel(db, target_sentinel),
            ),

            // A nominal descriptor annotation specifies the wrapped callable through `__func__`.
            // Comparing that contract directly preserves overloads and avoids replacing the
            // wrapped callable's parameter and return types with the default specialization.
            (
                Type::KnownInstance(KnownInstanceType::MethodWrapper(wrapper)),
                Type::NominalInstance(target_instance),
            ) if target_instance
                .class(db, env)
                .is_known(db, wrapper.class(db)) =>
            {
                self.with_recursion_guard(db, source, target, || {
                    let Some(target_function) = target
                        .member_lookup_with_policy(
                            db,
                            env,
                            "__func__",
                            MemberLookupPolicy::NO_INSTANCE_FALLBACK,
                        )
                        .place
                        .ignore_possibly_undefined()
                    else {
                        return self.never();
                    };
                    self.check_child_pair(db, wrapper.wrapped(db), target_function)
                })
            }

            // When checking `FunctoolsPartial <: functools.partial[T]`, we need to specialize
            // the nominal instance with the partial's return type so the check is precise.
            (
                Type::KnownInstance(KnownInstanceType::FunctoolsPartial(partial)),
                Type::NominalInstance(target_instance),
            ) if target_instance
                .class(db, env)
                .is_known(db, KnownClass::FunctoolsPartial) =>
            {
                let specialized = partial.partial(db).into_functools_partial_instance(db, env);
                self.check_child_pair(db, specialized, target)
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
                    TypeRelation::Subtyping => false,
                    TypeRelation::Assignability => true,
                    TypeRelation::Redundancy { .. } => match target {
                        Type::Dynamic(_) => true,
                        Type::Union(union) => union.elements(db).iter().any(Type::is_dynamic),
                        _ => false,
                    },
                },
            ),
            (_, Type::Dynamic(_)) => ConstraintSet::from_bool(
                self.constraints,
                match self.relation {
                    TypeRelation::Subtyping => false,
                    TypeRelation::Assignability => true,
                    TypeRelation::Redundancy { .. } => match source {
                        Type::Dynamic(_) => true,
                        Type::Intersection(intersection) => {
                            // If a `Divergent` type is involved, it must not be eliminated.
                            intersection
                                .positive(db)
                                .iter()
                                .any(Type::is_non_divergent_dynamic)
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
                    && union.elements(db).contains(&source) =>
            {
                self.always()
            }

            // A similar rule applies in reverse to intersection types.
            (Type::Intersection(intersection), _)
                if self.relation.can_safely_assume_reflexivity(target)
                    && intersection.positive(db).contains(&target) =>
            {
                self.always()
            }
            (Type::Intersection(intersection), _)
                if self.relation.is_assignability()
                    && intersection.positive(db).iter().any(Type::is_dynamic) =>
            {
                // If the intersection contains `Any`/`Unknown`/`@Todo`, it is assignable to any type.
                // `Any` could materialize to `Never`, `Never & T & ~S` simplifies to `Never` for any
                // `T` and any `S`, and `Never` is a subtype of all types.
                self.always()
            }
            (Type::Intersection(intersection), _)
                if self.relation.can_safely_assume_reflexivity(target)
                    && intersection.negative(db).contains(&target) =>
            {
                self.never()
            }

            // When `A` has an exact instance projection, `type[T]` is a subtype of `A` if `T`
            // is a subtype of that projection. If `A` is a metaclass instance (instance of a specific
            // subclass of `type`), we instead compare in the metaclass-instance domain, since
            // collapsing `A` through `to_instance()` would erase it to `object` (we have no
            // precise representation for "all instances of any classes with a given metaclass").
            (Type::SubclassOf(subclass_of), _)
                if let Some(constraint_set) =
                    self.check_typevar_subclass_relation_to_target(db, subclass_of, target) =>
            {
                constraint_set
            }

            // And vice versa. (No special metaclass handling is needed in this direction, since
            // "collapse to 'object'" in this case is a sound over-approximation.)
            (_, Type::SubclassOf(subclass_of))
                if let Some(type_var) = subclass_of.into_type_var()
                    && let Some(instance) = source.to_instance_approximation(db, env) =>
            {
                self.check_child_pair(db, instance, Type::TypeVar(type_var))
            }

            // A TypeVarTuple specialization is represented by one tuple value. Keep inferable
            // TypeVarTuples bare for constraint solving, but compare fixed symbolic values using
            // the same tuple relation as concrete specializations.
            (Type::TypeVar(bound_typevar), target)
                if !bound_typevar.is_inferable(db, self.inferable)
                    && bound_typevar.is_typevartuple(db)
                    && target.exact_tuple_instance_spec(db).is_some() =>
            {
                self.check_child_pair(
                    db,
                    Type::tuple(TupleType::unpacked_typevartuple(db, env, bound_typevar)),
                    target,
                )
            }
            // A fixed tuple cannot satisfy every specialization of a non-inferable TypeVarTuple.
            // Let it reach the ordinary rejection below; expanding the target would repeat the
            // same tuple comparison and cause the recursion guard to accept it.
            (source, Type::TypeVar(bound_typevar))
                if !bound_typevar.is_inferable(db, self.inferable)
                    && bound_typevar.is_typevartuple(db)
                    && source
                        .exact_tuple_instance_spec(db)
                        .is_some_and(|spec| spec.is_variadic()) =>
            {
                self.check_child_pair(
                    db,
                    source,
                    Type::tuple(TupleType::unpacked_typevartuple(db, env, bound_typevar)),
                )
            }

            // A gradual `ParamSpec` value (`...`) is assignability-consistent with any concrete
            // `ParamSpec` value. This only applies to fixed `ParamSpec` values in already-
            // specialized generic aliases; inferable `ParamSpec`s are handled by the inference
            // paths below.
            (Type::TypeVar(bound_typevar), Type::Callable(other))
            | (Type::Callable(other), Type::TypeVar(bound_typevar))
                if self.is_eager_assignability()
                    && !bound_typevar.is_inferable(db, self.inferable)
                    && bound_typevar.domain(db) == TypeVarDomain::ParameterSignature
                    && Self::is_gradual_paramspec_value(db, other) =>
            {
                self.always()
            }

            // Compare fixed `ParamSpec`s with the endpoints of the materialization range of `...`:
            // its bottom is below every `ParamSpec`, and its top is above every `ParamSpec`.
            (Type::TypeVar(bound_typevar), Type::Callable(other))
                if !bound_typevar.is_inferable(db, self.inferable)
                    && bound_typevar.domain(db) == TypeVarDomain::ParameterSignature
                    && other.is_top_paramspec_value(db) =>
            {
                self.always()
            }

            (Type::Callable(other), Type::TypeVar(bound_typevar))
                if !bound_typevar.is_inferable(db, self.inferable)
                    && bound_typevar.domain(db) == TypeVarDomain::ParameterSignature
                    && other.is_bottom_paramspec_value(db) =>
            {
                self.always()
            }

            // If the typevar is constrained, there must be multiple constraints, and the typevar
            // might be specialized to any one of them. However, the constraints do not have to be
            // disjoint, which means an lhs type might be a subtype of all of the constraints.
            (_, Type::TypeVar(bound_typevar))
                if !bound_typevar.is_inferable(db, self.inferable)
                    && let constraints = bound_typevar
                        .typevar(db)
                        .constraints(db, env)
                        .when_some_and(db, self.constraints, |constraints| {
                            constraints.iter().when_all(db, self.constraints, |c| {
                                self.check_child_pair(db, source, *c)
                            })
                        })
                    && !constraints.is_never_satisfied(db, env, self.inferable) =>
            {
                constraints
            }

            (Type::TypeVar(bound_typevar), _) if bound_typevar.is_inferable(db, self.inferable) => {
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
                if source.is_object() && !typevar.is_inferable(db, self.inferable) =>
            {
                self.never()
            }

            (Type::NewTypeInstance(source_newtype), Type::NewTypeInstance(target_newtype)) => {
                self.check_newtype_pair(db, source_newtype, target_newtype)
            }

            (Type::Union(union), _) => self.check_source_union(db, union, target),
            (_, Type::Union(union)) => self.check_target_union(db, source, union),

            // If both sides are intersections we need to handle the right side first
            // (A & B & C) is a subtype of (A & B) because the left is a subtype of both A and B,
            // but none of A, B, or C is a subtype of (A & B).
            (_, Type::Intersection(intersection)) => {
                self.check_target_intersection(db, source, intersection)
            }

            // Check an inferable target's bound before splitting a source intersection.
            // For `T: A & B`, neither `A` nor `B` alone need satisfy the bound, but `A & B` does.
            (_, Type::TypeVar(typevar))
                if self.is_eager_assignability() && typevar.is_inferable(db, self.inferable) =>
            {
                // TODO: record the unification constraints
                typevar.typevar(db).upper_bound(db, env).when_none_or(
                    db,
                    self.constraints,
                    |bound| self.check_child_pair(db, source, bound),
                )
            }

            (Type::Intersection(intersection), _) => {
                self.check_source_intersection(db, intersection, target)
            }

            // A fully static typevar is a subtype of its upper bound, and to something similar to
            // the union of its constraints. An unbound, unconstrained, fully static typevar has an
            // implicit upper bound of `object` (which is handled above).
            (Type::TypeVar(bound_typevar), _)
                if !bound_typevar.is_inferable(db, self.inferable)
                    && let Some(bound_or_constraints) =
                        bound_typevar.typevar(db).bound_or_constraints(db, env) =>
            {
                // Upcast the type variable directly rather than promoting it to its upper bound,
                // such that `Self` in the callable signature refers back to the original type variable.
                if let Type::Callable(target_callable) = target
                    && let Some(callables) = source.try_upcast_to_callable_in_context(
                        db,
                        env,
                        UpcastPolicy::from(self.relation),
                        self.operands().source.unchanged_or_unresolved(source),
                        self.context(),
                    )
                {
                    self.with_recursion_guard(db, source, target, || {
                        self.check_callables_vs_callable(db, &callables, target_callable)
                    })
                } else {
                    self.check_source_typevar_bounds(db, bound_or_constraints, target)
                }
            }

            // `Never` is the bottom type, the empty set.
            (_, Type::Never) => self.never(),

            // Other than the special cases checked above, no other types are a subtype of a
            // typevar, since there's no guarantee what type the typevar will be specialized to.
            // (If the typevar is bounded, it might be specialized to a smaller type than the
            // bound. This is true even if the bound is a final class, since the typevar can still
            // be specialized to `Never`.)
            (_, Type::TypeVar(bound_typevar))
                if !bound_typevar.is_inferable(db, self.inferable) =>
            {
                self.never()
            }

            // TODO: Infer specializations here
            (_, Type::TypeVar(typevar)) if typevar.is_inferable(db, self.inferable) => self.never(),
            (Type::TypeVar(bound_typevar), _) => {
                // All inferable cases should have been handled above
                assert!(!bound_typevar.is_inferable(db, self.inferable));
                self.never()
            }

            // All other `NewType` assignments fall back to the concrete base type.
            // This case must come after the TypeVar cases above, so that when checking
            // `NewType <: TypeVar`, we use the TypeVar handling rather than falling back
            // to the NewType's concrete base type.
            (Type::NewTypeInstance(source_newtype), _) => {
                self.check_child_pair(db, source_newtype.concrete_base_type(db), target)
            }

            // Note that the definition of `Type::AlwaysFalsy` depends on the return value of `__bool__`.
            // If `__bool__` always returns True or False, it can be treated as a subtype of `AlwaysTruthy` or `AlwaysFalsy`, respectively.
            (_, Type::AlwaysFalsy) => {
                ConstraintSet::from_bool(self.constraints, source.bool(db, env).is_always_false())
            }
            (_, Type::AlwaysTruthy) => {
                ConstraintSet::from_bool(self.constraints, source.bool(db, env).is_always_true())
            }
            // Currently, the only supertype of `AlwaysFalsy` and `AlwaysTruthy` is the universal set (object instance).
            (Type::AlwaysFalsy | Type::AlwaysTruthy, _) => {
                self.with_recursion_guard(db, source, target, || {
                    self.check_child_pair(db, Type::object(), target)
                })
            }

            // These clauses handle type variants that include function literals. A function
            // literal is the subtype of itself, and not of any other function literal. However,
            // our representation of a function literal includes any specialization that should be
            // applied to the signature. Different specializations of the same function literal are
            // only subtypes of each other if they result in the same signature.
            (Type::FunctionLiteral(source_function), Type::FunctionLiteral(target_function)) => {
                self.check_function_pair(db, source_function, target_function)
            }
            (
                Type::KnownInstance(
                    KnownInstanceType::FunctoolsPartial(source_partial)
                    | KnownInstanceType::FunctoolsPartialCall(source_partial),
                ),
                Type::FunctionLiteral(target_function),
            ) if matches!(self.relation, TypeRelation::Assignability) => {
                self.with_recursion_guard(db, source, target, || {
                    self.check_callable_signature_pair(
                        db,
                        source_partial.partial(db).signatures(db),
                        target_function.into_callable_type(db).signatures(db),
                    )
                })
            }
            (Type::BoundMethod(source_method), Type::BoundMethod(target_method)) => {
                self.check_bound_method_pair(db, source_method, target_method)
            }
            (Type::KnownBoundMethod(source_method), Type::KnownBoundMethod(target_method)) => {
                self.check_known_bound_method_pair(db, source_method, target_method)
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

            (Type::Callable(source_callable), Type::Callable(target_callable)) => self
                .with_recursion_guard(db, source, target, || {
                    self.check_callable_pair(db, source_callable, target_callable)
                }),

            (
                Type::Callable(source_callable),
                Type::KnownInstance(KnownInstanceType::FunctoolsPartialCall(target_partial)),
            ) if self.relation.is_assignability() => {
                self.with_recursion_guard(db, source, target, || {
                    self.check_callable_pair(db, source_callable, target_partial.partial(db))
                })
            }

            (_, Type::Callable(target_callable)) => {
                self.with_recursion_guard(db, source, target, || {
                    // Bound methods can be assigned to inferred function-like callback types,
                    // but are not nominal subtypes of functions.
                    let target_callable = if self.relation.is_assignability()
                        && matches!(source, Type::BoundMethod(_))
                        && target_callable.is_function_like(db)
                    {
                        target_callable.into_regular(db)
                    } else {
                        target_callable
                    };
                    let Some(callables) = source.try_upcast_to_callable_in_context(
                        db,
                        env,
                        UpcastPolicy::from(self.relation),
                        self.operands().source.unchanged_or_unresolved(source),
                        self.context(),
                    ) else {
                        return self.never();
                    };

                    let result = self.check_callables_vs_callable(db, &callables, target_callable);

                    if let Some(context) = self.report_context()
                        && self.should_provide_callable_upcast_context(source)
                        && result.is_never_satisfied(db, env, self.inferable)
                    {
                        context.push(ErrorContext::InferredCallableType {
                            source,
                            callable: callables.to_type(db, env),
                        });
                    }

                    result
                })
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
                self.check_child_pair(db, KnownClass::Type.to_instance(db, env), target)
            }

            (_, Type::ProtocolInstance(target_proto)) => {
                if let Some(result) =
                    self.try_parametric_protocol_relation(db, source, target_proto)
                {
                    return result;
                }
                self.with_recursion_guard(db, source, target, || {
                    self.check_type_satisfies_protocol(db, source, target_proto)
                })
            }

            // A protocol instance can never be a subtype of a nominal type, with the *sole* exception of `object`.
            (Type::ProtocolInstance(_), _) => self.never(),

            (Type::TypedDict(source_td), Type::TypedDict(target_td)) => {
                self.with_recursion_guard(db, source, target, || {
                    self.check_typeddict_pair(db, source_td, target_td)
                })
            }

            (Type::TypedDict(typed_dict), _) => {
                self.with_recursion_guard(db, source, target, || {
                    let dict_value_type =
                        typed_dict.dict_value_type_if(db, |field_ty, extra_ty| {
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
                                checker.check_child_pair(db, field_ty, extra_ty).and(
                                    db,
                                    self.constraints,
                                    || checker.check_child_pair(db, extra_ty, field_ty),
                                )
                            } else {
                                self.as_equivalence_checker()
                                    .check_child_pair(db, field_ty, extra_ty)
                            };
                            result.is_always_satisfied(db, env, TypeVarSet::None)
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
                    let result = self.check_child_pair(db, fallback, target);

                    if let Some(context) = self.report_context()
                        && result.is_never_satisfied(db, env, self.inferable)
                        && let Type::NominalInstance(instance) = target
                    {
                        match instance.class(db, env).known(db) {
                            Some(KnownClass::Dict) => {
                                context
                                    .push(ErrorContext::TypedDictNotAssignableToDict(typed_dict));
                            }
                            Some(KnownClass::Mapping)
                                if typed_dict.openness(db).is_implicitly_open() =>
                            {
                                let field_types =
                                    typed_dict.items(db).values().map(|field| field.declared_ty);
                                let mapping_fallback_spec = &[
                                    KnownClass::Str.to_instance(db, env),
                                    UnionType::from_elements(db, env, field_types),
                                ];

                                let closed_typeddict_fallback = KnownClass::Mapping
                                    .to_specialized_instance(db, env, mapping_fallback_spec);

                                if self
                                    .check_child_pair(db, closed_typeddict_fallback, target)
                                    .is_always_satisfied(db, env, self.inferable)
                                {
                                    let context_element =
                                        ErrorContext::OpenTypedDictNotAssignableToMapping {
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
                })
            }

            // A non-`TypedDict` cannot subtype a `TypedDict`
            (_, Type::TypedDict(_)) => self.never(),

            // A string literal `Literal["abc"]` is assignable to `str` *and* to
            // `Sequence[Literal["a", "b", "c"]]` because strings are sequences of their characters.
            (Type::LiteralValue(literal), Type::NominalInstance(instance))
                if let Some(value) = literal.as_string() =>
            {
                let target_class = instance.class(db, env);

                if target_class.is_known(db, KnownClass::Str) {
                    return self.always();
                }

                if let Some(sequence_class) = KnownClass::Sequence.try_to_class_literal(db, env)
                    && !sequence_class
                        .iter_mro(db, None)
                        .filter_map(ClassBase::into_class)
                        .map(|class| class.class_literal(db))
                        .contains(&target_class.class_literal(db))
                {
                    return self.never();
                }

                let chars: FxHashSet<char> = value.value(db).chars().collect();

                let spec = match chars.len() {
                    0 => Type::Never,
                    1 => Type::single_char_string_literal(db, *chars.iter().next().unwrap()),
                    _ => {
                        // Optimisation: since we know this union will only include string-literal types,
                        // avoid eagerly creating string-literal types when unnecessary, and avoid going
                        // via the union-builder.
                        let union_elements: Box<[Type<'db>]> = chars
                            .iter()
                            .map(|c| Type::single_char_string_literal(db, *c))
                            .collect();
                        Type::Union(UnionType::new(db, union_elements, RecursivelyDefined::No))
                    }
                };

                KnownClass::Sequence
                    .to_specialized_class_type(db, env, &[spec])
                    .when_some_and(db, self.constraints, |sequence| {
                        self.check_class_pair(db, sequence, target_class)
                    })
            }

            (Type::LiteralValue(literal), _) if literal.is_string() => self.never(),

            // A bytes literal `Literal[b"abc"]` is assignable to `bytes` *and* to
            // `Sequence[Literal[97, 98, 99]]` because bytes are sequences of integers.
            (Type::LiteralValue(literal), Type::NominalInstance(instance))
                if let Some(value) = literal.as_bytes() =>
            {
                let target_class = instance.class(db, env);

                if target_class.is_known(db, KnownClass::Bytes) {
                    return self.always();
                }

                if let Some(sequence_class) = KnownClass::Sequence.try_to_class_literal(db, env)
                    && !sequence_class
                        .iter_mro(db, None)
                        .filter_map(ClassBase::into_class)
                        .map(|class| class.class_literal(db))
                        .contains(&target_class.class_literal(db))
                {
                    return self.never();
                }

                let ints: FxHashSet<i64> = value
                    .value(db)
                    .iter()
                    .map(|byte| i64::from(*byte))
                    .collect();

                let spec = match ints.len() {
                    0 => Type::Never,
                    1 => Type::int_literal(*ints.iter().next().unwrap()),
                    _ => {
                        let union_elements: Box<[Type<'db>]> =
                            ints.iter().map(|int| Type::int_literal(*int)).collect();
                        Type::Union(UnionType::new(db, union_elements, RecursivelyDefined::No))
                    }
                };

                KnownClass::Sequence
                    .to_specialized_class_type(db, env, &[spec])
                    .when_some_and(db, self.constraints, |sequence| {
                        self.check_class_pair(db, sequence, target_class)
                    })
            }

            (Type::LiteralValue(literal), _) if literal.is_bytes() => self.never(),

            // An instance is a subtype of an enum literal, if it is an instance of the enum class
            // and the enum has only one member.
            (Type::NominalInstance(_), Type::LiteralValue(literal))
                if let Some(target_enum_literal) = literal.as_enum() =>
            {
                if target_enum_literal.enum_class_instance(db, env) != source {
                    self.never()
                } else {
                    ConstraintSet::from_bool(
                        self.constraints,
                        is_single_member_enum(db, target_enum_literal.enum_class(db)),
                    )
                }
            }

            // Except for the special `BytesLiteral`, `LiteralString`, and string literal cases above,
            // most `Literal` types delegate to their instance fallbacks
            // unless `source` is exactly equivalent to `target` (handled above)
            (Type::ModuleLiteral(_) | Type::LiteralValue(_) | Type::FunctionLiteral(_), _) => {
                source.literal_fallback_instance(db, env).when_some_and(
                    db,
                    self.constraints,
                    |source_instance| self.check_child_pair(db, source_instance, target),
                )
            }

            // The same reasoning applies for these special callable types:
            (Type::BoundMethod(_), _) => {
                self.check_child_pair(db, KnownClass::MethodType.to_instance(db, env), target)
            }
            (Type::KnownBoundMethod(method), _) => {
                self.check_child_pair(db, method.class().to_instance(db, env), target)
            }
            (Type::WrapperDescriptor(_), _) => self.check_child_pair(
                db,
                KnownClass::WrapperDescriptorType.to_instance(db, env),
                target,
            ),

            (Type::DataclassDecorator(_) | Type::DataclassTransformer(_), _) => {
                // TODO: Implement subtyping using an equivalent `Callable` type.
                self.never()
            }

            // `TypeIs` is invariant.
            (Type::TypeIs(source), Type::TypeIs(target)) => self
                .check_relation_in_invariant_position(
                    db,
                    source.type_argument(db),
                    source.materialization_kind(db),
                    target.type_argument(db),
                    target.materialization_kind(db),
                ),

            // `TypeGuard` is covariant.
            (Type::TypeGuard(source), Type::TypeGuard(target)) => {
                self.check_child_pair(db, source.return_type(db), target.return_type(db))
            }

            // `TypeIs[T]` and `TypeGuard[T]` are subtypes of `bool`.
            (Type::TypeIs(_) | Type::TypeGuard(_), _) => {
                self.check_child_pair(db, KnownClass::Bool.to_instance(db, env), target)
            }

            (Type::Callable(callable), _) if let Some(class) = callable.runtime_class(db) => {
                self.check_child_pair(db, class.to_instance(db, env), target)
            }

            (Type::Callable(_), _) => self.never(),

            (Type::BoundSuper(source), Type::BoundSuper(target)) => self
                .as_equivalence_checker()
                .check_bound_super_pair(db, source, target),

            (Type::BoundSuper(_), _) => {
                self.check_child_pair(db, KnownClass::Super.to_instance(db, env), target)
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
                    SubclassOfInner::Protocol(target_protocol) => self
                        .check_meta_type_satisfies_protocol(
                            db,
                            Type::ClassLiteral(source_cls),
                            target_protocol,
                        ),
                    target => target
                        .into_class(db, env)
                        .map(|target_cls| {
                            self.check_class_pair(
                                db,
                                source_cls.default_specialization(db),
                                target_cls,
                            )
                        })
                        .unwrap_or_else(|| {
                            ConstraintSet::from_bool(
                                self.constraints,
                                self.relation.is_assignability(),
                            )
                        }),
                }
            }

            // Similarly, `<class 'C'>` is assignable to `<class 'C[...]'>` (a generic-alias type)
            // if the default specialization of `C` is assignable to `C[...]`. This scenario occurs
            // with final generic types, where `type[C[...]]` is simplified to the generic-alias
            // type `<class 'C[...]'>`, due to the fact that `C[...]` has no subclasses.
            (Type::ClassLiteral(source_cls), Type::GenericAlias(target_alias)) => self
                .check_class_pair(
                    db,
                    source_cls.default_specialization(db),
                    ClassType::Generic(target_alias),
                ),

            // For generic aliases, we delegate to the underlying class type.
            (Type::GenericAlias(source_alias), Type::GenericAlias(target_alias)) => self
                .check_class_pair(
                    db,
                    ClassType::Generic(source_alias),
                    ClassType::Generic(target_alias),
                ),

            (Type::GenericAlias(source_alias), Type::SubclassOf(target_subclass_ty)) => {
                match target_subclass_ty.subclass_of() {
                    SubclassOfInner::Protocol(target_protocol) => self
                        .check_meta_type_satisfies_protocol(
                            db,
                            Type::GenericAlias(source_alias),
                            target_protocol,
                        ),
                    target => target
                        .into_class(db, env)
                        .map(|target_cls| {
                            self.check_class_pair(db, ClassType::Generic(source_alias), target_cls)
                        })
                        .unwrap_or_else(|| {
                            ConstraintSet::from_bool(
                                self.constraints,
                                self.relation.is_assignability(),
                            )
                        }),
                }
            }

            // This branch asks: given two types `type[T]` and `type[S]`, is `type[T]` a subtype of `type[S]`?
            (Type::SubclassOf(source), Type::SubclassOf(target)) => {
                self.check_subclassof_pair(db, source, target)
            }

            // `Literal[str]` is a subtype of `type` because the `str` class object is an instance of its metaclass `type`.
            // `Literal[abc.ABC]` is a subtype of `abc.ABCMeta` because the `abc.ABC` class object
            // is an instance of its metaclass `abc.ABCMeta`.
            (Type::ClassLiteral(source_class), _) => {
                self.check_child_pair(db, source_class.metaclass_instance_type(db, env), target)
            }
            (Type::GenericAlias(source_alias), _) => self.check_child_pair(
                db,
                ClassType::Generic(source_alias).metaclass_instance_type(db, env),
                target,
            ),

            // `type[Any]` is a subtype of `type[object]`, and is assignable to any `type[...]`
            (Type::SubclassOf(subclass_of_ty), _) if subclass_of_ty.is_dynamic() => self
                .check_child_pair(db, KnownClass::Type.to_instance(db, env), target)
                .or(db, self.constraints, || {
                    ConstraintSet::from_bool(self.constraints, self.relation.is_assignability())
                        .and(db, self.constraints, || {
                            self.check_child_pair(db, target, KnownClass::Type.to_instance(db, env))
                        })
                }),

            // Any `type[...]` type is assignable to `type[Any]`
            (_, Type::SubclassOf(subclass_of_ty))
                if subclass_of_ty.is_dynamic() && self.relation.is_assignability() =>
            {
                self.check_child_pair(db, source, KnownClass::Type.to_instance(db, env))
            }

            // `type[str]` (== `SubclassOf("str")` in ty) describes all possible runtime subclasses
            // of the class object `str`. It is a subtype of `type` (== `Instance("type")`) because `str`
            // is an instance of `type`, and so all possible subclasses of `str` will also be instances of `type`.
            //
            // Similarly `type[enum.Enum]`  is a subtype of `enum.EnumMeta` because `enum.Enum`
            // is an instance of `enum.EnumMeta`. `type[Any]` and `type[Unknown]` do not participate in subtyping,
            // however, as they are not fully static types.
            (Type::SubclassOf(subclass_of_ty), _) => {
                self.check_child_pair(db, subclass_of_ty.to_metaclass_instance(db, env), target)
            }

            (Type::TypeForm(_), _) => self.check_child_pair(db, Type::object(), target),

            // For example: `Type::SpecialForm(SpecialFormType::Type)` is a subtype of `Type::NominalInstance(_SpecialForm)`,
            // because `Type::SpecialForm(SpecialFormType::Type)` is a set with exactly one runtime value in it
            // (the symbol `typing.Type`), and that symbol is known to be an instance of `typing._SpecialForm` at runtime.
            (Type::SpecialForm(source_form), _) => {
                self.check_child_pair(db, source_form.instance_fallback(db, env), target)
            }

            (Type::KnownInstance(source), _) => {
                self.check_child_pair(db, source.instance_fallback(db, env), target)
            }

            // `bool` is a subtype of `int`, because `bool` subclasses `int`,
            // which means that all instances of `bool` are also instances of `int`
            (Type::NominalInstance(source_i), Type::NominalInstance(target_i)) => {
                // As an optimization, skip the recursion guard when the target is
                // non-generic and has no tuple specification. These comparisons only
                // inspect MRO class identities; there are no type arguments or tuple
                // elements whose comparison could recurse.
                if target_i.own_tuple_spec(db).is_none()
                    && matches!(target_i.class(db, env), ClassType::NonGeneric(_))
                {
                    self.check_nominal_instance_pair(db, source_i, target_i)
                } else {
                    self.with_recursion_guard(db, source, target, || {
                        self.check_nominal_instance_pair(db, source_i, target_i)
                    })
                }
            }

            (Type::PropertyInstance(source_p), Type::PropertyInstance(target_p)) => self
                .with_recursion_guard(db, source, target, || {
                    self.check_property_instance_pair(db, source_p, target_p)
                }),

            (Type::PropertyInstance(property), _) => {
                self.check_child_pair(db, property.instance_fallback(db, env), target)
            }
            (_, Type::PropertyInstance(property)) => {
                self.check_child_pair(db, source, property.instance_fallback(db, env))
            }
            (Type::SlotDescriptor(_), _) => self.check_child_pair(
                db,
                KnownClass::MemberDescriptorType.to_instance(db, env),
                target,
            ),
            (_, Type::SlotDescriptor(_)) => self.check_child_pair(
                db,
                source,
                KnownClass::MemberDescriptorType.to_instance(db, env),
            ),
            // Other than the special cases enumerated above, nominal-instance types are never
            // subtypes of any other variants
            (Type::NominalInstance(_), _) => self.never(),
        }
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
            (Some(source), Some(target)) => self.check_child_pair(db, source, target),
            (None | Some(_), None | Some(_)) => self.never(),
        };

        self.check_child_pair(
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

    pub(super) fn as_equivalence_checker(&self) -> EquivalenceChecker<'_, 'c, 'db> {
        EquivalenceChecker {
            env: self.env,
            constraints: self.constraints,
            provenance: self.provenance,
            perform_expensive_checks: self.perform_expensive_checks,
            typevar_evaluation: TypeVarEvaluation::Eager,
            materialization_visitor: self.materialization_visitor,
            observations: self.observations.clone(),
        }
    }

    pub(super) fn as_disjointness_checker(&self) -> DisjointnessChecker<'_, 'c, 'db> {
        DisjointnessChecker {
            env: self.env,
            constraints: self.constraints,
            inferable: self.inferable,
            context_tree: None,
            provenance: self.provenance,
            perform_expensive_checks: self.perform_expensive_checks,
            materialization_visitor: self.materialization_visitor,
            observations: self.observations.clone(),
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

#[derive(Clone)]
pub(super) struct EquivalenceChecker<'a, 'c, 'db> {
    env: &'a ProgramEnvironment<'db>,
    pub(super) constraints: &'c ConstraintSetBuilder<'db>,
    provenance: ConstraintProvenance,
    perform_expensive_checks: bool,
    typevar_evaluation: TypeVarEvaluation,

    materialization_visitor: &'a ApplyTypeMappingVisitor<'a, 'db>,
    observations: ObservedTypePair<'db>,
}

impl<'c, 'db> EquivalenceChecker<'_, 'c, 'db> {
    fn as_relation_checker<'a>(
        &'a self,
        materialization_visitor: &'a ApplyTypeMappingVisitor<'a, 'db>,
    ) -> TypeRelationChecker<'a, 'c, 'db> {
        TypeRelationChecker {
            env: self.env,
            relation: TypeRelation::Redundancy { pure: true },
            typevar_evaluation: self.typevar_evaluation,
            constraints: self.constraints,
            context_tree: None,
            provenance: self.provenance,
            perform_expensive_checks: self.perform_expensive_checks,
            inferable: TypeVarSet::None,
            materialization_visitor,
            observations: self.observations.clone(),
        }
    }

    pub(super) fn always(&self) -> ConstraintSet<'db, 'c> {
        ConstraintSet::from_bool(self.constraints, true)
    }

    pub(super) fn never(&self) -> ConstraintSet<'db, 'c> {
        ConstraintSet::from_bool(self.constraints, false)
    }

    /// Compare values derived from the current operands without creating independent proof roots.
    pub(super) fn check_type_pair(
        &self,
        db: &'db dyn Db,
        source: Type<'db>,
        target: Type<'db>,
    ) -> ConstraintSet<'db, 'c> {
        self.check_child_pair(db, source, target)
    }

    /// Descend through the expressions owned by the current proof operands.
    pub(super) fn check_child_pair(
        &self,
        db: &'db dyn Db,
        source: Type<'db>,
        target: Type<'db>,
    ) -> ConstraintSet<'db, 'c> {
        let (source, target) = self.observations.children(source, target);
        self.check_observed_pair(db, source, target)
    }

    /// Compare children selected by structural edges of the current operands.
    pub(super) fn check_child_pair_at(
        &self,
        db: &'db dyn Db,
        source: Type<'db>,
        target: Type<'db>,
        source_edge: ObservationEdge,
        target_edge: ObservationEdge,
    ) -> ConstraintSet<'db, 'c> {
        let (source, target) =
            self.observations
                .children_at(db, self.env, source, target, source_edge, target_edge);
        self.check_observed_pair(db, source, target)
    }

    pub(super) fn check_observed_pair(
        &self,
        db: &'db dyn Db,
        source: ObservedType<'db>,
        target: ObservedType<'db>,
    ) -> ConstraintSet<'db, 'c> {
        let source_ty = source.ty;
        let target_ty = target.ty;
        let checker = Self {
            observations: ObservedTypePair::new(source, target),
            ..self.clone()
        };
        checker.check_type_pair_observed_impl(db, source_ty, target_ty)
    }

    fn check_type_pair_observed_impl(
        &self,
        db: &'db dyn Db,
        left: Type<'db>,
        right: Type<'db>,
    ) -> ConstraintSet<'db, 'c> {
        // Recursive materialization fallbacks depend on the comparison root, so each directional
        // pass needs fresh materialization caches. Nested equivalence checks still share the
        // materialization-equivalence recursion guard to avoid re-entering the same comparison.
        let left_to_right_materialization_visitor = self.materialization_visitor.for_new_mapping();
        self.as_relation_checker(&left_to_right_materialization_visitor)
            .check_child_pair(db, left, right)
            .and(db, self.constraints, || {
                let right_to_left_materialization_visitor =
                    self.materialization_visitor.for_new_mapping();
                self.as_relation_checker(&right_to_left_materialization_visitor)
                    .reversed()
                    .check_child_pair(db, right, left)
            })
    }
}

#[derive(Clone)]
pub(super) struct DisjointnessChecker<'a, 'c, 'db> {
    pub(super) env: &'a ProgramEnvironment<'db>,
    pub(super) constraints: &'c ConstraintSetBuilder<'db>,
    pub(super) inferable: TypeVarSet<'db>,
    context_tree: Option<ErrorContextTree<'db>>,
    provenance: ConstraintProvenance,
    perform_expensive_checks: bool,

    materialization_visitor: &'a ApplyTypeMappingVisitor<'a, 'db>,
    observations: ObservedTypePair<'db>,
}

impl<'a, 'c, 'db> DisjointnessChecker<'a, 'c, 'db> {
    pub(super) fn new(
        env: &'a ProgramEnvironment<'db>,
        constraints: &'c ConstraintSetBuilder<'db>,
        inferable: TypeVarSet<'db>,
        materialization_visitor: &'a ApplyTypeMappingVisitor<'a, 'db>,
        observations: ObservedTypePair<'db>,
    ) -> Self {
        Self {
            env,
            constraints,
            inferable,
            context_tree: None,
            provenance: constraints.relation_context().provenance(),
            perform_expensive_checks: constraints.relation_context().perform_expensive_checks(),
            materialization_visitor,
            observations,
        }
    }

    pub(super) fn as_relation_checker(
        &self,
        relation: TypeRelation,
    ) -> TypeRelationChecker<'_, 'c, 'db> {
        TypeRelationChecker {
            env: self.env,
            relation,
            typevar_evaluation: TypeVarEvaluation::Eager,
            constraints: self.constraints,
            inferable: self.inferable,
            context_tree: None,
            provenance: self.provenance,
            perform_expensive_checks: self.perform_expensive_checks,
            materialization_visitor: self.materialization_visitor,
            observations: self.observations.clone(),
        }
    }

    pub(super) fn operands(&self) -> &ObservedTypePair<'db> {
        &self.observations
    }

    pub(super) fn with_operands(&self, observations: ObservedTypePair<'db>) -> Self {
        Self {
            observations,
            ..self.clone()
        }
    }

    pub(super) fn report_context(&self) -> Option<&ErrorContextTree<'db>> {
        self.context_tree
            .as_ref()
            .filter(|context| context.is_enabled())
    }

    /// Negate a relation while retaining the polarity of recursive dependencies in its proof.
    pub(super) fn when_relation_does_not_hold(
        &self,
        db: &'db dyn Db,
        check: impl FnOnce() -> ConstraintSet<'db, 'c>,
    ) -> ConstraintSet<'db, 'c> {
        self.constraints
            .relation_session()
            .with_negation(check)
            .negate(db, self.constraints)
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
        let result = self
            .constraints
            .relation_session()
            .with_negation(|| check(&checker));
        if let Some(context) = self.report_context() {
            context.take();
            if result.is_never_satisfied(db, self.env, checker.inferable) {
                context.replace(&checker.into_error_context());
            }
        }
        result
    }

    fn as_equivalence_checker(&self) -> EquivalenceChecker<'_, 'c, 'db> {
        EquivalenceChecker {
            env: self.env,
            constraints: self.constraints,
            provenance: self.provenance,
            perform_expensive_checks: self.perform_expensive_checks,
            typevar_evaluation: TypeVarEvaluation::Eager,
            materialization_visitor: self.materialization_visitor,
            observations: self.observations.clone(),
        }
    }

    fn with_recursion_guard(
        &self,
        db: &'db dyn Db,
        source: Type<'db>,
        target: Type<'db>,
        work: impl FnOnce() -> ConstraintSet<'db, 'c>,
    ) -> ConstraintSet<'db, 'c> {
        let session = self.constraints.relation_session();
        let (source_observed, target_observed) = self.observations.children(source, target);
        let obligation = RelationObligation {
            source,
            target,
            relation: RelationGoal::Disjointness,
            evaluation: TypeVarEvaluation::Eager,
            inferable: self.inferable,
            provenance: self.provenance,
            perform_expensive_checks: self.perform_expensive_checks,
            negative: session.is_negative(),
        };
        session
            .visit_type_pair(
                db,
                self.env,
                self.constraints,
                ObservedRelationObligation {
                    obligation,
                    source_origin: source_observed.origin(),
                    target_origin: target_observed.origin(),
                    source_dependency: source_observed.dependency_origins(),
                    target_dependency: target_observed.dependency_origins(),
                },
                self.report_context().is_some(),
                work,
            )
            .unwrap_or_else(|_| ConstraintSet::incomplete(self.constraints))
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
                    && result.is_always_satisfied(db, env, self.inferable)
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

    /// Fall back to structural disjointness for intersections without an exact finite expansion.
    ///
    /// An intersection is disjoint from another type if any positive component is disjoint from
    /// that type, or if the other type is covered by one of the intersection's negative elements.
    fn check_intersection_pair_via_elements(
        &self,
        db: &'db dyn Db,
        left: Type<'db>,
        right: Type<'db>,
        intersection: IntersectionType<'db>,
        other: Type<'db>,
    ) -> ConstraintSet<'db, 'c> {
        self.with_recursion_guard(db, left, right, || {
            let negative_elements = intersection.negative(db);
            let checker = if matches!(left, Type::Intersection(_)) {
                self.clone()
            } else {
                self.reversed()
            };
            let subtyping_checker = checker.as_relation_checker(TypeRelation::Subtyping);

            (
                // As an optimization, test an exact exclusion before unrelated positive components.
                // Gradual types need the full reflexive subtyping check: `Any` is not a subtype of itself.
                ConstraintSet::from_bool(self.constraints, negative_elements.contains(&other)).and(
                    db,
                    self.constraints,
                    || {
                        subtyping_checker
                            .with_target_operands()
                            .check_child_pair_at(
                                db,
                                other,
                                other,
                                ObservationEdge::Identity,
                                ObservationEdge::Identity,
                            )
                    },
                )
            )
            .or(db, self.constraints, || {
                intersection.positive(db).iter().enumerate().when_any(
                    db,
                    self.constraints,
                    |(index, &pos_ty)| {
                        checker.check_child_pair_at(
                            db,
                            pos_ty,
                            other,
                            ObservationEdge::IntersectionPositive(index),
                            ObservationEdge::Identity,
                        )
                    },
                )
            })
            .or(db, self.constraints, || {
                // A & B & Not[C] is disjoint from C
                negative_elements.iter().enumerate().when_any(
                    db,
                    self.constraints,
                    |(index, &neg_ty)| {
                        subtyping_checker.reversed().check_child_pair_at(
                            db,
                            other,
                            neg_ty,
                            ObservationEdge::Identity,
                            ObservationEdge::IntersectionNegative(index),
                        )
                    },
                )
            })
        })
    }

    /// Compare values derived from the current operands without creating independent proof roots.
    pub(super) fn check_type_pair(
        &self,
        db: &'db dyn Db,
        source: Type<'db>,
        target: Type<'db>,
    ) -> ConstraintSet<'db, 'c> {
        self.check_child_pair(db, source, target)
    }

    /// Descend through the expressions owned by the current proof operands.
    pub(super) fn check_child_pair(
        &self,
        db: &'db dyn Db,
        source: Type<'db>,
        target: Type<'db>,
    ) -> ConstraintSet<'db, 'c> {
        let (source, target) = self.observations.children(source, target);
        self.check_observed_pair(db, source, target)
    }

    /// Compare children selected by structural edges of the current operands.
    pub(super) fn check_child_pair_at(
        &self,
        db: &'db dyn Db,
        source: Type<'db>,
        target: Type<'db>,
        source_edge: ObservationEdge,
        target_edge: ObservationEdge,
    ) -> ConstraintSet<'db, 'c> {
        let (source, target) =
            self.observations
                .children_at(db, self.env, source, target, source_edge, target_edge);
        self.check_observed_pair(db, source, target)
    }

    /// Enter selected structural children while preserving the relation mode and proof session.
    pub(super) fn with_child_operands_at(
        &self,
        db: &'db dyn Db,
        source: Type<'db>,
        target: Type<'db>,
        source_edge: ObservationEdge,
        target_edge: ObservationEdge,
    ) -> Self {
        let (source, target) =
            self.observations
                .children_at(db, self.env, source, target, source_edge, target_edge);
        Self {
            observations: ObservedTypePair::new(source, target),
            ..self.clone()
        }
    }

    pub(super) fn reversed(&self) -> Self {
        Self {
            observations: self.observations.reversed(),
            ..self.clone()
        }
    }

    pub(super) fn check_observed_pair(
        &self,
        db: &'db dyn Db,
        source: ObservedType<'db>,
        target: ObservedType<'db>,
    ) -> ConstraintSet<'db, 'c> {
        let source_ty = source.ty;
        let target_ty = target.ty;
        let checker = Self {
            observations: ObservedTypePair::new(source, target),
            ..self.clone()
        };
        checker.check_type_pair_observed_impl(db, source_ty, target_ty)
    }

    fn check_type_pair_observed_impl(
        &self,
        db: &'db dyn Db,
        left: Type<'db>,
        right: Type<'db>,
    ) -> ConstraintSet<'db, 'c> {
        if let Some(context) = self.report_context() {
            context.take();
        }
        let result = self.check_type_pair_impl(db, left, right);
        if let Some(context) = self.report_context()
            && !result.is_always_satisfied(db, self.env, self.inferable)
        {
            // A failed alternative is not evidence for a later successful disjointness check.
            context.take();
        }
        result
    }

    fn check_type_pair_impl(
        &self,
        db: &'db dyn Db,
        left: Type<'db>,
        right: Type<'db>,
    ) -> ConstraintSet<'db, 'c> {
        /// This lets us clearly mark below which match arms require a non-trivial amount of work
        /// to calculate, without sacrificing match guard exhaustiveness checks. If we are not
        /// performing expensive checks, then we will conservatively report that the two types are
        /// not disjoint.
        fn nontrivial_check<'db, 'c>(
            checker: &DisjointnessChecker<'_, 'c, 'db>,
            check: impl FnOnce() -> ConstraintSet<'db, 'c>,
        ) -> ConstraintSet<'db, 'c> {
            if checker.perform_expensive_checks {
                check()
            } else {
                checker.never()
            }
        }

        if matches!(left, Type::Deferred(_)) {
            return self
                .observations
                .source
                .unfold_in_context(db, self.env, &self.context())
                .map_or_else(
                    || ConstraintSet::incomplete(self.constraints),
                    |source| self.check_observed_pair(db, source, self.observations.target.clone()),
                );
        }
        if matches!(right, Type::Deferred(_)) {
            return self
                .observations
                .target
                .unfold_in_context(db, self.env, &self.context())
                .map_or_else(
                    || ConstraintSet::incomplete(self.constraints),
                    |target| self.check_observed_pair(db, self.observations.source.clone(), target),
                );
        }
        if let Some(left) = left.materialized_divergent_fallback() {
            return self.check_child_pair(db, left, right);
        }

        if let Some(right) = right.materialized_divergent_fallback() {
            return self.check_child_pair(db, left, right);
        }

        let env = self.env;

        match (left, right) {
            (Type::Deferred(_), _) | (_, Type::Deferred(_)) => {
                ConstraintSet::incomplete(self.constraints)
            }
            (Type::RecursiveVar(_), _) | (_, Type::RecursiveVar(_)) => {
                unreachable!("semantic operation on an unbound recursive variable")
            }
            (Type::Never, _) | (_, Type::Never) => self.always(),

            (Type::Dynamic(_), _) | (_, Type::Dynamic(_)) => self.never(),
            (Type::Divergent(_), _) | (_, Type::Divergent(_)) => self.never(),

            (Type::Recursive(_), _) => self.with_recursion_guard(db, left, right, || {
                self.observations
                    .children(left, right)
                    .0
                    .unfold(db, self.env)
                    .map(|left_unfolded| {
                        let target = self.observations.children(left, right).1;
                        self.check_observed_pair(db, left_unfolded, target)
                    })
                    .unwrap_or(self.never())
            }),

            (_, Type::Recursive(_)) => self.with_recursion_guard(db, left, right, || {
                self.observations
                    .children(left, right)
                    .1
                    .unfold(db, self.env)
                    .map(|right_unfolded| {
                        let source = self.observations.children(left, right).0;
                        self.check_observed_pair(db, source, right_unfolded)
                    })
                    .unwrap_or(self.never())
            }),

            (Type::TypeAlias(_), _) => nontrivial_check(self, || {
                self.with_recursion_guard(db, left, right, || {
                    self.observations
                        .children(left, right)
                        .0
                        .unfold(db, self.env)
                        .map(|left_unfolded| {
                            let target = self.observations.children(left, right).1;
                            self.check_observed_pair(db, left_unfolded, target)
                        })
                        .unwrap_or_else(|| ConstraintSet::incomplete(self.constraints))
                })
            }),

            (_, Type::TypeAlias(_)) => nontrivial_check(self, || {
                self.with_recursion_guard(db, left, right, || {
                    self.observations
                        .children(left, right)
                        .1
                        .unfold(db, self.env)
                        .map(|right_unfolded| {
                            let source = self.observations.children(left, right).0;
                            self.check_observed_pair(db, source, right_unfolded)
                        })
                        .unwrap_or_else(|| ConstraintSet::incomplete(self.constraints))
                })
            }),

            (Type::EnumComplement(complement), other) => nontrivial_check(self, || {
                self.check_child_pair(db, complement.remaining_literal_union(db, env), other)
            }),

            (other, Type::EnumComplement(complement)) => nontrivial_check(self, || {
                self.check_child_pair(db, other, complement.remaining_literal_union(db, env))
            }),

            // `type[T]` and `TypeForm[S]` overlap whenever their represented instance types do.
            (Type::SubclassOf(subclass_of), Type::TypeForm(typeform))
            | (Type::TypeForm(typeform), Type::SubclassOf(subclass_of)) => {
                nontrivial_check(self, || {
                    self.check_child_pair(
                        db,
                        subclass_of.to_instance(db, env),
                        typeform.type_argument(db),
                    )
                })
            }

            // `type[T]` is disjoint from a class object `A` if every instance of `T` is disjoint from an instance of `A`.
            (Type::SubclassOf(subclass_of), other) | (other, Type::SubclassOf(subclass_of))
                if let Some(type_var) = subclass_of.into_type_var()
                    && let Some(instance) = other.to_instance_approximation(db, env) =>
            {
                nontrivial_check(self, || {
                    self.check_child_pair(db, Type::TypeVar(type_var), instance)
                })
            }

            // A typevar is never disjoint from itself, since all occurrences of the typevar must
            // be specialized to the same type. (This is an important difference between typevars
            // and `Any`!) Different typevars might be disjoint, depending on their bounds and
            // constraints, which are handled below.
            (Type::TypeVar(left_tvar), Type::TypeVar(right_tvar))
                if !left_tvar.is_inferable(db, self.inferable)
                    && left_tvar.is_same_typevar_as(db, right_tvar) =>
            {
                self.never()
            }

            (Type::TypeVar(tvar), Type::Intersection(intersection))
            | (Type::Intersection(intersection), Type::TypeVar(tvar))
                if !tvar.is_inferable(db, self.inferable)
                    && intersection.negative(db).contains(&Type::TypeVar(tvar)) =>
            {
                self.always()
            }

            // An unbounded typevar is never disjoint from any other type, since it might be
            // specialized to any type. A bounded typevar is not disjoint from its bound, and is
            // only disjoint from other types if its bound is. A constrained typevar is disjoint
            // from a type if all of its constraints are.
            (Type::TypeVar(tvar), other) | (other, Type::TypeVar(tvar))
                if !tvar.is_inferable(db, self.inferable) =>
            {
                nontrivial_check(self, || {
                    match tvar.typevar(db).bound_or_constraints(db, env) {
                        None => self.never(),
                        Some(TypeVarBoundOrConstraints::UpperBound(bound)) => {
                            self.check_child_pair(db, bound, other)
                        }
                        Some(TypeVarBoundOrConstraints::Constraints(typevar_constraints)) => {
                            typevar_constraints.elements(db).iter().when_all(
                                db,
                                self.constraints,
                                |constraint| self.check_child_pair(db, *constraint, other),
                            )
                        }
                    }
                })
            }

            // TODO: Infer specializations here
            (Type::TypeVar(_), _) | (_, Type::TypeVar(_)) => self.never(),

            (Type::Union(union), other) | (other, Type::Union(union)) => {
                nontrivial_check(self, || {
                    let checker = if matches!(left, Type::Union(_)) {
                        self.clone()
                    } else {
                        self.reversed()
                    };
                    let mut children = Vec::new();
                    let result = union.elements(db).iter().enumerate().when_all(
                        db,
                        self.constraints,
                        |(index, e)| {
                            let result = checker.check_child_pair_at(
                                db,
                                *e,
                                other,
                                ObservationEdge::UnionElement(index),
                                ObservationEdge::Identity,
                            );
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
                        },
                    );
                    if let Some(context) = self.report_context()
                        && result.is_always_satisfied(db, env, self.inferable)
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
                })
            }

            // If we have two intersections, we test the positive elements of each one against the other intersection
            // Negative elements need a positive element on the other side in order to be disjoint.
            // This is similar to what would happen if we tried to build a new intersection that combines the two
            (Type::Intersection(left_intersection), Type::Intersection(right_intersection)) => {
                nontrivial_check(self, || {
                    if let Some(alternatives) = left_intersection.finite_alternative_union(db, env)
                    {
                        self.check_child_pair(db, alternatives, right)
                    } else if let Some(alternatives) =
                        right_intersection.finite_alternative_union(db, env)
                    {
                        self.check_child_pair(db, left, alternatives)
                    } else {
                        self.with_recursion_guard(db, left, right, || {
                            left_intersection
                                .positive(db)
                                .iter()
                                .enumerate()
                                .when_any(db, self.constraints, |(index, &pos_ty)| {
                                    self.check_child_pair_at(
                                        db,
                                        pos_ty,
                                        right,
                                        ObservationEdge::IntersectionPositive(index),
                                        ObservationEdge::Identity,
                                    )
                                })
                                .or(db, self.constraints, || {
                                    right_intersection.positive(db).iter().enumerate().when_any(
                                        db,
                                        self.constraints,
                                        |(index, &pos_ty)| {
                                            self.reversed().check_child_pair_at(
                                                db,
                                                pos_ty,
                                                left,
                                                ObservationEdge::IntersectionPositive(index),
                                                ObservationEdge::Identity,
                                            )
                                        },
                                    )
                                })
                        })
                    }
                })
            }

            (Type::Intersection(intersection), other) => nontrivial_check(self, || {
                if let Some(alternatives) = intersection.finite_alternative_union(db, env) {
                    self.check_child_pair(db, alternatives, other)
                } else {
                    self.check_intersection_pair_via_elements(db, left, right, intersection, other)
                }
            }),

            (other, Type::Intersection(intersection)) => nontrivial_check(self, || {
                if let Some(alternatives) = intersection.finite_alternative_union(db, env) {
                    self.check_child_pair(db, other, alternatives)
                } else {
                    self.check_intersection_pair_via_elements(db, left, right, intersection, other)
                }
            }),

            // A NewType's concrete base can be a metaclass: `N = NewType("N", Meta)`, where `Meta`
            // subclasses `type`. Unwrapping it here lets the earlier `to_instance_approximation`
            // arm reduce `type[T]` versus `Meta` to `T` versus `object`.
            // If we transposed first (the next arm), a protocol bound on T could instead make us
            // compare the protocol's own metaclass with Meta, incorrectly concluding that the
            // types are disjoint. Other NewType comparisons need their specialized checks before
            // unwrapping; in particular, protocol member lookup must preserve the NewType receiver
            // for `Self`.
            (class @ Type::SubclassOf(_), Type::NewTypeInstance(newtype))
            | (Type::NewTypeInstance(newtype), class @ Type::SubclassOf(_)) => {
                nontrivial_check(self, || {
                    self.check_child_pair(db, class, newtype.concrete_base_type(db))
                })
            }

            // `type[T]` is disjoint from another type if its transposed upper bound or constraints are.
            // Transposition preserves T's identity but replaces its bounds. The match arms that
            // perform the following operations must remain above this branch:
            // - Unfold aliases and recursive types, which can expose other cases in this list.
            // - Project class objects and TypeForms to their instance types, including metaclasses
            //   exposed by unwrapping a NewType.
            // - Handle bare typevars: T and its transpose share an identity but have different bounds.
            // - Decompose unions and intersections: their members can refer to T or exclude type[T],
            //   as in `type[T]` versus `Not[type[T]] | int`.
            // The cases after this do not rely on the original typevar identity at the outer
            // level.
            // Keep transposition before protocol checks: a final bound can expose an exact class
            // type whose missing members prove disjointness.
            (Type::SubclassOf(subclass_of), other) | (other, Type::SubclassOf(subclass_of))
                if let Some(type_var) = subclass_of
                    .subclass_of()
                    .with_transposed_type_var(db, env)
                    .into_type_var() =>
            {
                nontrivial_check(self, || {
                    self.check_child_pair(db, Type::TypeVar(type_var), other)
                })
            }

            (Type::LiteralValue(left), Type::LiteralValue(right))
                if left.is_literal_string() && right.is_literal_string()
                    || (left.is_string() && right.is_literal_string())
                    || (left.is_literal_string() && right.is_string()) =>
            {
                self.never()
            }

            (Type::LiteralValue(left), Type::LiteralValue(right)) => {
                if let (Some(left), Some(right)) = (left.as_enum(), right.as_enum())
                    && left.enum_class_literal(db) == right.enum_class_literal(db)
                    && !left.enum_class_literal(db).aliases_are_known(db)
                {
                    self.never()
                } else {
                    ConstraintSet::from_bool(self.constraints, left.kind() != right.kind())
                }
            }

            (Type::PropertyInstance(left), Type::PropertyInstance(right)) => {
                nontrivial_check(self, || self.check_property_instance_pair(db, left, right))
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
            ) => nontrivial_check(self, || self.check_property_instance_pair(db, left, right)),

            (
                Type::KnownBoundMethod(KnownBoundMethodType::FunctionTypeDunderGet(left)),
                Type::KnownBoundMethod(KnownBoundMethodType::FunctionTypeDunderGet(right)),
            )
            | (
                Type::KnownBoundMethod(KnownBoundMethodType::DunderCall(left)),
                Type::KnownBoundMethod(KnownBoundMethodType::DunderCall(right)),
            ) => nontrivial_check(self, || {
                self.check_child_pair(db, left.inner(db), right.inner(db))
            }),

            (
                Type::KnownBoundMethod(KnownBoundMethodType::MethodTypeDunderGet(left)),
                Type::KnownBoundMethod(KnownBoundMethodType::MethodTypeDunderGet(right)),
            ) => nontrivial_check(self, || {
                self.check_child_pair(db, Type::BoundMethod(left), Type::BoundMethod(right))
            }),

            (
                Type::KnownInstance(KnownInstanceType::Sentinel(left_sentinel)),
                Type::KnownInstance(KnownInstanceType::Sentinel(right_sentinel)),
            ) => ConstraintSet::from_bool(
                self.constraints,
                !left_sentinel.is_same_sentinel(db, right_sentinel),
            ),

            // Distinct wrapper types can describe the same descriptor when their wrapped types
            // overlap; they do not necessarily represent distinct objects.
            (
                Type::KnownInstance(KnownInstanceType::MethodWrapper(left_wrapper)),
                Type::KnownInstance(KnownInstanceType::MethodWrapper(right_wrapper)),
            ) if left_wrapper.kind(db) == right_wrapper.kind(db) => nontrivial_check(self, || {
                self.with_recursion_guard(db, left, right, || {
                    self.check_child_pair(db, left_wrapper.wrapped(db), right_wrapper.wrapped(db))
                })
            }),

            (
                Type::KnownInstance(KnownInstanceType::FunctoolsPartial(left_partial)),
                Type::KnownInstance(KnownInstanceType::FunctoolsPartial(right_partial)),
            )
            | (
                Type::KnownInstance(KnownInstanceType::FunctoolsPartialCall(left_partial)),
                Type::KnownInstance(KnownInstanceType::FunctoolsPartialCall(right_partial)),
            ) => nontrivial_check(self, || {
                self.with_recursion_guard(db, left, right, || {
                    self.check_child_pair(
                        db,
                        left_partial.wrapped(db).inner(db),
                        right_partial.wrapped(db).inner(db),
                    )
                })
            }),

            (
                Type::KnownInstance(KnownInstanceType::Annotated(left)),
                Type::KnownInstance(KnownInstanceType::Annotated(right)),
            ) if left.inner(db).is_recursive_divergent()
                || right.inner(db).is_recursive_divergent() =>
            {
                // A recursive approximation can represent the same Annotated object as a
                // concrete unfolding, even though their wrapped types differ.
                self.never()
            }

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
            ) => ConstraintSet::from_bool(self.constraints, left != right),

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
            ) => self.always(),

            (Type::AlwaysTruthy, ty) | (ty, Type::AlwaysTruthy) => {
                // `Truthiness::Ambiguous` may include `AlwaysTrue` as a subset, so it's not guaranteed to be disjoint.
                // Thus, they are only disjoint if `ty.bool() == AlwaysFalse`.
                nontrivial_check(self, || {
                    ConstraintSet::from_bool(self.constraints, ty.bool(db, env).is_always_false())
                })
            }
            (Type::AlwaysFalsy, ty) | (ty, Type::AlwaysFalsy) => {
                // Similarly, they are only disjoint if `ty.bool() == AlwaysTrue`.
                nontrivial_check(self, || {
                    ConstraintSet::from_bool(self.constraints, ty.bool(db, env).is_always_true())
                })
            }

            (Type::ProtocolInstance(left_proto), Type::ProtocolInstance(right_proto)) => {
                nontrivial_check(self, || {
                    self.with_recursion_guard(db, left, right, || {
                        self.check_protocol_instance_pair(db, left_proto, right_proto)
                    })
                })
            }

            (Type::ProtocolInstance(protocol), Type::SpecialForm(special_form))
            | (Type::SpecialForm(special_form), Type::ProtocolInstance(protocol)) => {
                nontrivial_check(self, || {
                    self.with_recursion_guard(db, left, right, || {
                        self.any_protocol_members_absent_or_disjoint(
                            db,
                            protocol,
                            special_form.instance_fallback(db, env),
                        )
                    })
                })
            }

            (Type::ProtocolInstance(protocol), Type::KnownInstance(known_instance))
            | (Type::KnownInstance(known_instance), Type::ProtocolInstance(protocol)) => {
                nontrivial_check(self, || {
                    self.with_recursion_guard(db, left, right, || {
                        self.any_protocol_members_absent_or_disjoint(
                            db,
                            protocol,
                            known_instance.instance_fallback(db, env),
                        )
                    })
                })
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
            ) => nontrivial_check(self, || {
                self.with_recursion_guard(db, left, right, || {
                    self.any_protocol_members_absent_or_disjoint(db, protocol, ty)
                })
            }),

            // This is the same as the branch above --
            // once guard patterns are stabilised, it could be unified with that branch
            // (<https://github.com/rust-lang/rust/issues/129967>)
            (Type::ProtocolInstance(protocol), Type::NominalInstance(nominal))
            | (Type::NominalInstance(nominal), Type::ProtocolInstance(protocol))
                if self.perform_expensive_checks && nominal.class(db, env).is_final(db) =>
            {
                nontrivial_check(self, || {
                    self.with_recursion_guard(db, left, right, || {
                        self.any_protocol_members_absent_or_disjoint(
                            db,
                            protocol,
                            Type::NominalInstance(nominal),
                        )
                    })
                })
            }

            (Type::ProtocolInstance(protocol), other)
            | (other, Type::ProtocolInstance(protocol)) => nontrivial_check(self, || {
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
                                && result.is_always_satisfied(db, env, self.inferable)
                            {
                                context.push(ErrorContext::ProtocolMemberIncompatible {
                                    member_name: member.name().into(),
                                });
                            }
                            result
                        })
                })
            }),

            (Type::GenericAlias(left_alias), Type::GenericAlias(right_alias)) => {
                ConstraintSet::from_bool(
                    self.constraints,
                    left_alias.origin(db) != right_alias.origin(db),
                )
                .or(db, self.constraints, || {
                    nontrivial_check(self, || {
                        self.check_specialization_pair(
                            db,
                            left_alias.specialization(db),
                            right_alias.specialization(db),
                        )
                    })
                })
            }

            (Type::ClassLiteral(class), Type::GenericAlias(alias_b))
            | (Type::GenericAlias(alias_b), Type::ClassLiteral(class)) => {
                nontrivial_check(self, || {
                    class
                        .default_specialization(db)
                        .into_generic_alias()
                        .when_none_or(db, self.constraints, |alias| {
                            self.check_child_pair(
                                db,
                                Type::GenericAlias(alias_b),
                                Type::GenericAlias(alias),
                            )
                        })
                })
            }

            (Type::SubclassOf(subclass_of_ty), Type::ClassLiteral(class_b))
            | (Type::ClassLiteral(class_b), Type::SubclassOf(subclass_of_ty)) => {
                match subclass_of_ty.subclass_of() {
                    SubclassOfInner::Dynamic(_) => self.never(),
                    SubclassOfInner::Protocol(_) => self.never(),
                    SubclassOfInner::Class(class_a) => nontrivial_check(self, || {
                        ConstraintSet::from_bool(
                            self.constraints,
                            !class_a.could_exist_in_mro_of_with_disjointness_checker(
                                db,
                                ClassType::NonGeneric(class_b),
                                self,
                            ),
                        )
                    }),
                    SubclassOfInner::TypeVar(_) => unreachable!(),
                }
            }

            (Type::SubclassOf(subclass_of_ty), Type::GenericAlias(alias_b))
            | (Type::GenericAlias(alias_b), Type::SubclassOf(subclass_of_ty)) => {
                match subclass_of_ty.subclass_of() {
                    SubclassOfInner::Dynamic(_) => self.never(),
                    SubclassOfInner::Protocol(_) => self.never(),
                    SubclassOfInner::Class(class_a) => nontrivial_check(self, || {
                        ConstraintSet::from_bool(
                            self.constraints,
                            !class_a.could_exist_in_mro_of_with_disjointness_checker(
                                db,
                                ClassType::Generic(alias_b),
                                self,
                            ),
                        )
                    }),
                    SubclassOfInner::TypeVar(_) => unreachable!(),
                }
            }

            (Type::SubclassOf(left), Type::SubclassOf(right)) => {
                nontrivial_check(self, || self.check_subclassof_pair(db, left, right))
            }

            // for `type[Any]`/`type[Unknown]`/`type[Todo]`, we know the type cannot be any larger than `type`,
            // so although the type is dynamic we can still determine disjointedness in some situations
            (Type::SubclassOf(subclass_of_ty), other)
            | (other, Type::SubclassOf(subclass_of_ty)) => {
                nontrivial_check(self, || match subclass_of_ty.subclass_of() {
                    SubclassOfInner::Dynamic(_) | SubclassOfInner::Protocol(_) => {
                        self.check_child_pair(db, KnownClass::Type.to_instance(db, env), other)
                    }
                    SubclassOfInner::Class(_) => self.check_child_pair(
                        db,
                        subclass_of_ty.to_metaclass_instance(db, env),
                        other,
                    ),
                    SubclassOfInner::TypeVar(_) => unreachable!(),
                })
            }

            (Type::SpecialForm(special_form), Type::NominalInstance(instance))
            | (Type::NominalInstance(instance), Type::SpecialForm(special_form)) => {
                nontrivial_check(self, || {
                    ConstraintSet::from_bool(
                        self.constraints,
                        !special_form.is_instance_of(db, env, instance.class(db, env)),
                    )
                })
            }

            (Type::KnownInstance(known_instance), Type::NominalInstance(instance))
            | (Type::NominalInstance(instance), Type::KnownInstance(known_instance)) => {
                nontrivial_check(self, || {
                    ConstraintSet::from_bool(
                        self.constraints,
                        !known_instance.is_instance_of(db, env, instance.class(db, env)),
                    )
                })
            }

            (Type::LiteralValue(literal), Type::NominalInstance(instance))
            | (Type::NominalInstance(instance), Type::LiteralValue(literal)) => {
                nontrivial_check(self, || {
                    self.when_relation_does_not_hold(db, || match literal.kind() {
                        LiteralValueTypeKind::Int(_) => KnownClass::Int.when_subclass_of(
                            db,
                            env,
                            instance.class(db, env),
                            self.constraints,
                        ),
                        LiteralValueTypeKind::Bool(_) => KnownClass::Bool.when_subclass_of(
                            db,
                            env,
                            instance.class(db, env),
                            self.constraints,
                        ),
                        LiteralValueTypeKind::LiteralString | LiteralValueTypeKind::String(_) => {
                            KnownClass::Str.when_subclass_of(
                                db,
                                env,
                                instance.class(db, env),
                                self.constraints,
                            )
                        }
                        LiteralValueTypeKind::Bytes(_) => KnownClass::Bytes.when_subclass_of(
                            db,
                            env,
                            instance.class(db, env),
                            self.constraints,
                        ),
                        LiteralValueTypeKind::Enum(enum_literal) => self
                            .as_relation_checker(TypeRelation::Subtyping)
                            .check_child_pair(
                                db,
                                enum_literal.enum_class_instance(db, env),
                                Type::NominalInstance(instance),
                            ),
                    })
                })
            }

            // Guard wrappers describe boolean results. Different narrowed types or guard kinds
            // do not prove that those results are disjoint.
            (Type::TypeIs(_) | Type::TypeGuard(_), Type::TypeIs(_) | Type::TypeGuard(_)) => {
                self.never()
            }

            (Type::TypeIs(_) | Type::TypeGuard(_), Type::LiteralValue(literal))
            | (Type::LiteralValue(literal), Type::TypeIs(_) | Type::TypeGuard(_)) => {
                ConstraintSet::from_bool(self.constraints, !literal.is_bool())
            }

            (Type::TypeIs(_) | Type::TypeGuard(_), Type::NominalInstance(instance))
            | (Type::NominalInstance(instance), Type::TypeIs(_) | Type::TypeGuard(_)) => {
                // A boolean literal must be an instance of exactly `bool`
                // (it cannot be an instance of a `bool` subclass)
                nontrivial_check(self, || {
                    self.when_relation_does_not_hold(db, || {
                        KnownClass::Bool.when_subclass_of(
                            db,
                            env,
                            instance.class(db, env),
                            self.constraints,
                        )
                    })
                })
            }

            (
                Type::NewTypeInstance(newtype),
                other @ (Type::LiteralValue(_) | Type::TypeIs(_) | Type::TypeGuard(_)),
            )
            | (
                other @ (Type::LiteralValue(_) | Type::TypeIs(_) | Type::TypeGuard(_)),
                Type::NewTypeInstance(newtype),
            ) => nontrivial_check(self, || {
                self.check_child_pair(db, newtype.concrete_base_type(db), other)
            }),

            (Type::TypeIs(_) | Type::TypeGuard(_), _)
            | (_, Type::TypeIs(_) | Type::TypeGuard(_)) => self.always(),

            (Type::LiteralValue(_), _) | (_, Type::LiteralValue(_)) => self.always(),

            // A class-literal type `X` is always disjoint from an instance type `Y`,
            // unless the type expressing "all instances of `Z`" is a subtype of `Y`,
            // where `Z` is `X`'s metaclass.
            (Type::ClassLiteral(class), Type::NominalInstance(instance))
            | (Type::NominalInstance(instance), Type::ClassLiteral(class)) => {
                nontrivial_check(self, || {
                    self.when_relation_does_not_hold(db, || {
                        class
                            .metaclass_instance_type(db, env)
                            .has_relation_to_with_typevar_evaluation(
                                db,
                                env,
                                Type::NominalInstance(instance),
                                self.constraints,
                                self.inferable,
                                TypeRelation::Subtyping,
                                TypeVarEvaluation::Eager,
                                self.provenance,
                            )
                    })
                })
            }

            (Type::GenericAlias(alias), Type::NominalInstance(instance))
            | (Type::NominalInstance(instance), Type::GenericAlias(alias)) => {
                nontrivial_check(self, || {
                    self.when_relation_does_not_hold(db, || {
                        self.as_relation_checker(TypeRelation::Subtyping)
                            .check_child_pair(
                                db,
                                ClassType::Generic(alias).metaclass_instance_type(db, env),
                                Type::NominalInstance(instance),
                            )
                    })
                })
            }

            (Type::FunctionLiteral(function), Type::NominalInstance(instance))
            | (Type::NominalInstance(instance), Type::FunctionLiteral(function)) => {
                // Function literals and their descriptor wrappers have an exact runtime class.
                nontrivial_check(self, || {
                    self.when_relation_does_not_hold(db, || {
                        function.runtime_class(db).when_subclass_of(
                            db,
                            env,
                            instance.class(db, env),
                            self.constraints,
                        )
                    })
                })
            }

            (Type::Callable(callable), other) | (other, Type::Callable(callable))
                if let Some(class) = callable.runtime_class(db) =>
            {
                let other = match other {
                    Type::Callable(other_callable) => {
                        let Some(other_class) = other_callable.runtime_class(db) else {
                            return self.never();
                        };
                        other_class.to_instance(db, env)
                    }
                    _ => other,
                };
                nontrivial_check(self, || {
                    self.check_child_pair(db, class.to_instance(db, env), other)
                })
            }

            // A `BoundMethod` type includes instances of the same method bound to a
            // subtype/subclass of the self type.
            (Type::BoundMethod(a), Type::BoundMethod(b)) => {
                let (Some(a_function), Some(b_function)) = (a.function(db), b.function(db)) else {
                    return nontrivial_check(self, || {
                        self.check_child_pair(db, a.func(db), b.func(db)).or(
                            db,
                            self.constraints,
                            || self.check_child_pair(db, a.self_instance(db), b.self_instance(db)),
                        )
                    });
                };
                if a_function.name(db) != b_function.name(db) {
                    // We typically ask about `BoundMethod` disjointness when we're looking at a
                    // method call on an intersection type like `A & B`. In that case, the same
                    // method name would show up on both sides of this check. However for
                    // completeness, if we're ever comparing `BoundMethod` types with different
                    // method names, then they're clearly disjoint.
                    return self.always();
                }

                nontrivial_check(self, || {
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
                        self.check_child_pair(db, a.self_instance(db), b.self_instance(db))
                    }
                })
            }

            (Type::BoundMethod(_), other) | (other, Type::BoundMethod(_)) => {
                nontrivial_check(self, || {
                    self.check_child_pair(db, KnownClass::MethodType.to_instance(db, env), other)
                })
            }

            (Type::KnownBoundMethod(method), other) | (other, Type::KnownBoundMethod(method)) => {
                nontrivial_check(self, || {
                    self.check_child_pair(db, method.class().to_instance(db, env), other)
                })
            }

            (Type::WrapperDescriptor(_), other) | (other, Type::WrapperDescriptor(_)) => {
                nontrivial_check(self, || {
                    self.check_child_pair(
                        db,
                        KnownClass::WrapperDescriptorType.to_instance(db, env),
                        other,
                    )
                })
            }

            (Type::Callable(_) | Type::FunctionLiteral(_), Type::Callable(_))
            | (Type::Callable(_), Type::FunctionLiteral(_)) => {
                // No two callable types are ever disjoint because
                // `(*args: object, **kwargs: object) -> Never` is a subtype of all fully static
                // callable types.
                self.never()
            }

            (Type::Callable(_), Type::SpecialForm(special_form))
            | (Type::SpecialForm(special_form), Type::Callable(_)) => {
                // A callable type is disjoint from special form types, except for special forms
                // that are callable (like TypedDict and collection constructors).
                // Most special forms are type constructors/annotations (like `typing.Literal`,
                // `typing.Union`, etc.) that are subscripted, not called.
                ConstraintSet::from_bool(self.constraints, !special_form.is_callable())
            }

            (
                Type::Callable(_) | Type::DataclassDecorator(_) | Type::DataclassTransformer(_),
                Type::NominalInstance(nominal),
            )
            | (
                Type::NominalInstance(nominal),
                Type::Callable(_) | Type::DataclassDecorator(_) | Type::DataclassTransformer(_),
            ) if self.perform_expensive_checks && nominal.class(db, env).is_final(db) => {
                nontrivial_check(self, || {
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
                            self.when_relation_does_not_hold(db, || {
                                self.as_relation_checker(TypeRelation::Assignability)
                                    .check_child_pair(
                                        db,
                                        dunder_call,
                                        Type::Callable(CallableType::unknown(db)),
                                    )
                            })
                        })
                })
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
                self.never()
            }

            (Type::ModuleLiteral(..), Type::NominalInstance(instance))
            | (Type::NominalInstance(instance), Type::ModuleLiteral(..)) => {
                // Modules *can* actually be instances of `ModuleType` subclasses
                nontrivial_check(self, || {
                    self.check_child_pair(
                        db,
                        Type::NominalInstance(instance),
                        KnownClass::ModuleType.to_instance(db, env),
                    )
                })
            }

            (Type::NominalInstance(left_i), Type::NominalInstance(right_i)) => {
                nontrivial_check(self, || {
                    self.with_recursion_guard(db, left, right, || {
                        self.check_nominal_instance_pair(db, left_i, right_i)
                    })
                })
            }

            (Type::NewTypeInstance(left), Type::NewTypeInstance(right)) => {
                nontrivial_check(self, || self.check_newtype_pair(db, left, right))
            }
            (Type::NewTypeInstance(newtype), other) | (other, Type::NewTypeInstance(newtype)) => {
                nontrivial_check(self, || {
                    self.check_child_pair(db, newtype.concrete_base_type(db), other)
                })
            }

            (Type::PropertyInstance(property), other)
            | (other, Type::PropertyInstance(property)) => nontrivial_check(self, || {
                self.check_child_pair(db, property.instance_fallback(db, env), other)
            }),

            (Type::SlotDescriptor(_), other) | (other, Type::SlotDescriptor(_)) => {
                nontrivial_check(self, || {
                    self.check_child_pair(
                        db,
                        KnownClass::MemberDescriptorType.to_instance(db, env),
                        other,
                    )
                })
            }

            (Type::BoundSuper(left), Type::BoundSuper(right)) => nontrivial_check(self, || {
                self.when_relation_does_not_hold(db, || {
                    self.as_equivalence_checker()
                        .check_bound_super_pair(db, left, right)
                })
            }),

            (Type::BoundSuper(_), other) | (other, Type::BoundSuper(_)) => {
                nontrivial_check(self, || {
                    self.check_child_pair(db, KnownClass::Super.to_instance(db, env), other)
                })
            }

            (Type::TypeForm(_), _) | (_, Type::TypeForm(_)) => self.never(),

            (Type::GenericAlias(_), _) | (_, Type::GenericAlias(_)) => self.always(),

            (Type::TypedDict(left_td), Type::TypedDict(right_td)) => nontrivial_check(self, || {
                self.with_recursion_guard(db, left, right, || {
                    self.check_typeddict_pair(db, left_td, right_td)
                })
            }),

            // For any type `T`, if `dict[str, Any]` is not assignable to `T`, then all `TypedDict`
            // types will always be disjoint from `T`. This doesn't cover all cases -- in fact
            // `dict` *itself* is almost always disjoint from `TypedDict` -- but it's a good
            // approximation, and some false negatives are acceptable.
            (Type::TypedDict(_), other) | (other, Type::TypedDict(_)) => {
                nontrivial_check(self, || {
                    let dict_str_any = KnownClass::Dict.to_specialized_instance(
                        db,
                        env,
                        &[KnownClass::Str.to_instance(db, env), Type::any()],
                    );

                    self.when_relation_does_not_hold(db, || {
                        self.as_relation_checker(TypeRelation::Assignability)
                            .check_child_pair(db, dict_str_any, other)
                    })
                })
            }
        }
    }

    fn check_property_instance_pair(
        &self,
        db: &'db dyn Db,
        left: PropertyInstanceType<'db>,
        right: PropertyInstanceType<'db>,
    ) -> ConstraintSet<'db, 'c> {
        let check_optional_methods = |left, right| match (left, right) {
            (None, None) => self.never(),
            (Some(left), Some(right)) => self.check_child_pair(db, left, right),
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

#[cfg(test)]
mod tests {
    use std::cell::Cell;
    use std::rc::Rc;

    use ruff_db::files::system_path_to_file;
    use ruff_db::system::DbWithWritableSystem;
    use ty_python_core::ProgramFile;

    use super::{
        ObservedRelationObligation, RelationGoal, RelationObligation, RelationReentry,
        RelationSession, RelationTypeOrigin, TypeRelation, TypeVarEvaluation, same_expression,
    };
    use crate::db::tests::setup_db;
    use crate::place::global_symbol;
    use crate::types::constraints::{ConstraintProvenance, ConstraintSet, ConstraintSetBuilder};
    use crate::types::cyclic::TypeIdentity;
    use crate::types::typevar::TypeVarSet;
    use crate::types::{KnownClass, Type};

    fn obligation<'db>(source: Type<'db>, target: Type<'db>) -> RelationObligation<'db> {
        RelationObligation {
            source,
            target,
            relation: RelationGoal::Relation(TypeRelation::Subtyping),
            evaluation: TypeVarEvaluation::Lazy,
            inferable: TypeVarSet::None,
            provenance: ConstraintProvenance::Evidence,
            perform_expensive_checks: true,
            negative: false,
        }
    }

    #[test]
    fn observed_parameter_descent_does_not_recur_by_declaration() {
        let mut db = setup_db();
        db.write_dedented(
            "/src/a.py",
            r#"
type Node[T] = tuple[T, Node[list[T]] | None]
outer: Node[Node[int]]
inner: Node[int]
"#,
        )
        .unwrap();
        let env = db.program_environment();
        let file = system_path_to_file(&db, "/src/a.py").unwrap();
        let file = ProgramFile::new(&db, file, env.program(&db));
        let outer = global_symbol(&db, file, "outer").place.expect_type();
        let inner = global_symbol(&db, file, "inner").place.expect_type();
        assert_ne!(outer, inner);
        assert!(same_expression(&db, outer, None, inner, None));

        let outer_origin = Rc::new(RelationTypeOrigin {
            constructor: outer.to_type_identity(&db),
            application: outer,
            node: Type::object().into(),
            operations: Box::default(),
        });
        let inner_origin = Rc::new(RelationTypeOrigin {
            constructor: inner.to_type_identity(&db),
            application: inner,
            node: Type::Never.into(),
            operations: Box::default(),
        });
        for (outer_origin, inner_origin) in [
            (Some(outer_origin.clone()), Some(inner_origin.clone())),
            (Some(outer_origin), None),
            (None, Some(inner_origin)),
        ] {
            assert!(!same_expression(
                &db,
                outer,
                outer_origin,
                inner,
                inner_origin,
            ));
        }
    }

    #[test]
    fn an_exact_obligation_does_not_depend_on_its_projection_path() {
        let db = setup_db();
        let session = RelationSession::default();
        let key = obligation(Type::object(), Type::unknown());
        let first = ObservedRelationObligation {
            obligation: key,
            source_origin: Some(Rc::new(RelationTypeOrigin {
                constructor: TypeIdentity::Other(Type::Never),
                application: Type::object(),
                node: Type::object().into(),
                operations: Box::default(),
            })),
            target_origin: None,
            source_dependency: Vec::new(),
            target_dependency: Vec::new(),
        };
        let second = ObservedRelationObligation {
            obligation: key,
            source_origin: Some(Rc::new(RelationTypeOrigin {
                constructor: TypeIdentity::Other(Type::Never),
                application: Type::any(),
                node: Type::Never.into(),
                operations: Box::default(),
            })),
            target_origin: None,
            source_dependency: Vec::new(),
            target_dependency: Vec::new(),
        };
        let result = session.visit(&db, first, || {
            assert!(matches!(
                session.visit(&db, second, || ()),
                Err(RelationReentry::Exact)
            ));
        });
        assert!(result.is_ok());
    }

    #[test]
    fn different_observed_types_do_not_close_an_exact_proof() {
        let db = setup_db();
        let session = RelationSession::default();
        let observed = |source| ObservedRelationObligation {
            obligation: obligation(source, Type::unknown()),
            source_origin: Some(Rc::new(RelationTypeOrigin {
                constructor: TypeIdentity::Other(Type::Never),
                application: source,
                node: Type::Never.into(),
                operations: Box::default(),
            })),
            target_origin: None,
            source_dependency: Vec::new(),
            target_dependency: Vec::new(),
        };
        let result = session.visit(&db, observed(Type::object()), || {
            assert!(matches!(
                session.visit(&db, observed(Type::any()), || ()),
                Err(RelationReentry::Expanding)
            ));
        });
        assert!(result.is_ok());
    }

    #[test]
    fn unresolved_observation_dependencies_allow_finite_work() {
        let db = setup_db();
        let session = RelationSession::default();
        let observed = |source| ObservedRelationObligation {
            obligation: obligation(source, Type::unknown()),
            source_origin: None,
            target_origin: None,
            source_dependency: vec![Rc::new(RelationTypeOrigin {
                constructor: TypeIdentity::Other(Type::Never),
                application: Type::object(),
                node: Type::Never.into(),
                operations: Box::default(),
            })],
            target_dependency: Vec::new(),
        };
        let evaluated = Cell::new(false);
        let result = session.visit(&db, observed(Type::object()), || {
            assert!(
                session
                    .visit(&db, observed(Type::any()), || evaluated.set(true))
                    .is_ok()
            );
        });
        assert!(result.is_ok());
        assert!(evaluated.get());
    }

    #[test]
    fn growing_unresolved_dependencies_do_not_close_a_proof() {
        let db = setup_db();
        let session = RelationSession::default();
        let observed = |source| ObservedRelationObligation {
            obligation: obligation(source, Type::unknown()),
            source_origin: None,
            target_origin: None,
            source_dependency: vec![Rc::new(RelationTypeOrigin {
                constructor: TypeIdentity::Other(Type::Never),
                application: source,
                node: Type::Never.into(),
                operations: Box::default(),
            })],
            target_dependency: Vec::new(),
        };
        let result = session.visit(&db, observed(Type::object()), || {
            assert!(matches!(
                session.visit(&db, observed(Type::any()), || ()),
                Err(RelationReentry::Expanding)
            ));
        });
        assert!(result.is_ok());
    }

    #[test]
    fn completed_relations_do_not_reuse_an_ancestors_assumption() {
        let db = setup_db();
        let db = &db;
        let env = db.program_environment();
        let session = Rc::new(RelationSession::default());
        let builder = ConstraintSetBuilder::with_relation_session(Rc::clone(&session));
        let a = obligation(
            KnownClass::Int.to_instance(db, &env),
            KnownClass::Str.to_instance(db, &env),
        );
        let b = obligation(
            KnownClass::Bool.to_instance(db, &env),
            KnownClass::Bytes.to_instance(db, &env),
        );
        let b_evaluations = Cell::new(0);

        // A requires B and a false condition. B initially succeeds only because A is active.
        let result = session
            .visit_type_pair(db, &env, &builder, a, false, || {
                let b_result = session
                    .visit_type_pair(db, &env, &builder, b, false, || {
                        b_evaluations.set(b_evaluations.get() + 1);
                        let recursive_a =
                            session.visit_type_pair(db, &env, &builder, a, false, || {
                                panic!("an active obligation must not be evaluated again")
                            });
                        assert!(matches!(recursive_a, Err(RelationReentry::Exact)));
                        ConstraintSet::from_bool(&builder, true)
                    })
                    .ok()
                    .expect("B is not active");
                b_result.and(db, &builder, || ConstraintSet::from_bool(&builder, false))
            })
            .ok()
            .expect("A is not active");
        assert!(result.is_never_satisfied(db, &env, TypeVarSet::None));

        // Once A fails, its provisional assumption cannot prove a subsequent call to B.
        let result = session
            .visit_type_pair(db, &env, &builder, b, false, || {
                b_evaluations.set(b_evaluations.get() + 1);
                ConstraintSet::from_bool(&builder, false)
            })
            .ok()
            .expect("B is not active");
        assert_eq!(b_evaluations.get(), 2);
        assert!(result.is_never_satisfied(db, &env, TypeVarSet::None));
    }

    #[test]
    fn completed_relations_do_not_reuse_incomplete_dependencies() {
        let db = setup_db();
        let db = &db;
        let env = db.program_environment();
        let key = obligation(
            KnownClass::Int.to_instance(db, &env),
            KnownClass::Str.to_instance(db, &env),
        );

        for complete_result in [false, true] {
            let session = Rc::new(RelationSession::default());
            let builder = ConstraintSetBuilder::with_relation_session(Rc::clone(&session));
            let evaluations = Cell::new(0);
            let result = session
                .visit_type_pair(db, &env, &builder, key, false, || {
                    evaluations.set(evaluations.get() + 1);
                    let pending = ConstraintSet::incomplete(&builder);
                    if complete_result {
                        // Even a complete outer result can have observed an unresolved dependency.
                        pending.or(db, &builder, || ConstraintSet::from_bool(&builder, true))
                    } else {
                        pending
                    }
                })
                .ok()
                .expect("the obligation is not active");
            assert_eq!(result.is_complete(), complete_result);

            let result = session
                .visit_type_pair(db, &env, &builder, key, false, || {
                    evaluations.set(evaluations.get() + 1);
                    ConstraintSet::from_bool(&builder, false)
                })
                .ok()
                .expect("the obligation is not active");
            assert_eq!(evaluations.get(), 2);
            assert!(result.is_never_satisfied(db, &env, TypeVarSet::None));
        }
    }
}
