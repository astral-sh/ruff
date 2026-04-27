use itertools::Itertools;
use ruff_python_ast::name::Name;
use rustc_hash::FxHashSet;

use crate::place::{DefinedPlace, Place};
use crate::types::constraints::{
    ConstraintSetBuilder, IteratorConstraintsExtension, OptionConstraintsExtension,
};
use crate::types::cyclic::PairVisitor;
use crate::types::enums::is_single_member_enum;
use crate::types::function::FunctionDecorators;
use crate::types::set_theoretic::RecursivelyDefined;
use crate::types::{
    ApplyTypeMappingVisitor, CallableType, ClassBase, ClassLiteral, ClassType, CycleDetector,
    IntersectionType, KnownBoundMethodType, KnownClass, KnownInstanceType, LiteralValueTypeKind,
    MemberLookupPolicy, PropertyInstanceType, ProtocolInstanceType, SubclassOfInner,
    SubclassOfType, TypeVarBoundOrConstraints, UnionType, UpcastPolicy,
};
use crate::{
    Db,
    types::{
        ErrorContext, ErrorContextTree, Type, constraints::ConstraintSet,
        generics::InferableTypeVars,
    },
};

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

    /// A placeholder for the new assignability relation that uses constraint sets to encode
    /// relationships with a typevar. This will eventually replace `Assignability`, but allows us
    /// to start using the new relation in a controlled manner in some places.
    ConstraintSetAssignability,
}

impl TypeRelation {
    pub(crate) const fn is_assignability(self) -> bool {
        matches!(self, TypeRelation::Assignability)
    }

    pub(crate) const fn is_constraint_set_assignability(self) -> bool {
        matches!(self, TypeRelation::ConstraintSetAssignability)
    }

    pub(crate) const fn is_subtyping(self) -> bool {
        matches!(self, TypeRelation::Subtyping)
    }

    pub(crate) const fn can_safely_assume_reflexivity(self, ty: Type) -> bool {
        match self {
            TypeRelation::Assignability
            | TypeRelation::ConstraintSetAssignability
            | TypeRelation::Redundancy { .. } => true,
            TypeRelation::Subtyping | TypeRelation::SubtypingAssuming => {
                ty.subtyping_is_always_reflexive()
            }
        }
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
            Type::Never
            | Type::FunctionLiteral(..)
            | Type::BoundMethod(_)
            | Type::WrapperDescriptor(_)
            | Type::KnownBoundMethod(
                KnownBoundMethodType::FunctionTypeDunderGet(_)
                | KnownBoundMethodType::FunctionTypeDunderCall(_)
                | KnownBoundMethodType::StrStartswith(_)
                | KnownBoundMethodType::ConstraintSetRange
                | KnownBoundMethodType::ConstraintSetAlways
                | KnownBoundMethodType::ConstraintSetNever
                | KnownBoundMethodType::ConstraintSetImpliesSubtypeOf(_)
                | KnownBoundMethodType::ConstraintSetSatisfies(_)
                | KnownBoundMethodType::ConstraintSetSatisfiedByAllTypeVars(_),
            )
            | Type::DataclassDecorator(_)
            | Type::DataclassTransformer(_)
            | Type::ModuleLiteral(..)
            | Type::LiteralValue(_)
            | Type::SpecialForm(_)
            | Type::KnownInstance(_)
            | Type::AlwaysFalsy
            | Type::AlwaysTruthy
            // `T` is always a subtype of itself,
            // and `T` is always a subtype of `T | None`
            | Type::TypeVar(_)
            // might inherit `Any`, but subtyping is still reflexive
            | Type::ClassLiteral(_)
             => true,
            Type::Dynamic(_)
            | Type::Divergent(_)
            | Type::NominalInstance(_)
            | Type::ProtocolInstance(_)
            | Type::GenericAlias(_)
            | Type::SubclassOf(_)
            | Type::Union(_)
            | Type::Intersection(_)
            | Type::Callable(_)
            | Type::KnownBoundMethod(
                KnownBoundMethodType::PropertyDunderGet(_)
                | KnownBoundMethodType::PropertyDunderSet(_)
                | KnownBoundMethodType::PropertyDunderDelete(_),
            )
            | Type::PropertyInstance(_)
            | Type::BoundSuper(_)
            | Type::TypeIs(_)
            | Type::TypeGuard(_)
            | Type::TypedDict(_)
            | Type::TypeAlias(_)
            | Type::NewTypeInstance(_) => false,
        }
    }

    /// Return true if this type is a subtype of type `target`.
    ///
    /// See [`TypeRelation::Subtyping`] for more details.
    pub(crate) fn is_subtype_of(self, db: &'db dyn Db, target: Type<'db>) -> bool {
        let constraints = ConstraintSetBuilder::new();
        self.when_subtype_of(db, target, &constraints, InferableTypeVars::None)
            .is_always_satisfied(db)
    }

    pub(super) fn when_subtype_of<'c>(
        self,
        db: &'db dyn Db,
        target: Type<'db>,
        constraints: &'c ConstraintSetBuilder<'db>,
        inferable: InferableTypeVars<'db>,
    ) -> ConstraintSet<'db, 'c> {
        self.has_relation_to(db, target, constraints, inferable, TypeRelation::Subtyping)
    }

    /// Return the constraints under which this type is a subtype of type `target`, assuming that
    /// all of the restrictions in `constraints` hold.
    ///
    /// See [`TypeRelation::SubtypingAssuming`] for more details.
    pub(super) fn when_subtype_of_assuming<'c>(
        self,
        db: &'db dyn Db,
        target: Type<'db>,
        assuming: ConstraintSet<'db, 'c>,
        constraints: &'c ConstraintSetBuilder<'db>,
        inferable: InferableTypeVars<'db>,
    ) -> ConstraintSet<'db, 'c> {
        let relation_visitor = HasRelationToVisitor::default(constraints);
        let disjointness_visitor = IsDisjointVisitor::default(constraints);
        let materialization_visitor = ApplyTypeMappingVisitor::default();
        let checker = TypeRelationChecker {
            constraints,
            inferable,
            relation: TypeRelation::SubtypingAssuming,
            context_tree: ErrorContextTree::disabled(),
            given: assuming,
            relation_visitor: &relation_visitor,
            disjointness_visitor: &disjointness_visitor,
            materialization_visitor: &materialization_visitor,
        };
        checker.check_type_pair(db, self, target)
    }

    /// Return true if this type is assignable to type `target`.
    ///
    /// See `TypeRelation::Assignability` for more details.
    pub fn is_assignable_to(self, db: &'db dyn Db, target: Type<'db>) -> bool {
        let constraints = ConstraintSetBuilder::new();
        self.when_assignable_to(db, target, &constraints, InferableTypeVars::None)
            .is_always_satisfied(db)
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
        target: Type<'db>,
    ) -> ErrorContextTree<'db> {
        let builder = ConstraintSetBuilder::new();
        let checker = TypeRelationChecker {
            constraints: &builder,
            inferable: InferableTypeVars::None,
            relation: TypeRelation::Assignability,
            context_tree: ErrorContextTree::enabled(),
            given: ConstraintSet::from_bool(&builder, false),
            relation_visitor: &HasRelationToVisitor::default(&builder),
            disjointness_visitor: &IsDisjointVisitor::default(&builder),
            materialization_visitor: &ApplyTypeMappingVisitor::default(),
        };
        checker.check_type_pair(db, self, target);
        checker.context_tree
    }

    /// Return true if this type is assignable to type `target` using constraint-set assignability.
    ///
    /// This uses `TypeRelation::ConstraintSetAssignability`, which encodes typevar relations into
    /// a constraint set and lets `satisfied_by_all_typevars` perform existential vs universal
    /// reasoning depending on inferable typevars.
    pub fn is_constraint_set_assignable_to(self, db: &'db dyn Db, target: Type<'db>) -> bool {
        let constraints = ConstraintSetBuilder::new();
        self.when_constraint_set_assignable_to(db, target, &constraints)
            .is_always_satisfied(db)
    }

    pub(super) fn when_assignable_to<'c>(
        self,
        db: &'db dyn Db,
        target: Type<'db>,
        constraints: &'c ConstraintSetBuilder<'db>,
        inferable: InferableTypeVars<'db>,
    ) -> ConstraintSet<'db, 'c> {
        self.has_relation_to(
            db,
            target,
            constraints,
            inferable,
            TypeRelation::Assignability,
        )
    }

    pub(super) fn when_constraint_set_assignable_to<'c>(
        self,
        db: &'db dyn Db,
        target: Type<'db>,
        constraints: &'c ConstraintSetBuilder<'db>,
    ) -> ConstraintSet<'db, 'c> {
        self.has_relation_to(
            db,
            target,
            constraints,
            InferableTypeVars::None,
            TypeRelation::ConstraintSetAssignability,
        )
    }

    /// Return `true` if it would be redundant to add `self` to a union that already contains `other`.
    ///
    /// See [`TypeRelation::Redundancy`] for more details.
    pub(super) fn is_redundant_with(self, db: &'db dyn Db, other: Type<'db>) -> bool {
        #[salsa::tracked(cycle_initial=|_, _, _, _| true, heap_size=ruff_memory_usage::heap_size)]
        fn is_redundant_with_impl<'db>(
            db: &'db dyn Db,
            self_ty: Type<'db>,
            other: Type<'db>,
        ) -> bool {
            self_ty
                .has_relation_to(
                    db,
                    other,
                    &ConstraintSetBuilder::new(),
                    InferableTypeVars::None,
                    TypeRelation::Redundancy { pure: false },
                )
                .is_always_satisfied(db)
        }

        if self == other {
            return true;
        }

        is_redundant_with_impl(db, self, other)
    }

    pub(super) fn has_relation_to<'c>(
        self,
        db: &'db dyn Db,
        target: Type<'db>,
        constraints: &'c ConstraintSetBuilder<'db>,
        inferable: InferableTypeVars<'db>,
        relation: TypeRelation,
    ) -> ConstraintSet<'db, 'c> {
        let relation_visitor = HasRelationToVisitor::default(constraints);
        let disjointness_visitor = IsDisjointVisitor::default(constraints);
        let materialization_visitor = ApplyTypeMappingVisitor::default();
        let checker = TypeRelationChecker {
            constraints,
            inferable,
            relation,
            context_tree: ErrorContextTree::disabled(),
            given: ConstraintSet::from_bool(constraints, false),
            relation_visitor: &relation_visitor,
            disjointness_visitor: &disjointness_visitor,
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
    pub(crate) fn is_equivalent_to(self, db: &'db dyn Db, other: Type<'db>) -> bool {
        self.when_equivalent_to(db, other, &ConstraintSetBuilder::new())
            .is_always_satisfied(db)
    }

    pub(crate) fn is_equivalent_to_with_materialization_visitor(
        self,
        db: &'db dyn Db,
        other: Type<'db>,
        materialization_visitor: &ApplyTypeMappingVisitor<'db>,
    ) -> bool {
        self.when_equivalent_to_with_materialization_visitor(
            db,
            other,
            &ConstraintSetBuilder::new(),
            materialization_visitor,
        )
        .is_always_satisfied(db)
    }

    pub(crate) fn when_equivalent_to<'c>(
        self,
        db: &'db dyn Db,
        other: Type<'db>,
        constraints: &'c ConstraintSetBuilder<'db>,
    ) -> ConstraintSet<'db, 'c> {
        let materialization_visitor = ApplyTypeMappingVisitor::default();
        self.when_equivalent_to_with_materialization_visitor(
            db,
            other,
            constraints,
            &materialization_visitor,
        )
    }

    pub(crate) fn when_equivalent_to_with_materialization_visitor<'c>(
        self,
        db: &'db dyn Db,
        other: Type<'db>,
        constraints: &'c ConstraintSetBuilder<'db>,
        materialization_visitor: &ApplyTypeMappingVisitor<'db>,
    ) -> ConstraintSet<'db, 'c> {
        let relation_visitor = HasRelationToVisitor::default(constraints);
        let disjointness_visitor = IsDisjointVisitor::default(constraints);
        let checker = EquivalenceChecker {
            constraints,
            given: ConstraintSet::from_bool(constraints, false),
            relation_visitor: &relation_visitor,
            disjointness_visitor: &disjointness_visitor,
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
    pub(crate) fn is_disjoint_from(self, db: &'db dyn Db, other: Type<'db>) -> bool {
        let constraints = ConstraintSetBuilder::new();
        self.when_disjoint_from(db, other, &constraints, InferableTypeVars::None)
            .is_always_satisfied(db)
    }

    pub(crate) fn when_disjoint_from<'c>(
        self,
        db: &'db dyn Db,
        other: Type<'db>,
        constraints: &'c ConstraintSetBuilder<'db>,
        inferable: InferableTypeVars<'db>,
    ) -> ConstraintSet<'db, 'c> {
        let relation_visitor = HasRelationToVisitor::default(constraints);
        let disjointness_visitor = IsDisjointVisitor::default(constraints);
        let materialization_visitor = ApplyTypeMappingVisitor::default();
        let checker = DisjointnessChecker {
            constraints,
            inferable,
            given: ConstraintSet::from_bool(constraints, false),
            disjointness_visitor: &disjointness_visitor,
            relation_visitor: &relation_visitor,
            materialization_visitor: &materialization_visitor,
        };
        checker.check_type_pair(db, self, other)
    }
}

/// A [`PairVisitor`] that is used in `has_relation_to` methods.
pub(crate) type HasRelationToVisitor<'db, 'c> =
    CycleDetector<TypeRelation, (Type<'db>, Type<'db>, TypeRelation), ConstraintSet<'db, 'c>>;

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

#[derive(Clone)]
pub(super) struct TypeRelationChecker<'a, 'c, 'db> {
    pub(super) constraints: &'c ConstraintSetBuilder<'db>,
    pub(super) inferable: InferableTypeVars<'db>,
    pub(super) relation: TypeRelation,
    context_tree: ErrorContextTree<'db>,
    given: ConstraintSet<'db, 'c>,

    // N.B. these fields are private to reduce the risk of
    // "double-visiting" a given pair of types. You should
    // generally only ever call `self.relation_visitor.visit()`
    // or `self.disjointness_visitor.visit()` from
    // `check_type_pair`, never from `check_typeddict_pair` or
    // any other more "low-level" method.
    relation_visitor: &'a HasRelationToVisitor<'db, 'c>,
    disjointness_visitor: &'a IsDisjointVisitor<'db, 'c>,
    pub(super) materialization_visitor: &'a ApplyTypeMappingVisitor<'db>,
}

impl<'a, 'c, 'db> TypeRelationChecker<'a, 'c, 'db> {
    pub(super) fn subtyping(
        constraints: &'c ConstraintSetBuilder<'db>,
        inferable: InferableTypeVars<'db>,
        relation_visitor: &'a HasRelationToVisitor<'db, 'c>,
        disjointness_visitor: &'a IsDisjointVisitor<'db, 'c>,
        materialization_visitor: &'a ApplyTypeMappingVisitor<'db>,
    ) -> Self {
        Self {
            constraints,
            inferable,
            relation: TypeRelation::Subtyping,
            context_tree: ErrorContextTree::disabled(),
            given: ConstraintSet::from_bool(constraints, false),
            relation_visitor,
            disjointness_visitor,
            materialization_visitor,
        }
    }

    pub(super) fn constraint_set_assignability(
        constraints: &'c ConstraintSetBuilder<'db>,
        relation_visitor: &'a HasRelationToVisitor<'db, 'c>,
        disjointness_visitor: &'a IsDisjointVisitor<'db, 'c>,
        materialization_visitor: &'a ApplyTypeMappingVisitor<'db>,
    ) -> Self {
        Self {
            constraints,
            inferable: InferableTypeVars::None,
            relation: TypeRelation::ConstraintSetAssignability,
            context_tree: ErrorContextTree::disabled(),
            given: ConstraintSet::from_bool(constraints, false),
            relation_visitor,
            disjointness_visitor,
            materialization_visitor,
        }
    }

    pub(super) fn with_inferable_typevars(&self, inferable: InferableTypeVars<'db>) -> Self {
        Self {
            inferable,
            ..self.clone()
        }
    }

    pub(super) fn always(&self) -> ConstraintSet<'db, 'c> {
        ConstraintSet::from_bool(self.constraints, true)
    }

    pub(super) fn never(&self) -> ConstraintSet<'db, 'c> {
        ConstraintSet::from_bool(self.constraints, false)
    }

    /// Provide context about a failing (assignability) relation between two types.
    pub(super) fn provide_context(&self, get_context: impl FnOnce() -> ErrorContext<'db>) {
        self.context_tree.push(get_context);
    }

    /// Overwrite the error context tree with a new root context and child nodes.
    pub(super) fn set_context(
        &self,
        root: ErrorContext<'db>,
        children: impl IntoIterator<Item = ErrorContextTree<'db>>,
    ) {
        self.context_tree.set(root, children);
    }

    /// Return true if error context collection is currently enabled.
    pub(super) fn is_context_collection_enabled(&self) -> bool {
        self.context_tree.is_enabled()
    }

    /// Temporarily suppress error context collection for the duration of `f`.
    ///
    /// Note: we may eventually not need this method once we properly retain error
    /// context everywhere.
    pub(super) fn without_context_collection<R>(&self, f: impl FnOnce() -> R) -> R {
        let was_enabled = self.context_tree.is_enabled();
        self.context_tree.set_enabled(false);
        let result = f();
        self.context_tree.set_enabled(was_enabled);
        result
    }

    fn with_recursion_guard(
        &self,
        source: Type<'db>,
        target: Type<'db>,
        work: impl FnOnce() -> ConstraintSet<'db, 'c>,
    ) -> ConstraintSet<'db, 'c> {
        self.relation_visitor
            .visit((source, target, self.relation), work)
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
    fn is_metaclass_instance(db: &'db dyn Db, target: Type<'db>) -> bool {
        target.as_nominal_instance().is_some_and(|instance| {
            KnownClass::Type
                .try_to_class_literal(db)
                .is_some_and(|type_class| {
                    instance
                        .class(db)
                        .is_subclass_of(db, ClassType::NonGeneric(ClassLiteral::Static(type_class)))
                })
        })
    }

    /// Can we check `target`s relation to a `type[T]` in either the metaclass-instance domain (it
    /// must pass `is_metaclass_instance`) or the regular instance domain (it must have Some
    /// `.to_instance()`)?
    fn can_check_typevar_subclass_relation_to_target(db: &'db dyn Db, target: Type<'db>) -> bool {
        Self::is_metaclass_instance(db, target) || target.to_instance(db).is_some()
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
    /// If `A` has no `.to_instance()` projection and is not a metaclass instance type, it won't
    /// pass the `can_check_typevar_subclass_relation_to_target` guard, and this helper does not
    /// decide the relation; it will fall through to other type-pair branches.
    fn check_typevar_subclass_relation_to_target(
        &self,
        db: &'db dyn Db,
        source_subclass: SubclassOfType<'db>,
        target: Type<'db>,
    ) -> ConstraintSet<'db, 'c> {
        source_subclass
            .into_type_var()
            .when_some_and(db, self.constraints, |source_i| {
                if Self::is_metaclass_instance(db, target) {
                    self.check_type_pair(db, source_subclass.to_metaclass_instance(db), target)
                } else {
                    target
                        .to_instance(db)
                        .when_some_and(db, self.constraints, |target_i| {
                            self.check_type_pair(db, Type::TypeVar(source_i), target_i)
                        })
                }
            })
    }

    /// Return a constraint set indicating the conditions under which `self.relation` holds between `source` and `target`.
    pub(super) fn check_type_pair(
        &self,
        db: &'db dyn Db,
        source: Type<'db>,
        target: Type<'db>,
    ) -> ConstraintSet<'db, 'c> {
        if let Some(source) = source.materialized_divergent_fallback() {
            return self.check_type_pair(db, source, target);
        }

        if let Some(target) = target.materialized_divergent_fallback() {
            return self.check_type_pair(db, source, target);
        }

        // Subtyping implies assignability, so if subtyping is reflexive and the two types are
        // equal, it is both a subtype and assignable. Assignability is always reflexive.
        //
        // Note that we could do a full equivalence check here, but that would be both expensive
        // and unnecessary. This early return is only an optimisation.
        if self.relation.can_safely_assume_reflexivity(source) && source == target {
            return self.always();
        }

        // Handle constraint implication first. If either `source` or `target` is a typevar, check
        // the constraint set to see if the corresponding constraint is satisfied.
        if self.relation == TypeRelation::SubtypingAssuming
            && (source.is_type_var() || target.is_type_var())
        {
            return self
                .given
                .implies_subtype_of(db, self.constraints, source, target);
        }

        // Handle the constraint-set-based assignability relation next. Comparisons with a
        // typevar are translated directly into a constraint set.
        if self.relation.is_constraint_set_assignability() {
            // A typevar satisfies a relation when...it satisfies the relation. Yes that's a
            // tautology! We're moving the caller's subtyping/assignability requirement into a
            // constraint set. If the typevar has an upper bound or constraints, then the relation
            // only has to hold when the typevar has a valid specialization (i.e., one that
            // satisfies the upper bound/constraints).
            if let Type::TypeVar(bound_typevar) = source {
                return ConstraintSet::constrain_typevar(
                    db,
                    self.constraints,
                    bound_typevar,
                    Type::Never,
                    target,
                );
            } else if let Type::TypeVar(bound_typevar) = target {
                return ConstraintSet::constrain_typevar(
                    db,
                    self.constraints,
                    bound_typevar,
                    source,
                    Type::object(),
                );
            }
        }

        let should_expand_intersection = |intersection: IntersectionType<'db>| {
            intersection
                .positive(db)
                .iter()
                .any(|element| match element {
                    Type::TypeVar(tvar) => !tvar.is_inferable(db, self.inferable),
                    Type::NewTypeInstance(newtype) => newtype.concrete_base_type(db).is_union(),
                    _ => false,
                })
        };

        match (source, target) {
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

            (Type::TypeAlias(source_alias), _) => self.with_recursion_guard(source, target, || {
                self.check_type_pair(db, source_alias.value_type(db), target)
            }),

            (_, Type::TypeAlias(target_alias)) => self.with_recursion_guard(source, target, || {
                self.check_type_pair(db, source, target_alias.value_type(db))
            }),

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
                        self.check_type_pair(db, default_type, target)
                    })
                    .and(db, self.constraints, || {
                        field
                            .converter(db)
                            .map(|(_, output_ty)| output_ty)
                            .when_none_or(db, self.constraints, |converter_output_type| {
                                self.check_type_pair(db, converter_output_type, target)
                            })
                    })
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
                    TypeRelation::Assignability | TypeRelation::ConstraintSetAssignability => true,
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
                    TypeRelation::Subtyping | TypeRelation::SubtypingAssuming => false,
                    TypeRelation::Assignability | TypeRelation::ConstraintSetAssignability => true,
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

            // `type[T]` is a subtype of the class object `A` if every instance of `T` is a subtype
            // of an instance of `A`. If `A` is a metaclass instance (instance of a specific
            // subclass of `type`), we instead compare in the metaclass-instance domain, since
            // collapsing `A` through `to_instance()` would erase it to `object` (we have no
            // precise representation for "all instances of any classes with a given metaclass").
            (Type::SubclassOf(subclass_of), _)
                if subclass_of.is_type_var()
                    && Self::can_check_typevar_subclass_relation_to_target(db, target) =>
            {
                self.check_typevar_subclass_relation_to_target(db, subclass_of, target)
            }

            // And vice versa. (No special metaclass handling is needed in this direction, since
            // "collapse to 'object'" in this case is a sound over-approximation.)
            (_, Type::SubclassOf(subclass_of))
                if subclass_of.is_type_var() && source.to_instance(db).is_some() =>
            {
                subclass_of
                    .into_type_var()
                    .zip(source.to_instance(db))
                    .when_some_and(db, self.constraints, |(target_i, source_i)| {
                        self.check_type_pair(db, source_i, Type::TypeVar(target_i))
                    })
            }

            // A fully static typevar is a subtype of its upper bound, and to something similar to
            // the union of its constraints. An unbound, unconstrained, fully static typevar has an
            // implicit upper bound of `object` (which is handled above).
            (Type::TypeVar(bound_typevar), _)
                if !bound_typevar.is_inferable(db, self.inferable)
                    && bound_typevar.typevar(db).bound_or_constraints(db).is_some() =>
            {
                match bound_typevar.typevar(db).bound_or_constraints(db) {
                    None => unreachable!(),
                    Some(TypeVarBoundOrConstraints::UpperBound(bound)) => {
                        self.check_type_pair(db, bound, target)
                    }
                    Some(TypeVarBoundOrConstraints::Constraints(typevar_constraints)) => {
                        typevar_constraints.elements(db).iter().when_all(
                            db,
                            self.constraints,
                            |constraint| self.check_type_pair(db, *constraint, target),
                        )
                    }
                }
            }

            // If the typevar is constrained, there must be multiple constraints, and the typevar
            // might be specialized to any one of them. However, the constraints do not have to be
            // disjoint, which means an lhs type might be a subtype of all of the constraints.
            (_, Type::TypeVar(bound_typevar))
                if !bound_typevar.is_inferable(db, self.inferable)
                    && !bound_typevar
                        .typevar(db)
                        .constraints(db)
                        .when_some_and(db, self.constraints, |constraints| {
                            constraints.iter().when_all(db, self.constraints, |c| {
                                self.check_type_pair(db, source, *c)
                            })
                        })
                        .is_never_satisfied(db) =>
            {
                // TODO: The repetition here isn't great, but we really need the fallthrough logic,
                // where this arm only engages if it returns true (or in the world of constraints,
                // not false). Once we're using real constraint sets instead of bool, we should be
                // able to simplify the typevar logic.
                bound_typevar.typevar(db).constraints(db).when_some_and(
                    db,
                    self.constraints,
                    |constraints| {
                        constraints.iter().when_all(db, self.constraints, |c| {
                            self.check_type_pair(db, source, *c)
                        })
                    },
                )
            }

            (Type::TypeVar(bound_typevar), _) if bound_typevar.is_inferable(db, self.inferable) => {
                // The implicit lower bound of a typevar is `Never`, which means
                // that it is always assignable to any other type.

                // TODO: record the unification constraints

                self.always()
            }

            // Fast path for various types that we know `object` is never a subtype of
            // (`object` can be a subtype of some protocols, or of itself, but those cases are
            // handled above).
            (
                Type::NominalInstance(source),
                Type::NominalInstance(_)
                | Type::SubclassOf(_)
                | Type::Callable(_)
                | Type::ProtocolInstance(_),
            ) if source.is_object() => self.never(),

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

            (Type::Union(union), _) => {
                union
                    .elements(db)
                    .iter()
                    .when_all(db, self.constraints, |&elem_ty| {
                        let constraint_set = self.check_type_pair(db, elem_ty, target);
                        if constraint_set.is_never_satisfied(db) {
                            self.provide_context(|| ErrorContext::NotAllUnionElementsAssignable {
                                element: elem_ty,
                                union: source,
                                target,
                            });
                        }
                        constraint_set
                    })
            }

            (_, Type::Union(union)) => {
                let is_new_type_of_union = || {
                    // Normally non-unions cannot directly contain unions in our model due to the fact that we
                    // enforce a DNF structure on our set-theoretic types. However, it *is* possible for there
                    // to be a newtype of a union, for an intersection to contain a newtype of a union, or for
                    // a non-inferable typevar (possibly inside an intersection) to widen to a bound or set of
                    // constraints that exposes a union; this requires special handling.
                    match source {
                        Type::Intersection(intersection)
                            if should_expand_intersection(intersection) =>
                        {
                            self.check_type_pair(
                                db,
                                intersection.with_expanded_typevars_and_newtypes(db),
                                target,
                            )
                        }
                        Type::NewTypeInstance(newtype) => {
                            let concrete_base = newtype.concrete_base_type(db);
                            if concrete_base.is_union() {
                                self.check_type_pair(db, concrete_base, target)
                            } else {
                                self.never()
                            }
                        }
                        _ => self.never(),
                    }
                };

                let mut elements_context = vec![];
                let context_collection_enabled = self.is_context_collection_enabled();

                let elements = union.elements(db);
                let result = elements
                    .iter()
                    .when_any(db, self.constraints, |&elem_ty| {
                        let result = self.check_type_pair(db, source, elem_ty);
                        if context_collection_enabled {
                            let ctx = self.context_tree.take();
                            if !ctx.is_empty() {
                                elements_context.push(ctx);
                            }
                        }
                        result
                    })
                    .or(db, self.constraints, is_new_type_of_union);

                if context_collection_enabled
                    && !elements_context.is_empty()
                    && result.is_never_satisfied(db)
                {
                    let elements_without_context = elements.len() - elements_context.len();
                    if elements_without_context > 0 && elements_without_context < elements.len() {
                        elements_context.push(
                            ErrorContext::NotAssignableToNOtherUnionElements {
                                n: elements_without_context,
                            }
                            .into(),
                        );
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

            // If both sides are intersections we need to handle the right side first
            // (A & B & C) is a subtype of (A & B) because the left is a subtype of both A and B,
            // but none of A, B, or C is a subtype of (A & B).
            (_, Type::Intersection(intersection)) => intersection
                .positive(db)
                .iter()
                .when_all(db, self.constraints, |&pos_ty| {
                    let constraint_set = self.check_type_pair(db, source, pos_ty);
                    if constraint_set.is_never_satisfied(db) {
                        self.provide_context(|| ErrorContext::NotAssignableToIntersectionElement {
                            source,
                            element: pos_ty,
                            intersection: target,
                        });
                    }
                    constraint_set
                })
                .and(db, self.constraints, || {
                    // For subtyping, we would want to check whether the *top materialization* of `source`
                    // is disjoint from the *top materialization* of `neg_ty`. As an optimization, however,
                    // we can avoid this explicit transformation here, since our `Type::is_disjoint_from`
                    // implementation already only returns true for `T.is_disjoint_from(U)` if the *top
                    // materialization* of `T` is disjoint from the *top materialization* of `U`.
                    //
                    // Note that the implementation of redundancy here may be too strict from a
                    // theoretical perspective: under redundancy, `T <: ~U` if `Bottom[T]` is disjoint
                    // from `Top[U]` and `Bottom[U]` is disjoint from `Top[T]`. It's possible that this
                    // could be improved. For now, however, we err on the side of strictness for our
                    // redundancy implementation: a fully complete implementation of redundancy may lead
                    // to non-transitivity (highly undesirable); and pragmatically, a full implementation
                    // of redundancy may not generally lead to simpler types in many situations.
                    let source_ty = match self.relation {
                        TypeRelation::Subtyping
                        | TypeRelation::Redundancy { .. }
                        | TypeRelation::SubtypingAssuming => source,
                        TypeRelation::Assignability | TypeRelation::ConstraintSetAssignability => {
                            source.bottom_materialization(db)
                        }
                    };
                    intersection
                        .negative(db)
                        .iter()
                        .when_all(db, self.constraints, |&neg_ty| {
                            let neg_ty = match self.relation {
                                TypeRelation::Subtyping
                                | TypeRelation::Redundancy { .. }
                                | TypeRelation::SubtypingAssuming => neg_ty,
                                TypeRelation::Assignability
                                | TypeRelation::ConstraintSetAssignability => {
                                    neg_ty.bottom_materialization(db)
                                }
                            };
                            self.as_disjointness_checker()
                                .check_type_pair(db, source_ty, neg_ty)
                        })
                }),

            (Type::Intersection(intersection), _) => {
                // An intersection type is a subtype of another type if at least one of its
                // positive elements is a subtype of that type. If there are no positive elements,
                // we treat `object` as the implicit positive element (e.g., `~str` is semantically
                // `object & ~str`).

                let mut elements_context = vec![];
                let context_collection_enabled = self.is_context_collection_enabled();

                let result = intersection
                    .positive_elements_or_object(db)
                    .when_any(db, self.constraints, |elem_ty| {
                        let result = self.check_type_pair(db, elem_ty, target);
                        if context_collection_enabled {
                            let ctx = self.context_tree.take();
                            if !ctx.is_empty() {
                                elements_context.push(ctx);
                            }
                        }
                        result
                    })
                    .or(db, self.constraints, || {
                        if should_expand_intersection(intersection) {
                            self.check_type_pair(
                                db,
                                intersection.with_expanded_typevars_and_newtypes(db),
                                target,
                            )
                        } else {
                            self.never()
                        }
                    });

                if context_collection_enabled
                    && !elements_context.is_empty()
                    && result.is_never_satisfied(db)
                {
                    self.set_context(
                        ErrorContext::NoIntersectionElementAssignableToTarget {
                            intersection: source,
                            target,
                        },
                        elements_context,
                    );
                }

                result
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
            (_, Type::TypeVar(typevar)) if typevar.is_inferable(db, self.inferable) => {
                if self.relation.is_assignability() {
                    // TODO: record the unification constraints
                    typevar.typevar(db).upper_bound(db).when_none_or(
                        db,
                        self.constraints,
                        |bound| self.check_type_pair(db, source, bound),
                    )
                } else {
                    self.never()
                }
            }
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
                self.check_type_pair(db, source_newtype.concrete_base_type(db), target)
            }

            // Note that the definition of `Type::AlwaysFalsy` depends on the return value of `__bool__`.
            // If `__bool__` always returns True or False, it can be treated as a subtype of `AlwaysTruthy` or `AlwaysFalsy`, respectively.
            (_, Type::AlwaysFalsy) => {
                ConstraintSet::from_bool(self.constraints, source.bool(db).is_always_false())
            }
            (_, Type::AlwaysTruthy) => {
                ConstraintSet::from_bool(self.constraints, source.bool(db).is_always_true())
            }
            // Currently, the only supertype of `AlwaysFalsy` and `AlwaysTruthy` is the universal set (object instance).
            (Type::AlwaysFalsy | Type::AlwaysTruthy, _) => {
                self.with_recursion_guard(source, target, || {
                    self.check_type_pair(db, Type::object(), target)
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
                .with_recursion_guard(source, target, || {
                    self.check_callable_pair(db, source_callable, target_callable)
                }),

            (_, Type::Callable(target_callable)) => {
                self.with_recursion_guard(source, target, || {
                    source
                        .try_upcast_to_callable_with_policy(db, UpcastPolicy::from(self.relation))
                        .when_some_and(db, self.constraints, |callables| {
                            self.check_callables_vs_callable(db, &callables, target_callable)
                        })
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
                self.check_type_pair(db, KnownClass::Type.to_instance(db), target)
            }

            (_, Type::ProtocolInstance(target_proto)) => {
                self.with_recursion_guard(source, target, || {
                    self.check_type_satisfies_protocol(db, source, target_proto)
                })
            }

            // A protocol instance can never be a subtype of a nominal type, with the *sole* exception of `object`.
            (Type::ProtocolInstance(_), _) => self.never(),

            (Type::TypedDict(source_td), Type::TypedDict(target_td)) => {
                self.with_recursion_guard(source, target, || {
                    self.check_typeddict_pair(db, source_td, target_td)
                })
            }

            // TODO: When we support `closed` and/or `extra_items`, we could allow assignments to other
            // compatible `Mapping`s. `extra_items` could also allow for some assignments to `dict`, as
            // long as `total=False`. (But then again, does anyone want a non-total `TypedDict` where all
            // key types are a supertype of the extra items type?)
            (Type::TypedDict(typed_dict), _) => self.with_recursion_guard(source, target, || {
                let spec = &[KnownClass::Str.to_instance(db), Type::object()];
                let str_object_map = KnownClass::Mapping.to_specialized_instance(db, spec);
                let result = self.check_type_pair(db, str_object_map, target);
                if result.is_never_satisfied(db) {
                    if let Type::NominalInstance(instance) = target
                        && instance.class(db).is_known(db, KnownClass::Dict)
                    {
                        self.provide_context(|| {
                            ErrorContext::TypedDictNotAssignableToDict(typed_dict)
                        });
                    }
                }
                result
            }),

            // A non-`TypedDict` cannot subtype a `TypedDict`
            (_, Type::TypedDict(_)) => self.never(),

            // A string literal `Literal["abc"]` is assignable to `str` *and* to
            // `Sequence[Literal["a", "b", "c"]]` because strings are sequences of their characters.
            (Type::LiteralValue(literal), Type::NominalInstance(instance))
                if literal.is_string() =>
            {
                let value = literal.as_string().unwrap();
                let target_class = instance.class(db);

                if target_class.is_known(db, KnownClass::Str) {
                    return self.always();
                }

                if let Some(sequence_class) = KnownClass::Sequence.try_to_class_literal(db)
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
                    .to_specialized_class_type(db, &[spec])
                    .when_some_and(db, self.constraints, |sequence| {
                        self.check_class_pair(db, sequence, target_class)
                    })
            }

            (Type::LiteralValue(literal), _) if literal.is_string() => self.never(),

            // A bytes literal `Literal[b"abc"]` is assignable to `bytes` *and* to
            // `Sequence[Literal[97, 98, 99]]` because bytes are sequences of integers.
            (Type::LiteralValue(literal), Type::NominalInstance(instance))
                if literal.is_bytes() =>
            {
                let value = literal.as_bytes().unwrap();
                let target_class = instance.class(db);

                if target_class.is_known(db, KnownClass::Bytes) {
                    return self.always();
                }

                if let Some(sequence_class) = KnownClass::Sequence.try_to_class_literal(db)
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
                    .to_specialized_class_type(db, &[spec])
                    .when_some_and(db, self.constraints, |sequence| {
                        self.check_class_pair(db, sequence, target_class)
                    })
            }

            (Type::LiteralValue(literal), _) if literal.is_bytes() => self.never(),

            // An instance is a subtype of an enum literal, if it is an instance of the enum class
            // and the enum has only one member.
            (Type::NominalInstance(_), Type::LiteralValue(literal)) if literal.is_enum() => {
                let target_enum_literal = literal.as_enum().unwrap();
                if target_enum_literal.enum_class_instance(db) != source {
                    return self.never();
                }

                ConstraintSet::from_bool(
                    self.constraints,
                    is_single_member_enum(db, target_enum_literal.enum_class(db)),
                )
            }

            // Except for the special `BytesLiteral`, `LiteralString`, and string literal cases above,
            // most `Literal` types delegate to their instance fallbacks
            // unless `source` is exactly equivalent to `target` (handled above)
            (Type::ModuleLiteral(_) | Type::LiteralValue(_) | Type::FunctionLiteral(_), _) => {
                source.literal_fallback_instance(db).when_some_and(
                    db,
                    self.constraints,
                    |source_instance| self.check_type_pair(db, source_instance, target),
                )
            }

            // The same reasoning applies for these special callable types:
            (Type::BoundMethod(_), _) => {
                self.check_type_pair(db, KnownClass::MethodType.to_instance(db), target)
            }
            (Type::KnownBoundMethod(method), _) => {
                self.check_type_pair(db, method.class().to_instance(db), target)
            }
            (Type::WrapperDescriptor(_), _) => self.check_type_pair(
                db,
                KnownClass::WrapperDescriptorType.to_instance(db),
                target,
            ),

            (Type::DataclassDecorator(_) | Type::DataclassTransformer(_), _) => {
                // TODO: Implement subtyping using an equivalent `Callable` type.
                self.never()
            }

            // `TypeIs` is invariant.
            (Type::TypeIs(source), Type::TypeIs(target)) => {
                let source_return = source.return_type(db);
                let target_return = target.return_type(db);
                self.check_type_pair(db, source_return, target_return).and(
                    db,
                    self.constraints,
                    || self.check_type_pair(db, target_return, source_return),
                )
            }

            // `TypeGuard` is covariant.
            (Type::TypeGuard(source), Type::TypeGuard(target)) => {
                self.check_type_pair(db, source.return_type(db), target.return_type(db))
            }

            // `TypeIs[T]` and `TypeGuard[T]` are subtypes of `bool`.
            (Type::TypeIs(_) | Type::TypeGuard(_), _) => {
                self.check_type_pair(db, KnownClass::Bool.to_instance(db), target)
            }

            // Function-like callables are subtypes of `FunctionType`
            (Type::Callable(callable), _) if callable.is_function_like(db) => {
                self.check_type_pair(db, KnownClass::FunctionType.to_instance(db), target)
            }

            (Type::Callable(_), _) => self.never(),

            (Type::BoundSuper(source), Type::BoundSuper(target)) => self
                .as_equivalence_checker()
                .check_bound_super_pair(db, source, target),

            (Type::BoundSuper(_), _) => {
                self.check_type_pair(db, KnownClass::Super.to_instance(db), target)
            }

            (Type::SubclassOf(subclass_of), _) | (_, Type::SubclassOf(subclass_of))
                if subclass_of.is_type_var() =>
            {
                self.never()
            }

            // `Literal[<class 'C'>]` is a subtype of `type[B]` if `C` is a subclass of `B`,
            // since `type[B]` describes all possible runtime subclasses of the class object `B`.
            (Type::ClassLiteral(source_cls), Type::SubclassOf(target_subclass_ty)) => {
                target_subclass_ty
                    .subclass_of()
                    .into_class(db)
                    .map(|target_cls| {
                        self.check_class_pair(db, source_cls.default_specialization(db), target_cls)
                    })
                    .unwrap_or_else(|| {
                        ConstraintSet::from_bool(self.constraints, self.relation.is_assignability())
                    })
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
                target_subclass_ty
                    .subclass_of()
                    .into_class(db)
                    .map(|target_cls| {
                        self.check_class_pair(db, ClassType::Generic(source_alias), target_cls)
                    })
                    .unwrap_or_else(|| {
                        ConstraintSet::from_bool(self.constraints, self.relation.is_assignability())
                    })
            }

            // This branch asks: given two types `type[T]` and `type[S]`, is `type[T]` a subtype of `type[S]`?
            (Type::SubclassOf(source), Type::SubclassOf(target)) => {
                self.check_subclassof_pair(db, source, target)
            }

            // `Literal[str]` is a subtype of `type` because the `str` class object is an instance of its metaclass `type`.
            // `Literal[abc.ABC]` is a subtype of `abc.ABCMeta` because the `abc.ABC` class object
            // is an instance of its metaclass `abc.ABCMeta`.
            (Type::ClassLiteral(source_class), _) => {
                self.check_type_pair(db, source_class.metaclass_instance_type(db), target)
            }
            (Type::GenericAlias(source_alias), _) => self.check_type_pair(
                db,
                ClassType::Generic(source_alias).metaclass_instance_type(db),
                target,
            ),

            // `type[Any]` is a subtype of `type[object]`, and is assignable to any `type[...]`
            (Type::SubclassOf(subclass_of_ty), _) if subclass_of_ty.is_dynamic() => self
                .check_type_pair(db, KnownClass::Type.to_instance(db), target)
                .or(db, self.constraints, || {
                    ConstraintSet::from_bool(self.constraints, self.relation.is_assignability())
                        .and(db, self.constraints, || {
                            self.check_type_pair(db, target, KnownClass::Type.to_instance(db))
                        })
                }),

            // Any `type[...]` type is assignable to `type[Any]`
            (_, Type::SubclassOf(subclass_of_ty))
                if subclass_of_ty.is_dynamic() && self.relation.is_assignability() =>
            {
                self.check_type_pair(db, source, KnownClass::Type.to_instance(db))
            }

            // `type[str]` (== `SubclassOf("str")` in ty) describes all possible runtime subclasses
            // of the class object `str`. It is a subtype of `type` (== `Instance("type")`) because `str`
            // is an instance of `type`, and so all possible subclasses of `str` will also be instances of `type`.
            //
            // Similarly `type[enum.Enum]`  is a subtype of `enum.EnumMeta` because `enum.Enum`
            // is an instance of `enum.EnumMeta`. `type[Any]` and `type[Unknown]` do not participate in subtyping,
            // however, as they are not fully static types.
            (Type::SubclassOf(subclass_of_ty), _) => self.check_type_pair(
                db,
                subclass_of_ty
                    .subclass_of()
                    .into_class(db)
                    .map(|source_class| source_class.metaclass_instance_type(db))
                    .unwrap_or_else(|| KnownClass::Type.to_instance(db)),
                target,
            ),

            // For example: `Type::SpecialForm(SpecialFormType::Type)` is a subtype of `Type::NominalInstance(_SpecialForm)`,
            // because `Type::SpecialForm(SpecialFormType::Type)` is a set with exactly one runtime value in it
            // (the symbol `typing.Type`), and that symbol is known to be an instance of `typing._SpecialForm` at runtime.
            (Type::SpecialForm(source_form), _) => {
                self.check_type_pair(db, source_form.instance_fallback(db), target)
            }

            (Type::KnownInstance(source), _) => {
                self.check_type_pair(db, source.instance_fallback(db), target)
            }

            // `bool` is a subtype of `int`, because `bool` subclasses `int`,
            // which means that all instances of `bool` are also instances of `int`
            (Type::NominalInstance(source_i), Type::NominalInstance(target_i)) => self
                .with_recursion_guard(source, target, || {
                    self.check_nominal_instance_pair(db, source_i, target_i)
                }),

            (Type::PropertyInstance(source_p), Type::PropertyInstance(target_p)) => self
                .with_recursion_guard(source, target, || {
                    self.check_property_instance_pair(db, source_p, target_p)
                }),

            (Type::PropertyInstance(_), _) => {
                self.check_type_pair(db, KnownClass::Property.to_instance(db), target)
            }
            (_, Type::PropertyInstance(_)) => {
                self.check_type_pair(db, source, KnownClass::Property.to_instance(db))
            }
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
        let check_optional_methods = |source, target| match (source, target) {
            (None, None) => self.always(),
            (Some(source), Some(target)) => self.check_type_pair(db, source, target),
            (None | Some(_), None | Some(_)) => self.never(),
        };

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
    }

    pub(super) fn as_equivalence_checker(&self) -> EquivalenceChecker<'_, 'c, 'db> {
        EquivalenceChecker {
            constraints: self.constraints,
            given: self.given,
            relation_visitor: self.relation_visitor,
            disjointness_visitor: self.disjointness_visitor,
            materialization_visitor: self.materialization_visitor,
        }
    }

    pub(super) fn as_disjointness_checker(&self) -> DisjointnessChecker<'_, 'c, 'db> {
        DisjointnessChecker {
            constraints: self.constraints,
            inferable: self.inferable,
            given: self.given,
            relation_visitor: self.relation_visitor,
            disjointness_visitor: self.disjointness_visitor,
            materialization_visitor: self.materialization_visitor,
        }
    }
}

pub(super) struct EquivalenceChecker<'a, 'c, 'db> {
    pub(super) constraints: &'c ConstraintSetBuilder<'db>,
    given: ConstraintSet<'db, 'c>,

    // N.B. these fields are private to reduce the risk of
    // "double-visiting" a given pair of types. You should
    // generally only ever call `self.relation_visitor.visit()`
    // or `self.disjointness_visitor.visit()` from
    // `check_type_pair`, never from `check_typeddict_pair` or
    // any other more "low-level" method.
    relation_visitor: &'a HasRelationToVisitor<'db, 'c>,
    disjointness_visitor: &'a IsDisjointVisitor<'db, 'c>,
    materialization_visitor: &'a ApplyTypeMappingVisitor<'db>,
}

impl<'c, 'db> EquivalenceChecker<'_, 'c, 'db> {
    fn as_relation_checker<'a>(
        &'a self,
        materialization_visitor: &'a ApplyTypeMappingVisitor<'db>,
    ) -> TypeRelationChecker<'a, 'c, 'db> {
        TypeRelationChecker {
            relation: TypeRelation::Redundancy { pure: true },
            constraints: self.constraints,
            context_tree: ErrorContextTree::disabled(),
            given: self.given,
            inferable: InferableTypeVars::None,
            relation_visitor: self.relation_visitor,
            disjointness_visitor: self.disjointness_visitor,
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
        // Recursive materialization fallbacks depend on the comparison root, so each directional
        // pass needs fresh materialization caches. Nested equivalence checks still share the
        // materialization-equivalence recursion guard to avoid re-entering the same comparison.
        let left_to_right_materialization_visitor =
            self.materialization_visitor.for_new_materialization_root();
        self.as_relation_checker(&left_to_right_materialization_visitor)
            .check_type_pair(db, left, right)
            .and(db, self.constraints, || {
                let right_to_left_materialization_visitor =
                    self.materialization_visitor.for_new_materialization_root();
                self.as_relation_checker(&right_to_left_materialization_visitor)
                    .check_type_pair(db, right, left)
            })
    }
}

pub(super) struct DisjointnessChecker<'a, 'c, 'db> {
    pub(super) constraints: &'c ConstraintSetBuilder<'db>,
    pub(super) inferable: InferableTypeVars<'db>,
    given: ConstraintSet<'db, 'c>,

    // N.B. these fields are private to reduce the risk of
    // "double-visiting" a given pair of types. You should
    // generally only ever call `self.relation_visitor.visit()`
    // or `self.disjointness_visitor.visit()` from
    // `check_type_pair`, never from `check_typeddict_pair` or
    // any other more "low-level" method.
    disjointness_visitor: &'a IsDisjointVisitor<'db, 'c>,
    relation_visitor: &'a HasRelationToVisitor<'db, 'c>,
    materialization_visitor: &'a ApplyTypeMappingVisitor<'db>,
}

impl<'a, 'c, 'db> DisjointnessChecker<'a, 'c, 'db> {
    pub(super) fn new(
        constraints: &'c ConstraintSetBuilder<'db>,
        inferable: InferableTypeVars<'db>,
        relation_visitor: &'a HasRelationToVisitor<'db, 'c>,
        disjointness_visitor: &'a IsDisjointVisitor<'db, 'c>,
        materialization_visitor: &'a ApplyTypeMappingVisitor<'db>,
    ) -> Self {
        Self {
            constraints,
            inferable,
            given: ConstraintSet::from_bool(constraints, false),
            disjointness_visitor,
            relation_visitor,
            materialization_visitor,
        }
    }

    pub(super) fn as_relation_checker(
        &self,
        relation: TypeRelation,
    ) -> TypeRelationChecker<'_, 'c, 'db> {
        TypeRelationChecker {
            relation,
            constraints: self.constraints,
            inferable: self.inferable,
            context_tree: ErrorContextTree::disabled(),
            given: self.given,
            relation_visitor: self.relation_visitor,
            disjointness_visitor: self.disjointness_visitor,
            materialization_visitor: self.materialization_visitor,
        }
    }

    pub(super) fn as_equivalence_checker(&self) -> EquivalenceChecker<'_, 'c, 'db> {
        EquivalenceChecker {
            constraints: self.constraints,
            given: self.given,
            relation_visitor: self.relation_visitor,
            disjointness_visitor: self.disjointness_visitor,
            materialization_visitor: self.materialization_visitor,
        }
    }

    fn with_recursion_guard(
        &self,
        source: Type<'db>,
        target: Type<'db>,
        work: impl FnOnce() -> ConstraintSet<'db, 'c>,
    ) -> ConstraintSet<'db, 'c> {
        self.disjointness_visitor.visit((source, target), work)
    }

    fn any_protocol_members_absent_or_disjoint(
        &self,
        db: &'db dyn Db,
        protocol: ProtocolInstanceType<'db>,
        other: Type<'db>,
    ) -> ConstraintSet<'db, 'c> {
        protocol
            .interface(db)
            .members(db)
            .when_any(db, self.constraints, |member| {
                other
                    .member(db, member.name())
                    .place
                    .ignore_possibly_undefined()
                    .when_none_or(db, self.constraints, |attribute_type| {
                        self.protocol_member_has_disjoint_type_from_ty(db, &member, attribute_type)
                    })
            })
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
        if let Some(left) = left.materialized_divergent_fallback() {
            return self.check_type_pair(db, left, right);
        }

        if let Some(right) = right.materialized_divergent_fallback() {
            return self.check_type_pair(db, left, right);
        }

        match (left, right) {
            (Type::Never, _) | (_, Type::Never) => self.always(),

            (Type::Dynamic(_), _) | (_, Type::Dynamic(_)) => self.never(),
            (Type::Divergent(_), _) | (_, Type::Divergent(_)) => self.never(),

            (Type::TypeAlias(alias), _) => {
                let left_alias_ty = alias.value_type(db);
                self.with_recursion_guard(left, right, || {
                    self.check_type_pair(db, left_alias_ty, right)
                })
            }

            (_, Type::TypeAlias(alias)) => {
                let right_alias_ty = alias.value_type(db);
                self.with_recursion_guard(left, right, || {
                    self.check_type_pair(db, left, right_alias_ty)
                })
            }

            // `type[T]` is disjoint from a callable or protocol instance if its upper bound or constraints are.
            (
                Type::SubclassOf(subclass_of),
                other @ (Type::Callable(_) | Type::ProtocolInstance(_)),
            )
            | (
                other @ (Type::Callable(_) | Type::ProtocolInstance(_)),
                Type::SubclassOf(subclass_of),
            ) if subclass_of.is_type_var() => {
                let type_var = subclass_of
                    .subclass_of()
                    .with_transposed_type_var(db)
                    .into_type_var()
                    .unwrap();

                self.check_type_pair(db, Type::TypeVar(type_var), other)
            }

            // `type[T]` is disjoint from a class object `A` if every instance of `T` is disjoint from an instance of `A`.
            (Type::SubclassOf(subclass_of), other) | (other, Type::SubclassOf(subclass_of))
                if subclass_of.is_type_var() && other.to_instance(db).is_some() =>
            {
                subclass_of
                    .into_type_var()
                    .zip(other.to_instance(db))
                    .when_none_or(db, self.constraints, |(this_instance, other_instance)| {
                        self.check_type_pair(db, Type::TypeVar(this_instance), other_instance)
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
                match tvar.typevar(db).bound_or_constraints(db) {
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

            // TODO: Infer specializations here
            (Type::TypeVar(_), _) | (_, Type::TypeVar(_)) => self.never(),

            (Type::Union(union), other) | (other, Type::Union(union)) => union
                .elements(db)
                .iter()
                .when_all(db, self.constraints, |e| {
                    self.check_type_pair(db, *e, other)
                }),

            // If we have two intersections, we test the positive elements of each one against the other intersection
            // Negative elements need a positive element on the other side in order to be disjoint.
            // This is similar to what would happen if we tried to build a new intersection that combines the two
            (Type::Intersection(left_intersection), Type::Intersection(right_intersection)) => self
                .with_recursion_guard(left, right, || {
                    left_intersection
                        .positive(db)
                        .iter()
                        .when_any(db, self.constraints, |&pos_ty| {
                            self.check_type_pair(db, pos_ty, right)
                        })
                        .or(db, self.constraints, || {
                            right_intersection.positive(db).iter().when_any(
                                db,
                                self.constraints,
                                |&pos_ty| self.check_type_pair(db, pos_ty, left),
                            )
                        })
                }),

            (Type::Intersection(intersection), other)
            | (other, Type::Intersection(intersection)) => {
                self.with_recursion_guard(left, right, || {
                    intersection
                        .positive(db)
                        .iter()
                        .when_any(db, self.constraints, |&pos_ty| {
                            self.check_type_pair(db, pos_ty, other)
                        })
                        // A & B & Not[C] is disjoint from C
                        .or(db, self.constraints, || {
                            intersection.negative(db).iter().when_any(
                                db,
                                self.constraints,
                                |&neg_ty| {
                                    self.as_relation_checker(TypeRelation::Subtyping)
                                        .check_type_pair(db, other, neg_ty)
                                },
                            )
                        })
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
                ConstraintSet::from_bool(self.constraints, left.kind() != right.kind())
            }

            (Type::PropertyInstance(left), Type::PropertyInstance(right)) => {
                self.check_property_instance_pair(db, left, right)
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
            ) => self.check_property_instance_pair(db, left, right),

            // any single-valued type is disjoint from another single-valued type
            // iff the two types are nonequal
            (
                // note `LiteralString` is not single-valued, but we handle the special case above
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
                ConstraintSet::from_bool(self.constraints, ty.bool(db).is_always_false())
            }
            (Type::AlwaysFalsy, ty) | (ty, Type::AlwaysFalsy) => {
                // Similarly, they are only disjoint if `ty.bool() == AlwaysTrue`.
                ConstraintSet::from_bool(self.constraints, ty.bool(db).is_always_true())
            }

            (Type::ProtocolInstance(left_proto), Type::ProtocolInstance(right_proto)) => self
                .with_recursion_guard(left, right, || {
                    self.check_protocol_instance_pair(db, left_proto, right_proto)
                }),

            (Type::ProtocolInstance(protocol), Type::SpecialForm(special_form))
            | (Type::SpecialForm(special_form), Type::ProtocolInstance(protocol)) => self
                .with_recursion_guard(left, right, || {
                    self.any_protocol_members_absent_or_disjoint(
                        db,
                        protocol,
                        special_form.instance_fallback(db),
                    )
                }),

            (Type::ProtocolInstance(protocol), Type::KnownInstance(known_instance))
            | (Type::KnownInstance(known_instance), Type::ProtocolInstance(protocol)) => self
                .with_recursion_guard(left, right, || {
                    self.any_protocol_members_absent_or_disjoint(
                        db,
                        protocol,
                        known_instance.instance_fallback(db),
                    )
                }),

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
            ) => self.with_recursion_guard(left, right, || {
                self.any_protocol_members_absent_or_disjoint(db, protocol, ty)
            }),

            // This is the same as the branch above --
            // once guard patterns are stabilised, it could be unified with that branch
            // (<https://github.com/rust-lang/rust/issues/129967>)
            (Type::ProtocolInstance(protocol), Type::NominalInstance(nominal))
            | (Type::NominalInstance(nominal), Type::ProtocolInstance(protocol))
                if nominal.class(db).is_final(db) =>
            {
                self.with_recursion_guard(left, right, || {
                    self.any_protocol_members_absent_or_disjoint(
                        db,
                        protocol,
                        Type::NominalInstance(nominal),
                    )
                })
            }

            (Type::ProtocolInstance(protocol), other)
            | (other, Type::ProtocolInstance(protocol)) => {
                self.with_recursion_guard(left, right, || {
                    protocol
                        .interface(db)
                        .members(db)
                        .when_any(db, self.constraints, |member| {
                            match other.member(db, member.name()).place {
                                Place::Defined(DefinedPlace {
                                    ty: attribute_type, ..
                                }) => self.protocol_member_has_disjoint_type_from_ty(
                                    db,
                                    &member,
                                    attribute_type,
                                ),
                                Place::Undefined => self.never(),
                            }
                        })
                })
            }

            (Type::SubclassOf(subclass_of_ty), _) | (_, Type::SubclassOf(subclass_of_ty))
                if subclass_of_ty.is_type_var() =>
            {
                self.always()
            }

            (Type::GenericAlias(left_alias), Type::GenericAlias(right_alias)) => {
                ConstraintSet::from_bool(
                    self.constraints,
                    left_alias.origin(db) != right_alias.origin(db),
                )
                .or(db, self.constraints, || {
                    self.check_specialization_pair(
                        db,
                        left_alias.specialization(db),
                        right_alias.specialization(db),
                    )
                })
            }

            (Type::ClassLiteral(class), Type::GenericAlias(alias_b))
            | (Type::GenericAlias(alias_b), Type::ClassLiteral(class)) => class
                .default_specialization(db)
                .into_generic_alias()
                .when_none_or(db, self.constraints, |alias| {
                    self.check_type_pair(db, Type::GenericAlias(alias_b), Type::GenericAlias(alias))
                }),

            (Type::SubclassOf(subclass_of_ty), Type::ClassLiteral(class_b))
            | (Type::ClassLiteral(class_b), Type::SubclassOf(subclass_of_ty)) => {
                match subclass_of_ty.subclass_of() {
                    SubclassOfInner::Dynamic(_) => self.never(),
                    SubclassOfInner::Class(class_a) => ConstraintSet::from_bool(
                        self.constraints,
                        !class_a.could_exist_in_mro_of(
                            db,
                            ClassType::NonGeneric(class_b),
                            self.constraints,
                        ),
                    ),
                    SubclassOfInner::TypeVar(_) => unreachable!(),
                }
            }

            (Type::SubclassOf(subclass_of_ty), Type::GenericAlias(alias_b))
            | (Type::GenericAlias(alias_b), Type::SubclassOf(subclass_of_ty)) => {
                match subclass_of_ty.subclass_of() {
                    SubclassOfInner::Dynamic(_) => self.never(),
                    SubclassOfInner::Class(class_a) => ConstraintSet::from_bool(
                        self.constraints,
                        !class_a.could_exist_in_mro_of(
                            db,
                            ClassType::Generic(alias_b),
                            self.constraints,
                        ),
                    ),
                    SubclassOfInner::TypeVar(_) => unreachable!(),
                }
            }

            (Type::SubclassOf(left), Type::SubclassOf(right)) => {
                self.check_subclassof_pair(db, left, right)
            }

            // for `type[Any]`/`type[Unknown]`/`type[Todo]`, we know the type cannot be any larger than `type`,
            // so although the type is dynamic we can still determine disjointedness in some situations
            (Type::SubclassOf(subclass_of_ty), other)
            | (other, Type::SubclassOf(subclass_of_ty)) => match subclass_of_ty.subclass_of() {
                SubclassOfInner::Dynamic(_) => {
                    self.check_type_pair(db, KnownClass::Type.to_instance(db), other)
                }
                SubclassOfInner::Class(class) => {
                    self.check_type_pair(db, class.metaclass_instance_type(db), other)
                }
                SubclassOfInner::TypeVar(_) => unreachable!(),
            },

            (Type::SpecialForm(special_form), Type::NominalInstance(instance))
            | (Type::NominalInstance(instance), Type::SpecialForm(special_form)) => {
                ConstraintSet::from_bool(
                    self.constraints,
                    !special_form.is_instance_of(db, instance.class(db)),
                )
            }

            (Type::KnownInstance(known_instance), Type::NominalInstance(instance))
            | (Type::NominalInstance(instance), Type::KnownInstance(known_instance)) => {
                ConstraintSet::from_bool(
                    self.constraints,
                    !known_instance.is_instance_of(db, instance.class(db)),
                )
            }

            (Type::LiteralValue(literal), Type::NominalInstance(instance))
            | (Type::NominalInstance(instance), Type::LiteralValue(literal)) => {
                let positive_relation_holds = match literal.kind() {
                    LiteralValueTypeKind::Int(_) => {
                        KnownClass::Int.when_subclass_of(db, instance.class(db), self.constraints)
                    }
                    LiteralValueTypeKind::Bool(_) => {
                        KnownClass::Bool.when_subclass_of(db, instance.class(db), self.constraints)
                    }
                    LiteralValueTypeKind::LiteralString | LiteralValueTypeKind::String(_) => {
                        KnownClass::Str.when_subclass_of(db, instance.class(db), self.constraints)
                    }
                    LiteralValueTypeKind::Bytes(_) => {
                        KnownClass::Bytes.when_subclass_of(db, instance.class(db), self.constraints)
                    }
                    LiteralValueTypeKind::Enum(enum_literal) => self
                        .as_relation_checker(TypeRelation::Subtyping)
                        .check_type_pair(
                            db,
                            enum_literal.enum_class_instance(db),
                            Type::NominalInstance(instance),
                        ),
                };
                positive_relation_holds.negate(db, self.constraints)
            }

            (Type::TypeIs(_) | Type::TypeGuard(_), Type::NominalInstance(instance))
            | (Type::NominalInstance(instance), Type::TypeIs(_) | Type::TypeGuard(_)) => {
                // A boolean literal must be an instance of exactly `bool`
                // (it cannot be an instance of a `bool` subclass)
                KnownClass::Bool
                    .when_subclass_of(db, instance.class(db), self.constraints)
                    .negate(db, self.constraints)
            }

            (Type::TypeIs(_) | Type::TypeGuard(_), _)
            | (_, Type::TypeIs(_) | Type::TypeGuard(_)) => self.always(),

            (Type::LiteralValue(_), _) | (_, Type::LiteralValue(_)) => self.always(),

            // A class-literal type `X` is always disjoint from an instance type `Y`,
            // unless the type expressing "all instances of `Z`" is a subtype of of `Y`,
            // where `Z` is `X`'s metaclass.
            (Type::ClassLiteral(class), Type::NominalInstance(instance))
            | (Type::NominalInstance(instance), Type::ClassLiteral(class)) => class
                .metaclass_instance_type(db)
                .when_subtype_of(
                    db,
                    Type::NominalInstance(instance),
                    self.constraints,
                    self.inferable,
                )
                .negate(db, self.constraints),

            (Type::GenericAlias(alias), Type::NominalInstance(instance))
            | (Type::NominalInstance(instance), Type::GenericAlias(alias)) => self
                .as_relation_checker(TypeRelation::Subtyping)
                .check_type_pair(
                    db,
                    ClassType::Generic(alias).metaclass_instance_type(db),
                    Type::NominalInstance(instance),
                )
                .negate(db, self.constraints),

            (Type::FunctionLiteral(..), Type::NominalInstance(instance))
            | (Type::NominalInstance(instance), Type::FunctionLiteral(..)) => {
                // A `Type::FunctionLiteral()` must be an instance of exactly `types.FunctionType`
                // (it cannot be an instance of a `types.FunctionType` subclass)
                KnownClass::FunctionType
                    .when_subclass_of(db, instance.class(db), self.constraints)
                    .negate(db, self.constraints)
            }

            // A `BoundMethod` type includes instances of the same method bound to a
            // subtype/subclass of the self type.
            (Type::BoundMethod(a), Type::BoundMethod(b)) => {
                if a.function(db).name(db) != b.function(db).name(db) {
                    // We typically ask about `BoundMethod` disjointness when we're looking at a
                    // method call on an intersection type like `A & B`. In that case, the same
                    // method name would show up on both sides of this check. However for
                    // completeness, if we're ever comparing `BoundMethod` types with different
                    // method names, then they're clearly disjoint.
                    self.always()
                } else if a.function(db) != b.function(db)
                    && a.function(db)
                        .has_known_decorator(db, FunctionDecorators::FINAL)
                    && b.function(db)
                        .has_known_decorator(db, FunctionDecorators::FINAL)
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

            (Type::BoundMethod(_), other) | (other, Type::BoundMethod(_)) => {
                self.check_type_pair(db, KnownClass::MethodType.to_instance(db), other)
            }

            (Type::KnownBoundMethod(method), other) | (other, Type::KnownBoundMethod(method)) => {
                self.check_type_pair(db, method.class().to_instance(db), other)
            }

            (Type::WrapperDescriptor(_), other) | (other, Type::WrapperDescriptor(_)) => {
                self.check_type_pair(db, KnownClass::WrapperDescriptorType.to_instance(db), other)
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
            ) if nominal.class(db).is_final(db) => Type::NominalInstance(nominal)
                .member_lookup_with_policy(
                    db,
                    Name::new_static("__call__"),
                    MemberLookupPolicy::NO_INSTANCE_FALLBACK,
                )
                .place
                .ignore_possibly_undefined()
                .when_none_or(db, self.constraints, |dunder_call| {
                    self.as_relation_checker(TypeRelation::Assignability)
                        .check_type_pair(db, dunder_call, Type::Callable(CallableType::unknown(db)))
                        .negate(db, self.constraints)
                }),

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
                self.check_type_pair(
                    db,
                    Type::NominalInstance(instance),
                    KnownClass::ModuleType.to_instance(db),
                )
            }

            (Type::NominalInstance(left_i), Type::NominalInstance(right_i)) => self
                .with_recursion_guard(left, right, || {
                    self.check_nominal_instance_pair(db, left_i, right_i)
                }),

            (Type::NewTypeInstance(left), Type::NewTypeInstance(right)) => {
                self.check_newtype_pair(db, left, right)
            }
            (Type::NewTypeInstance(newtype), other) | (other, Type::NewTypeInstance(newtype)) => {
                self.check_type_pair(db, newtype.concrete_base_type(db), other)
            }

            (Type::PropertyInstance(_), other) | (other, Type::PropertyInstance(_)) => {
                self.check_type_pair(db, KnownClass::Property.to_instance(db), other)
            }

            (Type::BoundSuper(left), Type::BoundSuper(right)) => self
                .as_equivalence_checker()
                .check_bound_super_pair(db, left, right)
                .negate(db, self.constraints),

            (Type::BoundSuper(_), other) | (other, Type::BoundSuper(_)) => {
                self.check_type_pair(db, KnownClass::Super.to_instance(db), other)
            }

            (Type::GenericAlias(_), _) | (_, Type::GenericAlias(_)) => self.always(),

            (Type::TypedDict(left_td), Type::TypedDict(right_td)) => {
                self.with_recursion_guard(left, right, || {
                    self.check_typeddict_pair(db, left_td, right_td)
                })
            }

            // For any type `T`, if `dict[str, Any]` is not assignable to `T`, then all `TypedDict`
            // types will always be disjoint from `T`. This doesn't cover all cases -- in fact
            // `dict` *itself* is almost always disjoint from `TypedDict` -- but it's a good
            // approximation, and some false negatives are acceptable.
            (Type::TypedDict(_), other) | (other, Type::TypedDict(_)) => {
                let dict_str_any = KnownClass::Dict
                    .to_specialized_instance(db, &[KnownClass::Str.to_instance(db), Type::any()]);

                self.as_relation_checker(TypeRelation::Assignability)
                    .check_type_pair(db, dict_str_any, other)
                    .negate(db, self.constraints)
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
