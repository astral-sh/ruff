use ruff_python_ast::name::Name;

use crate::place::{DefinedPlace, Place};
use crate::types::constraints::{IteratorConstraintsExtension, OptionConstraintsExtension};
use crate::types::enums::is_single_member_enum;
use crate::types::{
    CallableType, ClassType, CycleDetector, DynamicType, KnownClass, KnownInstanceType,
    MemberLookupPolicy, PairVisitor, ProtocolInstanceType, SubclassOfInner,
    TypeVarBoundOrConstraints,
};
use crate::{
    Db,
    types::{Type, constraints::ConstraintSet, generics::InferableTypeVars},
};

/// A non-exhaustive enumeration of relations that can exist between types.
#[derive(Debug, Copy, Clone, Hash, PartialEq, Eq)]
pub(crate) enum TypeRelation<'db> {
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
    /// The redundancy relation dictates whether the union `A | B` can be safely simplified
    /// to the type `A` without downstream consequences on ty's inference of types elsewhere.
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
    /// Practically speaking, in most respects the redundancy relation is the same as the subtyping
    /// relation. It is redundant to add `bool` to a union that includes `int`, because `bool` is a
    /// subtype of `int`, so inference of attribute access or binary expressions on the union
    /// `int | bool` would always produce a type that represents the same set of possible sets of
    /// runtime values as if ty had inferred the attribute access or binary expression on `int`
    /// alone.
    ///
    /// Where the redundancy relation differs from the subtyping relation is that there are a
    /// number of simplifications that can be made when simplifying unions that are not
    /// strictly permitted by the subtyping relation. For example, it is safe to avoid adding
    /// `Any` to a union that already includes `Any`, because `Any` already represents an
    /// unknown set of possible sets of runtime values that can materialize to any type in a
    /// gradual, permissive way. Inferring attribute access or binary expressions over
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
    Redundancy,

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
    SubtypingAssuming(ConstraintSet<'db>),

    /// A placeholder for the new assignability relation that uses constraint sets to encode
    /// relationships with a typevar. This will eventually replace `Assignability`, but allows us
    /// to start using the new relation in a controlled manner in some places.
    ConstraintSetAssignability,
}

impl TypeRelation<'_> {
    pub(crate) const fn is_assignability(self) -> bool {
        matches!(self, TypeRelation::Assignability)
    }

    pub(crate) const fn is_constraint_set_assignability(self) -> bool {
        matches!(self, TypeRelation::ConstraintSetAssignability)
    }

    pub(crate) const fn is_subtyping(self) -> bool {
        matches!(self, TypeRelation::Subtyping)
    }
}

#[salsa::tracked]
impl<'db> Type<'db> {
    /// Return true if this type is a subtype of type `target`.
    ///
    /// See [`TypeRelation::Subtyping`] for more details.
    pub(crate) fn is_subtype_of(self, db: &'db dyn Db, target: Type<'db>) -> bool {
        self.when_subtype_of(db, target, InferableTypeVars::None)
            .is_always_satisfied(db)
    }

    pub(super) fn when_subtype_of(
        self,
        db: &'db dyn Db,
        target: Type<'db>,
        inferable: InferableTypeVars<'_, 'db>,
    ) -> ConstraintSet<'db> {
        self.has_relation_to(db, target, inferable, TypeRelation::Subtyping)
    }

    /// Return the constraints under which this type is a subtype of type `target`, assuming that
    /// all of the restrictions in `constraints` hold.
    ///
    /// See [`TypeRelation::SubtypingAssuming`] for more details.
    pub(super) fn when_subtype_of_assuming(
        self,
        db: &'db dyn Db,
        target: Type<'db>,
        assuming: ConstraintSet<'db>,
        inferable: InferableTypeVars<'_, 'db>,
    ) -> ConstraintSet<'db> {
        self.has_relation_to(
            db,
            target,
            inferable,
            TypeRelation::SubtypingAssuming(assuming),
        )
    }

    /// Return true if this type is assignable to type `target`.
    ///
    /// See `TypeRelation::Assignability` for more details.
    pub fn is_assignable_to(self, db: &'db dyn Db, target: Type<'db>) -> bool {
        self.when_assignable_to(db, target, InferableTypeVars::None)
            .is_always_satisfied(db)
    }

    /// Return true if this type is assignable to type `target` using constraint-set assignability.
    ///
    /// This uses `TypeRelation::ConstraintSetAssignability`, which encodes typevar relations into
    /// a constraint set and lets `satisfied_by_all_typevars` perform existential vs universal
    /// reasoning depending on inferable typevars.
    pub fn is_constraint_set_assignable_to(self, db: &'db dyn Db, target: Type<'db>) -> bool {
        self.when_constraint_set_assignable_to(db, target, InferableTypeVars::None)
            .is_always_satisfied(db)
    }

    pub(super) fn when_assignable_to(
        self,
        db: &'db dyn Db,
        target: Type<'db>,
        inferable: InferableTypeVars<'_, 'db>,
    ) -> ConstraintSet<'db> {
        self.has_relation_to(db, target, inferable, TypeRelation::Assignability)
    }

    pub(super) fn when_constraint_set_assignable_to(
        self,
        db: &'db dyn Db,
        target: Type<'db>,
        inferable: InferableTypeVars<'_, 'db>,
    ) -> ConstraintSet<'db> {
        self.has_relation_to(
            db,
            target,
            inferable,
            TypeRelation::ConstraintSetAssignability,
        )
    }

    /// Return `true` if it would be redundant to add `self` to a union that already contains `other`.
    ///
    /// See [`TypeRelation::Redundancy`] for more details.
    pub(super) fn is_redundant_with(self, db: &'db dyn Db, other: Type<'db>) -> bool {
        #[salsa::tracked(cycle_initial=is_redundant_with_cycle_initial, heap_size=ruff_memory_usage::heap_size)]
        fn is_redundant_with_impl<'db>(
            db: &'db dyn Db,
            self_ty: Type<'db>,
            other: Type<'db>,
        ) -> bool {
            self_ty
                .has_relation_to(db, other, InferableTypeVars::None, TypeRelation::Redundancy)
                .is_always_satisfied(db)
        }

        if self == other {
            return true;
        }

        is_redundant_with_impl(db, self, other)
    }

    fn has_relation_to(
        self,
        db: &'db dyn Db,
        target: Type<'db>,
        inferable: InferableTypeVars<'_, 'db>,
        relation: TypeRelation<'db>,
    ) -> ConstraintSet<'db> {
        self.has_relation_to_impl(
            db,
            target,
            inferable,
            relation,
            &HasRelationToVisitor::default(),
            &IsDisjointVisitor::default(),
        )
    }

    pub(super) fn has_relation_to_impl(
        self,
        db: &'db dyn Db,
        target: Type<'db>,
        inferable: InferableTypeVars<'_, 'db>,
        relation: TypeRelation<'db>,
        relation_visitor: &HasRelationToVisitor<'db>,
        disjointness_visitor: &IsDisjointVisitor<'db>,
    ) -> ConstraintSet<'db> {
        // Subtyping implies assignability, so if subtyping is reflexive and the two types are
        // equal, it is both a subtype and assignable. Assignability is always reflexive.
        //
        // Note that we could do a full equivalence check here, but that would be both expensive
        // and unnecessary. This early return is only an optimisation.
        if (!relation.is_subtyping() || self.subtyping_is_always_reflexive()) && self == target {
            return ConstraintSet::from(true);
        }

        // Handle constraint implication first. If either `self` or `target` is a typevar, check
        // the constraint set to see if the corresponding constraint is satisfied.
        if let TypeRelation::SubtypingAssuming(constraints) = relation
            && (self.is_type_var() || target.is_type_var())
        {
            return constraints.implies_subtype_of(db, self, target);
        }

        // Handle the new constraint-set-based assignability relation next. Comparisons with a
        // typevar are translated directly into a constraint set.
        if relation.is_constraint_set_assignability() {
            // A typevar satisfies a relation when...it satisfies the relation. Yes that's a
            // tautology! We're moving the caller's subtyping/assignability requirement into a
            // constraint set. If the typevar has an upper bound or constraints, then the relation
            // only has to hold when the typevar has a valid specialization (i.e., one that
            // satisfies the upper bound/constraints).
            if let Type::TypeVar(bound_typevar) = self {
                return ConstraintSet::constrain_typevar(db, bound_typevar, Type::Never, target);
            } else if let Type::TypeVar(bound_typevar) = target {
                return ConstraintSet::constrain_typevar(db, bound_typevar, self, Type::object());
            }
        }

        match (self, target) {
            // Everything is a subtype of `object`.
            (_, Type::NominalInstance(instance)) if instance.is_object() => {
                ConstraintSet::from(true)
            }
            (_, Type::ProtocolInstance(target)) if target.is_equivalent_to_object(db) => {
                ConstraintSet::from(true)
            }

            // `Never` is the bottom type, the empty set.
            // It is a subtype of all other types.
            (Type::Never, _) => ConstraintSet::from(true),

            // In some specific situations, `Any`/`Unknown`/`@Todo` can be simplified out of unions and intersections,
            // but this is not true for divergent types (and moving this case any lower down appears to cause
            // "too many cycle iterations" panics).
            (Type::Dynamic(DynamicType::Divergent(_)), _)
            | (_, Type::Dynamic(DynamicType::Divergent(_))) => {
                ConstraintSet::from(relation.is_assignability())
            }

            (Type::TypeAlias(self_alias), _) => {
                relation_visitor.visit((self, target, relation), || {
                    self_alias.value_type(db).has_relation_to_impl(
                        db,
                        target,
                        inferable,
                        relation,
                        relation_visitor,
                        disjointness_visitor,
                    )
                })
            }

            (_, Type::TypeAlias(target_alias)) => {
                relation_visitor.visit((self, target, relation), || {
                    self.has_relation_to_impl(
                        db,
                        target_alias.value_type(db),
                        inferable,
                        relation,
                        relation_visitor,
                        disjointness_visitor,
                    )
                })
            }

            // Pretend that instances of `dataclasses.Field` are assignable to their default type.
            // This allows field definitions like `name: str = field(default="")` in dataclasses
            // to pass the assignability check of the inferred type to the declared type.
            (Type::KnownInstance(KnownInstanceType::Field(field)), right)
                if relation.is_assignability() =>
            {
                field.default_type(db).when_none_or(|default_type| {
                    default_type.has_relation_to_impl(
                        db,
                        right,
                        inferable,
                        relation,
                        relation_visitor,
                        disjointness_visitor,
                    )
                })
            }

            // Dynamic is only a subtype of `object` and only a supertype of `Never`; both were
            // handled above. It's always assignable, though.
            //
            // Union simplification sits in between subtyping and assignability. `Any <: T` only
            // holds true if `T` is also a dynamic type or a union that contains a dynamic type.
            // Similarly, `T <: Any` only holds true if `T` is a dynamic type or an intersection
            // that contains a dynamic type.
            (Type::Dynamic(dynamic), _) => {
                // If a `Divergent` type is involved, it must not be eliminated.
                debug_assert!(
                    !matches!(dynamic, DynamicType::Divergent(_)),
                    "DynamicType::Divergent should have been handled in an earlier branch"
                );
                ConstraintSet::from(match relation {
                    TypeRelation::Subtyping | TypeRelation::SubtypingAssuming(_) => false,
                    TypeRelation::Assignability | TypeRelation::ConstraintSetAssignability => true,
                    TypeRelation::Redundancy => match target {
                        Type::Dynamic(_) => true,
                        Type::Union(union) => union.elements(db).iter().any(Type::is_dynamic),
                        _ => false,
                    },
                })
            }
            (_, Type::Dynamic(_)) => ConstraintSet::from(match relation {
                TypeRelation::Subtyping | TypeRelation::SubtypingAssuming(_) => false,
                TypeRelation::Assignability | TypeRelation::ConstraintSetAssignability => true,
                TypeRelation::Redundancy => match self {
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
            }),

            // In general, a TypeVar `T` is not a subtype of a type `S` unless one of the two conditions is satisfied:
            // 1. `T` is a bound TypeVar and `T`'s upper bound is a subtype of `S`.
            //    TypeVars without an explicit upper bound are treated as having an implicit upper bound of `object`.
            // 2. `T` is a constrained TypeVar and all of `T`'s constraints are subtypes of `S`.
            //
            // However, there is one exception to this general rule: for any given typevar `T`,
            // `T` will always be a subtype of any union containing `T`.
            (Type::TypeVar(bound_typevar), Type::Union(union))
                if !bound_typevar.is_inferable(db, inferable)
                    && union.elements(db).contains(&self) =>
            {
                ConstraintSet::from(true)
            }

            // A similar rule applies in reverse to intersection types.
            (Type::Intersection(intersection), Type::TypeVar(bound_typevar))
                if !bound_typevar.is_inferable(db, inferable)
                    && intersection.positive(db).contains(&target) =>
            {
                ConstraintSet::from(true)
            }
            (Type::Intersection(intersection), Type::TypeVar(bound_typevar))
                if !bound_typevar.is_inferable(db, inferable)
                    && intersection.negative(db).contains(&target) =>
            {
                ConstraintSet::from(false)
            }

            // Two identical typevars must always solve to the same type, so they are always
            // subtypes of each other and assignable to each other.
            //
            // Note that this is not handled by the early return at the beginning of this method,
            // since subtyping between a TypeVar and an arbitrary other type cannot be guaranteed to be reflexive.
            (Type::TypeVar(lhs_bound_typevar), Type::TypeVar(rhs_bound_typevar))
                if !lhs_bound_typevar.is_inferable(db, inferable)
                    && lhs_bound_typevar.is_same_typevar_as(db, rhs_bound_typevar) =>
            {
                ConstraintSet::from(true)
            }

            // `type[T]` is a subtype of the class object `A` if every instance of `T` is a subtype of an instance
            // of `A`, and vice versa.
            (Type::SubclassOf(subclass_of), _)
                if !subclass_of
                    .into_type_var()
                    .zip(target.to_instance(db))
                    .when_some_and(|(this_instance, other_instance)| {
                        Type::TypeVar(this_instance).has_relation_to_impl(
                            db,
                            other_instance,
                            inferable,
                            relation,
                            relation_visitor,
                            disjointness_visitor,
                        )
                    })
                    .is_never_satisfied(db) =>
            {
                // TODO: The repetition here isn't great, but we need the fallthrough logic.
                subclass_of
                    .into_type_var()
                    .zip(target.to_instance(db))
                    .when_some_and(|(this_instance, other_instance)| {
                        Type::TypeVar(this_instance).has_relation_to_impl(
                            db,
                            other_instance,
                            inferable,
                            relation,
                            relation_visitor,
                            disjointness_visitor,
                        )
                    })
            }

            (_, Type::SubclassOf(subclass_of))
                if !subclass_of
                    .into_type_var()
                    .zip(self.to_instance(db))
                    .when_some_and(|(other_instance, this_instance)| {
                        this_instance.has_relation_to_impl(
                            db,
                            Type::TypeVar(other_instance),
                            inferable,
                            relation,
                            relation_visitor,
                            disjointness_visitor,
                        )
                    })
                    .is_never_satisfied(db) =>
            {
                // TODO: The repetition here isn't great, but we need the fallthrough logic.
                subclass_of
                    .into_type_var()
                    .zip(self.to_instance(db))
                    .when_some_and(|(other_instance, this_instance)| {
                        this_instance.has_relation_to_impl(
                            db,
                            Type::TypeVar(other_instance),
                            inferable,
                            relation,
                            relation_visitor,
                            disjointness_visitor,
                        )
                    })
            }

            // A fully static typevar is a subtype of its upper bound, and to something similar to
            // the union of its constraints. An unbound, unconstrained, fully static typevar has an
            // implicit upper bound of `object` (which is handled above).
            (Type::TypeVar(bound_typevar), _)
                if !bound_typevar.is_inferable(db, inferable)
                    && bound_typevar.typevar(db).bound_or_constraints(db).is_some() =>
            {
                match bound_typevar.typevar(db).bound_or_constraints(db) {
                    None => unreachable!(),
                    Some(TypeVarBoundOrConstraints::UpperBound(bound)) => bound
                        .has_relation_to_impl(
                            db,
                            target,
                            inferable,
                            relation,
                            relation_visitor,
                            disjointness_visitor,
                        ),
                    Some(TypeVarBoundOrConstraints::Constraints(constraints)) => {
                        constraints.elements(db).iter().when_all(db, |constraint| {
                            constraint.has_relation_to_impl(
                                db,
                                target,
                                inferable,
                                relation,
                                relation_visitor,
                                disjointness_visitor,
                            )
                        })
                    }
                }
            }

            // If the typevar is constrained, there must be multiple constraints, and the typevar
            // might be specialized to any one of them. However, the constraints do not have to be
            // disjoint, which means an lhs type might be a subtype of all of the constraints.
            (_, Type::TypeVar(bound_typevar))
                if !bound_typevar.is_inferable(db, inferable)
                    && !bound_typevar
                        .typevar(db)
                        .constraints(db)
                        .when_some_and(|constraints| {
                            constraints.iter().when_all(db, |constraint| {
                                self.has_relation_to_impl(
                                    db,
                                    *constraint,
                                    inferable,
                                    relation,
                                    relation_visitor,
                                    disjointness_visitor,
                                )
                            })
                        })
                        .is_never_satisfied(db) =>
            {
                // TODO: The repetition here isn't great, but we really need the fallthrough logic,
                // where this arm only engages if it returns true (or in the world of constraints,
                // not false). Once we're using real constraint sets instead of bool, we should be
                // able to simplify the typevar logic.
                bound_typevar
                    .typevar(db)
                    .constraints(db)
                    .when_some_and(|constraints| {
                        constraints.iter().when_all(db, |constraint| {
                            self.has_relation_to_impl(
                                db,
                                *constraint,
                                inferable,
                                relation,
                                relation_visitor,
                                disjointness_visitor,
                            )
                        })
                    })
            }

            (Type::TypeVar(bound_typevar), _) if bound_typevar.is_inferable(db, inferable) => {
                // The implicit lower bound of a typevar is `Never`, which means
                // that it is always assignable to any other type.

                // TODO: record the unification constraints

                ConstraintSet::from(true)
            }

            // `Never` is the bottom type, the empty set.
            (_, Type::Never) => ConstraintSet::from(false),

            (Type::NewTypeInstance(self_newtype), Type::NewTypeInstance(target_newtype)) => {
                self_newtype.has_relation_to_impl(db, target_newtype)
            }
            // In the special cases of `NewType`s of `float` or `complex`, the concrete base type
            // can be a union (`int | float` or `int | float | complex`). For that reason,
            // `NewType` assignability to a union needs to consider two different cases. It could
            // be that we need to treat the `NewType` as the underlying union it's assignable to,
            // for example:
            //
            // ```py
            // Foo = NewType("Foo", float)
            // static_assert(is_assignable_to(Foo, float | None))
            // ```
            //
            // The right side there is equivalent to `int | float | None`, but `Foo` as a whole
            // isn't assignable to any of those three types. However, `Foo`s concrete base type is
            // `int | float`, which is assignable, because union members on the left side get
            // checked individually. On the other hand, we need to be careful not to break the
            // following case, where `int | float` is *not* assignable to the right side:
            //
            // ```py
            // static_assert(is_assignable_to(Foo, Foo | None))
            // ```
            //
            // To handle both cases, we have to check that *either* `Foo` as a whole is assignable
            // (or subtypeable etc.) *or* that its concrete base type is. Note that this match arm
            // needs to take precedence over the `Type::Union` arms immediately below.
            (Type::NewTypeInstance(self_newtype), Type::Union(union)) => {
                // First the normal "assign to union" case, unfortunately duplicated from below.
                union
                    .elements(db)
                    .iter()
                    .when_any(db, |&elem_ty| {
                        self.has_relation_to_impl(
                            db,
                            elem_ty,
                            inferable,
                            relation,
                            relation_visitor,
                            disjointness_visitor,
                        )
                    })
                    // Failing that, if the concrete base type is a union, try delegating to that.
                    // Otherwise, this would be equivalent to what we just checked, and we
                    // shouldn't waste time checking it twice.
                    .or(db, || {
                        let concrete_base = self_newtype.concrete_base_type(db);
                        if matches!(concrete_base, Type::Union(_)) {
                            concrete_base.has_relation_to_impl(
                                db,
                                target,
                                inferable,
                                relation,
                                relation_visitor,
                                disjointness_visitor,
                            )
                        } else {
                            ConstraintSet::from(false)
                        }
                    })
            }
            // All other `NewType` assignments fall back to the concrete base type.
            (Type::NewTypeInstance(self_newtype), _) => {
                self_newtype.concrete_base_type(db).has_relation_to_impl(
                    db,
                    target,
                    inferable,
                    relation,
                    relation_visitor,
                    disjointness_visitor,
                )
            }

            (Type::Union(union), _) => union.elements(db).iter().when_all(db, |&elem_ty| {
                elem_ty.has_relation_to_impl(
                    db,
                    target,
                    inferable,
                    relation,
                    relation_visitor,
                    disjointness_visitor,
                )
            }),

            (_, Type::Union(union)) => union.elements(db).iter().when_any(db, |&elem_ty| {
                self.has_relation_to_impl(
                    db,
                    elem_ty,
                    inferable,
                    relation,
                    relation_visitor,
                    disjointness_visitor,
                )
            }),

            // If both sides are intersections we need to handle the right side first
            // (A & B & C) is a subtype of (A & B) because the left is a subtype of both A and B,
            // but none of A, B, or C is a subtype of (A & B).
            (_, Type::Intersection(intersection)) => intersection
                .positive(db)
                .iter()
                .when_all(db, |&pos_ty| {
                    self.has_relation_to_impl(
                        db,
                        pos_ty,
                        inferable,
                        relation,
                        relation_visitor,
                        disjointness_visitor,
                    )
                })
                .and(db, || {
                    // For subtyping, we would want to check whether the *top materialization* of `self`
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
                    let self_ty = match relation {
                        TypeRelation::Subtyping
                        | TypeRelation::Redundancy
                        | TypeRelation::SubtypingAssuming(_) => self,
                        TypeRelation::Assignability | TypeRelation::ConstraintSetAssignability => {
                            self.bottom_materialization(db)
                        }
                    };
                    intersection.negative(db).iter().when_all(db, |&neg_ty| {
                        let neg_ty = match relation {
                            TypeRelation::Subtyping
                            | TypeRelation::Redundancy
                            | TypeRelation::SubtypingAssuming(_) => neg_ty,
                            TypeRelation::Assignability
                            | TypeRelation::ConstraintSetAssignability => {
                                neg_ty.bottom_materialization(db)
                            }
                        };
                        self_ty.is_disjoint_from_impl(
                            db,
                            neg_ty,
                            inferable,
                            disjointness_visitor,
                            relation_visitor,
                        )
                    })
                }),

            (Type::Intersection(intersection), _) => {
                intersection.positive(db).iter().when_any(db, |&elem_ty| {
                    elem_ty.has_relation_to_impl(
                        db,
                        target,
                        inferable,
                        relation,
                        relation_visitor,
                        disjointness_visitor,
                    )
                })
            }

            // Other than the special cases checked above, no other types are a subtype of a
            // typevar, since there's no guarantee what type the typevar will be specialized to.
            // (If the typevar is bounded, it might be specialized to a smaller type than the
            // bound. This is true even if the bound is a final class, since the typevar can still
            // be specialized to `Never`.)
            (_, Type::TypeVar(bound_typevar)) if !bound_typevar.is_inferable(db, inferable) => {
                ConstraintSet::from(false)
            }

            (_, Type::TypeVar(typevar))
                if typevar.is_inferable(db, inferable)
                    && relation.is_assignability()
                    && typevar.typevar(db).upper_bound(db).is_none_or(|bound| {
                        !self
                            .has_relation_to_impl(
                                db,
                                bound,
                                inferable,
                                relation,
                                relation_visitor,
                                disjointness_visitor,
                            )
                            .is_never_satisfied(db)
                    }) =>
            {
                // TODO: record the unification constraints

                typevar.typevar(db).upper_bound(db).when_none_or(|bound| {
                    self.has_relation_to_impl(
                        db,
                        bound,
                        inferable,
                        relation,
                        relation_visitor,
                        disjointness_visitor,
                    )
                })
            }

            // TODO: Infer specializations here
            (_, Type::TypeVar(bound_typevar)) if bound_typevar.is_inferable(db, inferable) => {
                ConstraintSet::from(false)
            }
            (Type::TypeVar(bound_typevar), _) => {
                // All inferable cases should have been handled above
                assert!(!bound_typevar.is_inferable(db, inferable));
                ConstraintSet::from(false)
            }

            // Note that the definition of `Type::AlwaysFalsy` depends on the return value of `__bool__`.
            // If `__bool__` always returns True or False, it can be treated as a subtype of `AlwaysTruthy` or `AlwaysFalsy`, respectively.
            (left, Type::AlwaysFalsy) => ConstraintSet::from(left.bool(db).is_always_false()),
            (left, Type::AlwaysTruthy) => ConstraintSet::from(left.bool(db).is_always_true()),
            // Currently, the only supertype of `AlwaysFalsy` and `AlwaysTruthy` is the universal set (object instance).
            (Type::AlwaysFalsy | Type::AlwaysTruthy, _) => {
                target.when_equivalent_to(db, Type::object(), inferable)
            }

            // These clauses handle type variants that include function literals. A function
            // literal is the subtype of itself, and not of any other function literal. However,
            // our representation of a function literal includes any specialization that should be
            // applied to the signature. Different specializations of the same function literal are
            // only subtypes of each other if they result in the same signature.
            (Type::FunctionLiteral(self_function), Type::FunctionLiteral(target_function)) => {
                self_function.has_relation_to_impl(
                    db,
                    target_function,
                    inferable,
                    relation,
                    relation_visitor,
                    disjointness_visitor,
                )
            }
            (Type::BoundMethod(self_method), Type::BoundMethod(target_method)) => self_method
                .has_relation_to_impl(
                    db,
                    target_method,
                    inferable,
                    relation,
                    relation_visitor,
                    disjointness_visitor,
                ),
            (Type::KnownBoundMethod(self_method), Type::KnownBoundMethod(target_method)) => {
                self_method.has_relation_to_impl(
                    db,
                    target_method,
                    inferable,
                    relation,
                    relation_visitor,
                    disjointness_visitor,
                )
            }

            // No literal type is a subtype of any other literal type, unless they are the same
            // type (which is handled above). This case is not necessary from a correctness
            // perspective (the fallback cases below will handle it correctly), but it is important
            // for performance of simplifying large unions of literal types.
            (
                Type::StringLiteral(_)
                | Type::IntLiteral(_)
                | Type::BytesLiteral(_)
                | Type::ClassLiteral(_)
                | Type::FunctionLiteral(_)
                | Type::ModuleLiteral(_)
                | Type::EnumLiteral(_),
                Type::StringLiteral(_)
                | Type::IntLiteral(_)
                | Type::BytesLiteral(_)
                | Type::ClassLiteral(_)
                | Type::FunctionLiteral(_)
                | Type::ModuleLiteral(_)
                | Type::EnumLiteral(_),
            ) => ConstraintSet::from(false),

            (Type::Callable(self_callable), Type::Callable(other_callable)) => relation_visitor
                .visit((self, target, relation), || {
                    self_callable.has_relation_to_impl(
                        db,
                        other_callable,
                        inferable,
                        relation,
                        relation_visitor,
                        disjointness_visitor,
                    )
                }),

            (_, Type::Callable(other_callable)) => {
                relation_visitor.visit((self, target, relation), || {
                    self.try_upcast_to_callable(db).when_some_and(|callables| {
                        callables.has_relation_to_impl(
                            db,
                            other_callable,
                            inferable,
                            relation,
                            relation_visitor,
                            disjointness_visitor,
                        )
                    })
                })
            }

            // `type[Any]` is assignable to arbitrary protocols as it has arbitrary attributes
            // (this is handled by a lower-down branch), but it is only a subtype of a given
            // protocol if `type` is a subtype of that protocol. Similarly, `type[T]` will
            // always be assignable to any protocol if `type[<upper bound of T>]` is assignable
            // to that protocol (handled lower down), but it is only a subtype of that protocol
            // if `type` is a subtype of that protocol.
            (Type::SubclassOf(self_subclass_ty), Type::ProtocolInstance(_))
                if (self_subclass_ty.is_dynamic() || self_subclass_ty.is_type_var())
                    && !relation.is_assignability() =>
            {
                KnownClass::Type.to_instance(db).has_relation_to_impl(
                    db,
                    target,
                    inferable,
                    relation,
                    relation_visitor,
                    disjointness_visitor,
                )
            }

            (_, Type::ProtocolInstance(protocol)) => {
                relation_visitor.visit((self, target, relation), || {
                    self.satisfies_protocol(
                        db,
                        protocol,
                        inferable,
                        relation,
                        relation_visitor,
                        disjointness_visitor,
                    )
                })
            }

            // A protocol instance can never be a subtype of a nominal type, with the *sole* exception of `object`.
            (Type::ProtocolInstance(_), _) => ConstraintSet::from(false),

            (Type::TypedDict(self_typeddict), Type::TypedDict(other_typeddict)) => relation_visitor
                .visit((self, target, relation), || {
                    self_typeddict.has_relation_to_impl(
                        db,
                        other_typeddict,
                        inferable,
                        relation,
                        relation_visitor,
                        disjointness_visitor,
                    )
                }),

            // TODO: When we support `closed` and/or `extra_items`, we could allow assignments to other
            // compatible `Mapping`s. `extra_items` could also allow for some assignments to `dict`, as
            // long as `total=False`. (But then again, does anyone want a non-total `TypedDict` where all
            // key types are a supertype of the extra items type?)
            (Type::TypedDict(_), _) => relation_visitor.visit((self, target, relation), || {
                KnownClass::Mapping
                    .to_specialized_instance(db, [KnownClass::Str.to_instance(db), Type::object()])
                    .has_relation_to_impl(
                        db,
                        target,
                        inferable,
                        relation,
                        relation_visitor,
                        disjointness_visitor,
                    )
            }),

            // A non-`TypedDict` cannot subtype a `TypedDict`
            (_, Type::TypedDict(_)) => ConstraintSet::from(false),

            // All `StringLiteral` types are a subtype of `LiteralString`.
            (Type::StringLiteral(_), Type::LiteralString) => ConstraintSet::from(true),

            // An instance is a subtype of an enum literal, if it is an instance of the enum class
            // and the enum has only one member.
            (Type::NominalInstance(_), Type::EnumLiteral(target_enum_literal)) => {
                if target_enum_literal.enum_class_instance(db) != self {
                    return ConstraintSet::from(false);
                }

                ConstraintSet::from(is_single_member_enum(
                    db,
                    target_enum_literal.enum_class(db),
                ))
            }

            // Except for the special `LiteralString` case above,
            // most `Literal` types delegate to their instance fallbacks
            // unless `self` is exactly equivalent to `target` (handled above)
            (
                Type::StringLiteral(_)
                | Type::LiteralString
                | Type::BooleanLiteral(_)
                | Type::IntLiteral(_)
                | Type::BytesLiteral(_)
                | Type::ModuleLiteral(_)
                | Type::EnumLiteral(_)
                | Type::FunctionLiteral(_),
                _,
            ) => (self.literal_fallback_instance(db)).when_some_and(|instance| {
                instance.has_relation_to_impl(
                    db,
                    target,
                    inferable,
                    relation,
                    relation_visitor,
                    disjointness_visitor,
                )
            }),

            // The same reasoning applies for these special callable types:
            (Type::BoundMethod(_), _) => {
                KnownClass::MethodType.to_instance(db).has_relation_to_impl(
                    db,
                    target,
                    inferable,
                    relation,
                    relation_visitor,
                    disjointness_visitor,
                )
            }
            (Type::KnownBoundMethod(method), _) => {
                method.class().to_instance(db).has_relation_to_impl(
                    db,
                    target,
                    inferable,
                    relation,
                    relation_visitor,
                    disjointness_visitor,
                )
            }
            (Type::WrapperDescriptor(_), _) => KnownClass::WrapperDescriptorType
                .to_instance(db)
                .has_relation_to_impl(
                    db,
                    target,
                    inferable,
                    relation,
                    relation_visitor,
                    disjointness_visitor,
                ),

            (Type::DataclassDecorator(_) | Type::DataclassTransformer(_), _) => {
                // TODO: Implement subtyping using an equivalent `Callable` type.
                ConstraintSet::from(false)
            }

            // `TypeIs` is invariant.
            (Type::TypeIs(left), Type::TypeIs(right)) => left
                .return_type(db)
                .has_relation_to_impl(
                    db,
                    right.return_type(db),
                    inferable,
                    relation,
                    relation_visitor,
                    disjointness_visitor,
                )
                .and(db, || {
                    right.return_type(db).has_relation_to_impl(
                        db,
                        left.return_type(db),
                        inferable,
                        relation,
                        relation_visitor,
                        disjointness_visitor,
                    )
                }),

            // `TypeGuard` is covariant.
            (Type::TypeGuard(left), Type::TypeGuard(right)) => {
                left.return_type(db).has_relation_to_impl(
                    db,
                    right.return_type(db),
                    inferable,
                    relation,
                    relation_visitor,
                    disjointness_visitor,
                )
            }

            // `TypeIs[T]` and `TypeGuard[T]` are subtypes of `bool`.
            (Type::TypeIs(_) | Type::TypeGuard(_), _) => {
                KnownClass::Bool.to_instance(db).has_relation_to_impl(
                    db,
                    target,
                    inferable,
                    relation,
                    relation_visitor,
                    disjointness_visitor,
                )
            }

            // Function-like callables are subtypes of `FunctionType`
            (Type::Callable(callable), _) if callable.is_function_like(db) => {
                KnownClass::FunctionType
                    .to_instance(db)
                    .has_relation_to_impl(
                        db,
                        target,
                        inferable,
                        relation,
                        relation_visitor,
                        disjointness_visitor,
                    )
            }

            (Type::Callable(_), _) => ConstraintSet::from(false),

            (Type::BoundSuper(_), Type::BoundSuper(_)) => {
                self.when_equivalent_to(db, target, inferable)
            }
            (Type::BoundSuper(_), _) => KnownClass::Super.to_instance(db).has_relation_to_impl(
                db,
                target,
                inferable,
                relation,
                relation_visitor,
                disjointness_visitor,
            ),

            (Type::SubclassOf(subclass_of), _) | (_, Type::SubclassOf(subclass_of))
                if subclass_of.is_type_var() =>
            {
                ConstraintSet::from(false)
            }

            // `Literal[<class 'C'>]` is a subtype of `type[B]` if `C` is a subclass of `B`,
            // since `type[B]` describes all possible runtime subclasses of the class object `B`.
            (Type::ClassLiteral(class), Type::SubclassOf(target_subclass_ty)) => target_subclass_ty
                .subclass_of()
                .into_class(db)
                .map(|subclass_of_class| {
                    class.default_specialization(db).has_relation_to_impl(
                        db,
                        subclass_of_class,
                        inferable,
                        relation,
                        relation_visitor,
                        disjointness_visitor,
                    )
                })
                .unwrap_or_else(|| ConstraintSet::from(relation.is_assignability())),

            // Similarly, `<class 'C'>` is assignable to `<class 'C[...]'>` (a generic-alias type)
            // if the default specialization of `C` is assignable to `C[...]`. This scenario occurs
            // with final generic types, where `type[C[...]]` is simplified to the generic-alias
            // type `<class 'C[...]'>`, due to the fact that `C[...]` has no subclasses.
            (Type::ClassLiteral(class), Type::GenericAlias(target_alias)) => {
                class.default_specialization(db).has_relation_to_impl(
                    db,
                    ClassType::Generic(target_alias),
                    inferable,
                    relation,
                    relation_visitor,
                    disjointness_visitor,
                )
            }

            // For generic aliases, we delegate to the underlying class type.
            (Type::GenericAlias(self_alias), Type::GenericAlias(target_alias)) => {
                ClassType::Generic(self_alias).has_relation_to_impl(
                    db,
                    ClassType::Generic(target_alias),
                    inferable,
                    relation,
                    relation_visitor,
                    disjointness_visitor,
                )
            }

            (Type::GenericAlias(alias), Type::SubclassOf(target_subclass_ty)) => target_subclass_ty
                .subclass_of()
                .into_class(db)
                .map(|subclass_of_class| {
                    ClassType::Generic(alias).has_relation_to_impl(
                        db,
                        subclass_of_class,
                        inferable,
                        relation,
                        relation_visitor,
                        disjointness_visitor,
                    )
                })
                .unwrap_or_else(|| ConstraintSet::from(relation.is_assignability())),

            // This branch asks: given two types `type[T]` and `type[S]`, is `type[T]` a subtype of `type[S]`?
            (Type::SubclassOf(self_subclass_ty), Type::SubclassOf(target_subclass_ty)) => {
                self_subclass_ty.has_relation_to_impl(
                    db,
                    target_subclass_ty,
                    inferable,
                    relation,
                    relation_visitor,
                    disjointness_visitor,
                )
            }

            // `Literal[str]` is a subtype of `type` because the `str` class object is an instance of its metaclass `type`.
            // `Literal[abc.ABC]` is a subtype of `abc.ABCMeta` because the `abc.ABC` class object
            // is an instance of its metaclass `abc.ABCMeta`.
            (Type::ClassLiteral(class), _) => {
                class.metaclass_instance_type(db).has_relation_to_impl(
                    db,
                    target,
                    inferable,
                    relation,
                    relation_visitor,
                    disjointness_visitor,
                )
            }
            (Type::GenericAlias(alias), _) => ClassType::from(alias)
                .metaclass_instance_type(db)
                .has_relation_to_impl(
                    db,
                    target,
                    inferable,
                    relation,
                    relation_visitor,
                    disjointness_visitor,
                ),

            // `type[Any]` is a subtype of `type[object]`, and is assignable to any `type[...]`
            (Type::SubclassOf(subclass_of_ty), other) if subclass_of_ty.is_dynamic() => {
                KnownClass::Type
                    .to_instance(db)
                    .has_relation_to_impl(
                        db,
                        other,
                        inferable,
                        relation,
                        relation_visitor,
                        disjointness_visitor,
                    )
                    .or(db, || {
                        ConstraintSet::from(relation.is_assignability()).and(db, || {
                            other.has_relation_to_impl(
                                db,
                                KnownClass::Type.to_instance(db),
                                inferable,
                                relation,
                                relation_visitor,
                                disjointness_visitor,
                            )
                        })
                    })
            }

            // Any `type[...]` type is assignable to `type[Any]`
            (other, Type::SubclassOf(subclass_of_ty))
                if subclass_of_ty.is_dynamic() && relation.is_assignability() =>
            {
                other.has_relation_to_impl(
                    db,
                    KnownClass::Type.to_instance(db),
                    inferable,
                    relation,
                    relation_visitor,
                    disjointness_visitor,
                )
            }

            // `type[str]` (== `SubclassOf("str")` in ty) describes all possible runtime subclasses
            // of the class object `str`. It is a subtype of `type` (== `Instance("type")`) because `str`
            // is an instance of `type`, and so all possible subclasses of `str` will also be instances of `type`.
            //
            // Similarly `type[enum.Enum]`  is a subtype of `enum.EnumMeta` because `enum.Enum`
            // is an instance of `enum.EnumMeta`. `type[Any]` and `type[Unknown]` do not participate in subtyping,
            // however, as they are not fully static types.
            (Type::SubclassOf(subclass_of_ty), _) => subclass_of_ty
                .subclass_of()
                .into_class(db)
                .map(|class| class.metaclass_instance_type(db))
                .unwrap_or_else(|| KnownClass::Type.to_instance(db))
                .has_relation_to_impl(
                    db,
                    target,
                    inferable,
                    relation,
                    relation_visitor,
                    disjointness_visitor,
                ),

            // For example: `Type::SpecialForm(SpecialFormType::Type)` is a subtype of `Type::NominalInstance(_SpecialForm)`,
            // because `Type::SpecialForm(SpecialFormType::Type)` is a set with exactly one runtime value in it
            // (the symbol `typing.Type`), and that symbol is known to be an instance of `typing._SpecialForm` at runtime.
            (Type::SpecialForm(left), right) => left.instance_fallback(db).has_relation_to_impl(
                db,
                right,
                inferable,
                relation,
                relation_visitor,
                disjointness_visitor,
            ),

            (Type::KnownInstance(left), right) => left.instance_fallback(db).has_relation_to_impl(
                db,
                right,
                inferable,
                relation,
                relation_visitor,
                disjointness_visitor,
            ),

            // `bool` is a subtype of `int`, because `bool` subclasses `int`,
            // which means that all instances of `bool` are also instances of `int`
            (Type::NominalInstance(self_instance), Type::NominalInstance(target_instance)) => {
                relation_visitor.visit((self, target, relation), || {
                    self_instance.has_relation_to_impl(
                        db,
                        target_instance,
                        inferable,
                        relation,
                        relation_visitor,
                        disjointness_visitor,
                    )
                })
            }

            (Type::PropertyInstance(_), _) => {
                KnownClass::Property.to_instance(db).has_relation_to_impl(
                    db,
                    target,
                    inferable,
                    relation,
                    relation_visitor,
                    disjointness_visitor,
                )
            }
            (_, Type::PropertyInstance(_)) => self.has_relation_to_impl(
                db,
                KnownClass::Property.to_instance(db),
                inferable,
                relation,
                relation_visitor,
                disjointness_visitor,
            ),

            // Other than the special cases enumerated above, nominal-instance types are never
            // subtypes of any other variants
            (Type::NominalInstance(_), _) => ConstraintSet::from(false),
        }
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
        self.when_equivalent_to(db, other, InferableTypeVars::None)
            .is_always_satisfied(db)
    }

    pub(crate) fn when_equivalent_to(
        self,
        db: &'db dyn Db,
        other: Type<'db>,
        inferable: InferableTypeVars<'_, 'db>,
    ) -> ConstraintSet<'db> {
        self.is_equivalent_to_impl(db, other, inferable, &IsEquivalentVisitor::default())
    }

    pub(crate) fn is_equivalent_to_impl(
        self,
        db: &'db dyn Db,
        other: Type<'db>,
        inferable: InferableTypeVars<'_, 'db>,
        visitor: &IsEquivalentVisitor<'db>,
    ) -> ConstraintSet<'db> {
        if self == other {
            return ConstraintSet::from(true);
        }

        match (self, other) {
            // The `Divergent` type is a special type that is not equivalent to other kinds of dynamic types,
            // which prevents `Divergent` from being eliminated during union reduction.
            (Type::Dynamic(_), Type::Dynamic(DynamicType::Divergent(_)))
            | (Type::Dynamic(DynamicType::Divergent(_)), Type::Dynamic(_)) => {
                ConstraintSet::from(false)
            }
            (Type::Dynamic(_), Type::Dynamic(_)) => ConstraintSet::from(true),

            (Type::SubclassOf(first), Type::SubclassOf(second)) => {
                match (first.subclass_of(), second.subclass_of()) {
                    (first, second) if first == second => ConstraintSet::from(true),
                    (SubclassOfInner::Dynamic(_), SubclassOfInner::Dynamic(_)) => {
                        ConstraintSet::from(true)
                    }
                    _ => ConstraintSet::from(false),
                }
            }

            (Type::TypeAlias(self_alias), _) => {
                let self_alias_ty = self_alias.value_type(db).normalized(db);
                visitor.visit((self_alias_ty, other), || {
                    self_alias_ty.is_equivalent_to_impl(db, other, inferable, visitor)
                })
            }

            (_, Type::TypeAlias(other_alias)) => {
                let other_alias_ty = other_alias.value_type(db).normalized(db);
                visitor.visit((self, other_alias_ty), || {
                    self.is_equivalent_to_impl(db, other_alias_ty, inferable, visitor)
                })
            }

            (Type::NewTypeInstance(self_newtype), Type::NewTypeInstance(other_newtype)) => {
                ConstraintSet::from(self_newtype.is_equivalent_to_impl(db, other_newtype))
            }

            (Type::NominalInstance(first), Type::NominalInstance(second)) => {
                first.is_equivalent_to_impl(db, second, inferable, visitor)
            }

            (Type::Union(first), Type::Union(second)) => {
                first.is_equivalent_to_impl(db, second, inferable, visitor)
            }

            (Type::Intersection(first), Type::Intersection(second)) => {
                first.is_equivalent_to_impl(db, second, inferable, visitor)
            }

            (Type::FunctionLiteral(self_function), Type::FunctionLiteral(target_function)) => {
                self_function.is_equivalent_to_impl(db, target_function, inferable, visitor)
            }
            (Type::BoundMethod(self_method), Type::BoundMethod(target_method)) => {
                self_method.is_equivalent_to_impl(db, target_method, inferable, visitor)
            }
            (Type::KnownBoundMethod(self_method), Type::KnownBoundMethod(target_method)) => {
                self_method.is_equivalent_to_impl(db, target_method, inferable, visitor)
            }
            (Type::Callable(first), Type::Callable(second)) => {
                first.is_equivalent_to_impl(db, second, inferable, visitor)
            }

            (Type::ProtocolInstance(first), Type::ProtocolInstance(second)) => {
                first.is_equivalent_to_impl(db, second, inferable, visitor)
            }
            (Type::ProtocolInstance(protocol), nominal @ Type::NominalInstance(n))
            | (nominal @ Type::NominalInstance(n), Type::ProtocolInstance(protocol)) => {
                ConstraintSet::from(n.is_object() && protocol.normalized(db) == nominal)
            }
            // An instance of an enum class is equivalent to an enum literal of that class,
            // if that enum has only has one member.
            (Type::NominalInstance(instance), Type::EnumLiteral(literal))
            | (Type::EnumLiteral(literal), Type::NominalInstance(instance)) => {
                if literal.enum_class_instance(db) != Type::NominalInstance(instance) {
                    return ConstraintSet::from(false);
                }
                ConstraintSet::from(is_single_member_enum(db, instance.class_literal(db)))
            }

            (Type::PropertyInstance(left), Type::PropertyInstance(right)) => {
                left.is_equivalent_to_impl(db, right, inferable, visitor)
            }

            (Type::TypedDict(left), Type::TypedDict(right)) => visitor.visit((self, other), || {
                left.is_equivalent_to_impl(db, right, inferable, visitor)
            }),

            _ => ConstraintSet::from(false),
        }
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
        self.when_disjoint_from(db, other, InferableTypeVars::None)
            .is_always_satisfied(db)
    }

    pub(crate) fn when_disjoint_from(
        self,
        db: &'db dyn Db,
        other: Type<'db>,
        inferable: InferableTypeVars<'_, 'db>,
    ) -> ConstraintSet<'db> {
        self.is_disjoint_from_impl(
            db,
            other,
            inferable,
            &IsDisjointVisitor::default(),
            &HasRelationToVisitor::default(),
        )
    }

    pub(crate) fn is_disjoint_from_impl(
        self,
        db: &'db dyn Db,
        other: Type<'db>,
        inferable: InferableTypeVars<'_, 'db>,
        disjointness_visitor: &IsDisjointVisitor<'db>,
        relation_visitor: &HasRelationToVisitor<'db>,
    ) -> ConstraintSet<'db> {
        fn any_protocol_members_absent_or_disjoint<'db>(
            db: &'db dyn Db,
            protocol: ProtocolInstanceType<'db>,
            other: Type<'db>,
            inferable: InferableTypeVars<'_, 'db>,
            disjointness_visitor: &IsDisjointVisitor<'db>,
            relation_visitor: &HasRelationToVisitor<'db>,
        ) -> ConstraintSet<'db> {
            protocol.interface(db).members(db).when_any(db, |member| {
                other
                    .member(db, member.name())
                    .place
                    .ignore_possibly_undefined()
                    .when_none_or(|attribute_type| {
                        member.has_disjoint_type_from(
                            db,
                            attribute_type,
                            inferable,
                            disjointness_visitor,
                            relation_visitor,
                        )
                    })
            })
        }

        match (self, other) {
            (Type::Never, _) | (_, Type::Never) => ConstraintSet::from(true),

            (Type::Dynamic(_), _) | (_, Type::Dynamic(_)) => ConstraintSet::from(false),

            (Type::TypeAlias(alias), _) => {
                let self_alias_ty = alias.value_type(db);
                disjointness_visitor.visit((self, other), || {
                    self_alias_ty.is_disjoint_from_impl(
                        db,
                        other,
                        inferable,
                        disjointness_visitor,
                        relation_visitor,
                    )
                })
            }

            (_, Type::TypeAlias(alias)) => {
                let other_alias_ty = alias.value_type(db);
                disjointness_visitor.visit((self, other), || {
                    self.is_disjoint_from_impl(
                        db,
                        other_alias_ty,
                        inferable,
                        disjointness_visitor,
                        relation_visitor,
                    )
                })
            }

            // `type[T]` is disjoint from a callable or protocol instance if its upper bound or constraints are.
            (Type::SubclassOf(subclass_of), Type::Callable(_) | Type::ProtocolInstance(_))
            | (Type::Callable(_) | Type::ProtocolInstance(_), Type::SubclassOf(subclass_of))
                if subclass_of.is_type_var() =>
            {
                let type_var = subclass_of
                    .subclass_of()
                    .with_transposed_type_var(db)
                    .into_type_var()
                    .unwrap();

                Type::TypeVar(type_var).is_disjoint_from_impl(
                    db,
                    other,
                    inferable,
                    disjointness_visitor,
                    relation_visitor,
                )
            }

            // `type[T]` is disjoint from a class object `A` if every instance of `T` is disjoint from an instance of `A`.
            (Type::SubclassOf(subclass_of), other) | (other, Type::SubclassOf(subclass_of))
                if !subclass_of
                    .into_type_var()
                    .zip(other.to_instance(db))
                    .when_none_or(|(this_instance, other_instance)| {
                        Type::TypeVar(this_instance).is_disjoint_from_impl(
                            db,
                            other_instance,
                            inferable,
                            disjointness_visitor,
                            relation_visitor,
                        )
                    })
                    .is_always_satisfied(db) =>
            {
                // TODO: The repetition here isn't great, but we need the fallthrough logic.
                subclass_of
                    .into_type_var()
                    .zip(other.to_instance(db))
                    .when_none_or(|(this_instance, other_instance)| {
                        Type::TypeVar(this_instance).is_disjoint_from_impl(
                            db,
                            other_instance,
                            inferable,
                            disjointness_visitor,
                            relation_visitor,
                        )
                    })
            }

            // A typevar is never disjoint from itself, since all occurrences of the typevar must
            // be specialized to the same type. (This is an important difference between typevars
            // and `Any`!) Different typevars might be disjoint, depending on their bounds and
            // constraints, which are handled below.
            (Type::TypeVar(self_bound_typevar), Type::TypeVar(other_bound_typevar))
                if !self_bound_typevar.is_inferable(db, inferable)
                    && self_bound_typevar.is_same_typevar_as(db, other_bound_typevar) =>
            {
                ConstraintSet::from(false)
            }

            (tvar @ Type::TypeVar(bound_typevar), Type::Intersection(intersection))
            | (Type::Intersection(intersection), tvar @ Type::TypeVar(bound_typevar))
                if !bound_typevar.is_inferable(db, inferable)
                    && intersection.negative(db).contains(&tvar) =>
            {
                ConstraintSet::from(true)
            }

            // An unbounded typevar is never disjoint from any other type, since it might be
            // specialized to any type. A bounded typevar is not disjoint from its bound, and is
            // only disjoint from other types if its bound is. A constrained typevar is disjoint
            // from a type if all of its constraints are.
            (Type::TypeVar(bound_typevar), other) | (other, Type::TypeVar(bound_typevar))
                if !bound_typevar.is_inferable(db, inferable) =>
            {
                match bound_typevar.typevar(db).bound_or_constraints(db) {
                    None => ConstraintSet::from(false),
                    Some(TypeVarBoundOrConstraints::UpperBound(bound)) => bound
                        .is_disjoint_from_impl(
                            db,
                            other,
                            inferable,
                            disjointness_visitor,
                            relation_visitor,
                        ),
                    Some(TypeVarBoundOrConstraints::Constraints(constraints)) => {
                        constraints.elements(db).iter().when_all(db, |constraint| {
                            constraint.is_disjoint_from_impl(
                                db,
                                other,
                                inferable,
                                disjointness_visitor,
                                relation_visitor,
                            )
                        })
                    }
                }
            }

            // TODO: Infer specializations here
            (Type::TypeVar(_), _) | (_, Type::TypeVar(_)) => ConstraintSet::from(false),

            (Type::Union(union), other) | (other, Type::Union(union)) => {
                union.elements(db).iter().when_all(db, |e| {
                    e.is_disjoint_from_impl(
                        db,
                        other,
                        inferable,
                        disjointness_visitor,
                        relation_visitor,
                    )
                })
            }

            // If we have two intersections, we test the positive elements of each one against the other intersection
            // Negative elements need a positive element on the other side in order to be disjoint.
            // This is similar to what would happen if we tried to build a new intersection that combines the two
            (Type::Intersection(self_intersection), Type::Intersection(other_intersection)) => {
                disjointness_visitor.visit((self, other), || {
                    self_intersection
                        .positive(db)
                        .iter()
                        .when_any(db, |p| {
                            p.is_disjoint_from_impl(
                                db,
                                other,
                                inferable,
                                disjointness_visitor,
                                relation_visitor,
                            )
                        })
                        .or(db, || {
                            other_intersection.positive(db).iter().when_any(db, |p| {
                                p.is_disjoint_from_impl(
                                    db,
                                    self,
                                    inferable,
                                    disjointness_visitor,
                                    relation_visitor,
                                )
                            })
                        })
                })
            }

            (Type::Intersection(intersection), non_intersection)
            | (non_intersection, Type::Intersection(intersection)) => {
                disjointness_visitor.visit((self, other), || {
                    intersection
                        .positive(db)
                        .iter()
                        .when_any(db, |p| {
                            p.is_disjoint_from_impl(
                                db,
                                non_intersection,
                                inferable,
                                disjointness_visitor,
                                relation_visitor,
                            )
                        })
                        // A & B & Not[C] is disjoint from C
                        .or(db, || {
                            intersection.negative(db).iter().when_any(db, |&neg_ty| {
                                non_intersection.has_relation_to_impl(
                                    db,
                                    neg_ty,
                                    inferable,
                                    TypeRelation::Subtyping,
                                    relation_visitor,
                                    disjointness_visitor,
                                )
                            })
                        })
                })
            }

            // any single-valued type is disjoint from another single-valued type
            // iff the two types are nonequal
            (
                left @ (Type::BooleanLiteral(..)
                | Type::IntLiteral(..)
                | Type::StringLiteral(..)
                | Type::BytesLiteral(..)
                | Type::EnumLiteral(..)
                | Type::FunctionLiteral(..)
                | Type::BoundMethod(..)
                | Type::KnownBoundMethod(..)
                | Type::WrapperDescriptor(..)
                | Type::ModuleLiteral(..)
                | Type::ClassLiteral(..)
                | Type::SpecialForm(..)
                | Type::KnownInstance(..)),
                right @ (Type::BooleanLiteral(..)
                | Type::IntLiteral(..)
                | Type::StringLiteral(..)
                | Type::BytesLiteral(..)
                | Type::EnumLiteral(..)
                | Type::FunctionLiteral(..)
                | Type::BoundMethod(..)
                | Type::KnownBoundMethod(..)
                | Type::WrapperDescriptor(..)
                | Type::ModuleLiteral(..)
                | Type::ClassLiteral(..)
                | Type::SpecialForm(..)
                | Type::KnownInstance(..)),
            ) => ConstraintSet::from(left != right),

            (
                Type::SubclassOf(_),
                Type::BooleanLiteral(..)
                | Type::IntLiteral(..)
                | Type::StringLiteral(..)
                | Type::LiteralString
                | Type::BytesLiteral(..)
                | Type::EnumLiteral(..)
                | Type::FunctionLiteral(..)
                | Type::BoundMethod(..)
                | Type::KnownBoundMethod(..)
                | Type::WrapperDescriptor(..)
                | Type::ModuleLiteral(..),
            )
            | (
                Type::BooleanLiteral(..)
                | Type::IntLiteral(..)
                | Type::StringLiteral(..)
                | Type::LiteralString
                | Type::BytesLiteral(..)
                | Type::EnumLiteral(..)
                | Type::FunctionLiteral(..)
                | Type::BoundMethod(..)
                | Type::KnownBoundMethod(..)
                | Type::WrapperDescriptor(..)
                | Type::ModuleLiteral(..),
                Type::SubclassOf(_),
            ) => ConstraintSet::from(true),

            (Type::AlwaysTruthy, ty) | (ty, Type::AlwaysTruthy) => {
                // `Truthiness::Ambiguous` may include `AlwaysTrue` as a subset, so it's not guaranteed to be disjoint.
                // Thus, they are only disjoint if `ty.bool() == AlwaysFalse`.
                ConstraintSet::from(ty.bool(db).is_always_false())
            }
            (Type::AlwaysFalsy, ty) | (ty, Type::AlwaysFalsy) => {
                // Similarly, they are only disjoint if `ty.bool() == AlwaysTrue`.
                ConstraintSet::from(ty.bool(db).is_always_true())
            }

            (Type::ProtocolInstance(left), Type::ProtocolInstance(right)) => disjointness_visitor
                .visit((self, other), || {
                    left.is_disjoint_from_impl(db, right, inferable, disjointness_visitor)
                }),

            (Type::ProtocolInstance(protocol), Type::SpecialForm(special_form))
            | (Type::SpecialForm(special_form), Type::ProtocolInstance(protocol)) => {
                disjointness_visitor.visit((self, other), || {
                    any_protocol_members_absent_or_disjoint(
                        db,
                        protocol,
                        special_form.instance_fallback(db),
                        inferable,
                        disjointness_visitor,
                        relation_visitor,
                    )
                })
            }

            (Type::ProtocolInstance(protocol), Type::KnownInstance(known_instance))
            | (Type::KnownInstance(known_instance), Type::ProtocolInstance(protocol)) => {
                disjointness_visitor.visit((self, other), || {
                    any_protocol_members_absent_or_disjoint(
                        db,
                        protocol,
                        known_instance.instance_fallback(db),
                        inferable,
                        disjointness_visitor,
                        relation_visitor,
                    )
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
                ty @ (Type::LiteralString
                | Type::StringLiteral(..)
                | Type::BytesLiteral(..)
                | Type::BooleanLiteral(..)
                | Type::ClassLiteral(..)
                | Type::FunctionLiteral(..)
                | Type::ModuleLiteral(..)
                | Type::GenericAlias(..)
                | Type::IntLiteral(..)
                | Type::EnumLiteral(..)),
                Type::ProtocolInstance(protocol),
            )
            | (
                Type::ProtocolInstance(protocol),
                ty @ (Type::LiteralString
                | Type::StringLiteral(..)
                | Type::BytesLiteral(..)
                | Type::BooleanLiteral(..)
                | Type::ClassLiteral(..)
                | Type::FunctionLiteral(..)
                | Type::ModuleLiteral(..)
                | Type::GenericAlias(..)
                | Type::IntLiteral(..)
                | Type::EnumLiteral(..)),
            ) => disjointness_visitor.visit((self, other), || {
                any_protocol_members_absent_or_disjoint(
                    db,
                    protocol,
                    ty,
                    inferable,
                    disjointness_visitor,
                    relation_visitor,
                )
            }),

            // This is the same as the branch above --
            // once guard patterns are stabilised, it could be unified with that branch
            // (<https://github.com/rust-lang/rust/issues/129967>)
            (Type::ProtocolInstance(protocol), nominal @ Type::NominalInstance(n))
            | (nominal @ Type::NominalInstance(n), Type::ProtocolInstance(protocol))
                if n.class(db).is_final(db) =>
            {
                disjointness_visitor.visit((self, other), || {
                    any_protocol_members_absent_or_disjoint(
                        db,
                        protocol,
                        nominal,
                        inferable,
                        disjointness_visitor,
                        relation_visitor,
                    )
                })
            }

            (Type::ProtocolInstance(protocol), other)
            | (other, Type::ProtocolInstance(protocol)) => {
                disjointness_visitor.visit((self, other), || {
                    protocol.interface(db).members(db).when_any(db, |member| {
                        match other.member(db, member.name()).place {
                            Place::Defined(DefinedPlace {
                                ty: attribute_type, ..
                            }) => member.has_disjoint_type_from(
                                db,
                                attribute_type,
                                inferable,
                                disjointness_visitor,
                                relation_visitor,
                            ),
                            Place::Undefined => ConstraintSet::from(false),
                        }
                    })
                })
            }

            (Type::SubclassOf(subclass_of_ty), _) | (_, Type::SubclassOf(subclass_of_ty))
                if subclass_of_ty.is_type_var() =>
            {
                ConstraintSet::from(true)
            }

            (Type::GenericAlias(left_alias), Type::GenericAlias(right_alias)) => {
                ConstraintSet::from(left_alias.origin(db) != right_alias.origin(db)).or(db, || {
                    left_alias.specialization(db).is_disjoint_from_impl(
                        db,
                        right_alias.specialization(db),
                        inferable,
                        disjointness_visitor,
                        relation_visitor,
                    )
                })
            }

            (Type::ClassLiteral(class_literal), other @ Type::GenericAlias(_))
            | (other @ Type::GenericAlias(_), Type::ClassLiteral(class_literal)) => class_literal
                .default_specialization(db)
                .into_generic_alias()
                .when_none_or(|alias| {
                    other.is_disjoint_from_impl(
                        db,
                        Type::GenericAlias(alias),
                        inferable,
                        disjointness_visitor,
                        relation_visitor,
                    )
                }),

            (Type::SubclassOf(subclass_of_ty), Type::ClassLiteral(class_b))
            | (Type::ClassLiteral(class_b), Type::SubclassOf(subclass_of_ty)) => {
                match subclass_of_ty.subclass_of() {
                    SubclassOfInner::Dynamic(_) => ConstraintSet::from(false),
                    SubclassOfInner::Class(class_a) => ConstraintSet::from(
                        !class_a.could_exist_in_mro_of(db, ClassType::NonGeneric(class_b)),
                    ),
                    SubclassOfInner::TypeVar(_) => unreachable!(),
                }
            }

            (Type::SubclassOf(subclass_of_ty), Type::GenericAlias(alias_b))
            | (Type::GenericAlias(alias_b), Type::SubclassOf(subclass_of_ty)) => {
                match subclass_of_ty.subclass_of() {
                    SubclassOfInner::Dynamic(_) => ConstraintSet::from(false),
                    SubclassOfInner::Class(class_a) => ConstraintSet::from(
                        !class_a.could_exist_in_mro_of(db, ClassType::Generic(alias_b)),
                    ),
                    SubclassOfInner::TypeVar(_) => unreachable!(),
                }
            }

            (Type::SubclassOf(left), Type::SubclassOf(right)) => {
                left.is_disjoint_from_impl(db, right, inferable, disjointness_visitor)
            }

            // for `type[Any]`/`type[Unknown]`/`type[Todo]`, we know the type cannot be any larger than `type`,
            // so although the type is dynamic we can still determine disjointedness in some situations
            (Type::SubclassOf(subclass_of_ty), other)
            | (other, Type::SubclassOf(subclass_of_ty)) => match subclass_of_ty.subclass_of() {
                SubclassOfInner::Dynamic(_) => {
                    KnownClass::Type.to_instance(db).is_disjoint_from_impl(
                        db,
                        other,
                        inferable,
                        disjointness_visitor,
                        relation_visitor,
                    )
                }
                SubclassOfInner::Class(class) => {
                    class.metaclass_instance_type(db).is_disjoint_from_impl(
                        db,
                        other,
                        inferable,
                        disjointness_visitor,
                        relation_visitor,
                    )
                }
                SubclassOfInner::TypeVar(_) => unreachable!(),
            },

            (Type::SpecialForm(special_form), Type::NominalInstance(instance))
            | (Type::NominalInstance(instance), Type::SpecialForm(special_form)) => {
                ConstraintSet::from(!special_form.is_instance_of(db, instance.class(db)))
            }

            (Type::KnownInstance(known_instance), Type::NominalInstance(instance))
            | (Type::NominalInstance(instance), Type::KnownInstance(known_instance)) => {
                ConstraintSet::from(!known_instance.is_instance_of(db, instance.class(db)))
            }

            (
                Type::BooleanLiteral(..) | Type::TypeIs(_) | Type::TypeGuard(_),
                Type::NominalInstance(instance),
            )
            | (
                Type::NominalInstance(instance),
                Type::BooleanLiteral(..) | Type::TypeIs(_) | Type::TypeGuard(_),
            ) => {
                // A `Type::BooleanLiteral()` must be an instance of exactly `bool`
                // (it cannot be an instance of a `bool` subclass)
                KnownClass::Bool
                    .when_subclass_of(db, instance.class(db))
                    .negate(db)
            }

            (Type::BooleanLiteral(..) | Type::TypeIs(_) | Type::TypeGuard(_), _)
            | (_, Type::BooleanLiteral(..) | Type::TypeIs(_) | Type::TypeGuard(_)) => {
                ConstraintSet::from(true)
            }

            (Type::IntLiteral(..), Type::NominalInstance(instance))
            | (Type::NominalInstance(instance), Type::IntLiteral(..)) => {
                // A `Type::IntLiteral()` must be an instance of exactly `int`
                // (it cannot be an instance of an `int` subclass)
                KnownClass::Int
                    .when_subclass_of(db, instance.class(db))
                    .negate(db)
            }

            (Type::IntLiteral(..), _) | (_, Type::IntLiteral(..)) => ConstraintSet::from(true),

            (Type::StringLiteral(..), Type::LiteralString)
            | (Type::LiteralString, Type::StringLiteral(..)) => ConstraintSet::from(false),

            (Type::StringLiteral(..) | Type::LiteralString, Type::NominalInstance(instance))
            | (Type::NominalInstance(instance), Type::StringLiteral(..) | Type::LiteralString) => {
                // A `Type::StringLiteral()` or a `Type::LiteralString` must be an instance of exactly `str`
                // (it cannot be an instance of a `str` subclass)
                KnownClass::Str
                    .when_subclass_of(db, instance.class(db))
                    .negate(db)
            }

            (Type::LiteralString, Type::LiteralString) => ConstraintSet::from(false),
            (Type::LiteralString, _) | (_, Type::LiteralString) => ConstraintSet::from(true),

            (Type::BytesLiteral(..), Type::NominalInstance(instance))
            | (Type::NominalInstance(instance), Type::BytesLiteral(..)) => {
                // A `Type::BytesLiteral()` must be an instance of exactly `bytes`
                // (it cannot be an instance of a `bytes` subclass)
                KnownClass::Bytes
                    .when_subclass_of(db, instance.class(db))
                    .negate(db)
            }

            (Type::EnumLiteral(enum_literal), instance @ Type::NominalInstance(_))
            | (instance @ Type::NominalInstance(_), Type::EnumLiteral(enum_literal)) => {
                enum_literal
                    .enum_class_instance(db)
                    .has_relation_to_impl(
                        db,
                        instance,
                        inferable,
                        TypeRelation::Subtyping,
                        relation_visitor,
                        disjointness_visitor,
                    )
                    .negate(db)
            }
            (Type::EnumLiteral(..), _) | (_, Type::EnumLiteral(..)) => ConstraintSet::from(true),

            // A class-literal type `X` is always disjoint from an instance type `Y`,
            // unless the type expressing "all instances of `Z`" is a subtype of of `Y`,
            // where `Z` is `X`'s metaclass.
            (Type::ClassLiteral(class), instance @ Type::NominalInstance(_))
            | (instance @ Type::NominalInstance(_), Type::ClassLiteral(class)) => class
                .metaclass_instance_type(db)
                .when_subtype_of(db, instance, inferable)
                .negate(db),
            (Type::GenericAlias(alias), instance @ Type::NominalInstance(_))
            | (instance @ Type::NominalInstance(_), Type::GenericAlias(alias)) => {
                ClassType::from(alias)
                    .metaclass_instance_type(db)
                    .has_relation_to_impl(
                        db,
                        instance,
                        inferable,
                        TypeRelation::Subtyping,
                        relation_visitor,
                        disjointness_visitor,
                    )
                    .negate(db)
            }

            (Type::FunctionLiteral(..), Type::NominalInstance(instance))
            | (Type::NominalInstance(instance), Type::FunctionLiteral(..)) => {
                // A `Type::FunctionLiteral()` must be an instance of exactly `types.FunctionType`
                // (it cannot be an instance of a `types.FunctionType` subclass)
                KnownClass::FunctionType
                    .when_subclass_of(db, instance.class(db))
                    .negate(db)
            }

            (Type::BoundMethod(_), other) | (other, Type::BoundMethod(_)) => KnownClass::MethodType
                .to_instance(db)
                .is_disjoint_from_impl(
                    db,
                    other,
                    inferable,
                    disjointness_visitor,
                    relation_visitor,
                ),

            (Type::KnownBoundMethod(method), other) | (other, Type::KnownBoundMethod(method)) => {
                method.class().to_instance(db).is_disjoint_from_impl(
                    db,
                    other,
                    inferable,
                    disjointness_visitor,
                    relation_visitor,
                )
            }

            (Type::WrapperDescriptor(_), other) | (other, Type::WrapperDescriptor(_)) => {
                KnownClass::WrapperDescriptorType
                    .to_instance(db)
                    .is_disjoint_from_impl(
                        db,
                        other,
                        inferable,
                        disjointness_visitor,
                        relation_visitor,
                    )
            }

            (Type::Callable(_) | Type::FunctionLiteral(_), Type::Callable(_))
            | (Type::Callable(_), Type::FunctionLiteral(_)) => {
                // No two callable types are ever disjoint because
                // `(*args: object, **kwargs: object) -> Never` is a subtype of all fully static
                // callable types.
                ConstraintSet::from(false)
            }

            (Type::Callable(_), Type::StringLiteral(_) | Type::BytesLiteral(_))
            | (Type::StringLiteral(_) | Type::BytesLiteral(_), Type::Callable(_)) => {
                // A callable type is disjoint from other literal types. For example,
                // `Type::StringLiteral` must be an instance of exactly `str`, not a subclass
                // of `str`, and `str` is not callable. The same applies to other literal types.
                ConstraintSet::from(true)
            }

            (Type::Callable(_), Type::SpecialForm(special_form))
            | (Type::SpecialForm(special_form), Type::Callable(_)) => {
                // A callable type is disjoint from special form types, except for special forms
                // that are callable (like TypedDict and collection constructors).
                // Most special forms are type constructors/annotations (like `typing.Literal`,
                // `typing.Union`, etc.) that are subscripted, not called.
                ConstraintSet::from(!special_form.is_callable())
            }

            (
                Type::Callable(_) | Type::DataclassDecorator(_) | Type::DataclassTransformer(_),
                instance @ Type::NominalInstance(nominal),
            )
            | (
                instance @ Type::NominalInstance(nominal),
                Type::Callable(_) | Type::DataclassDecorator(_) | Type::DataclassTransformer(_),
            ) if nominal.class(db).is_final(db) => instance
                .member_lookup_with_policy(
                    db,
                    Name::new_static("__call__"),
                    MemberLookupPolicy::NO_INSTANCE_FALLBACK,
                )
                .place
                .ignore_possibly_undefined()
                .when_none_or(|dunder_call| {
                    dunder_call
                        .has_relation_to_impl(
                            db,
                            Type::Callable(CallableType::unknown(db)),
                            inferable,
                            TypeRelation::Assignability,
                            relation_visitor,
                            disjointness_visitor,
                        )
                        .negate(db)
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
                ConstraintSet::from(false)
            }

            (Type::ModuleLiteral(..), other @ Type::NominalInstance(..))
            | (other @ Type::NominalInstance(..), Type::ModuleLiteral(..)) => {
                // Modules *can* actually be instances of `ModuleType` subclasses
                other.is_disjoint_from_impl(
                    db,
                    KnownClass::ModuleType.to_instance(db),
                    inferable,
                    disjointness_visitor,
                    relation_visitor,
                )
            }

            (Type::NominalInstance(left), Type::NominalInstance(right)) => disjointness_visitor
                .visit((self, other), || {
                    left.is_disjoint_from_impl(
                        db,
                        right,
                        inferable,
                        disjointness_visitor,
                        relation_visitor,
                    )
                }),

            (Type::NewTypeInstance(left), Type::NewTypeInstance(right)) => {
                left.is_disjoint_from_impl(db, right)
            }
            (Type::NewTypeInstance(newtype), other) | (other, Type::NewTypeInstance(newtype)) => {
                newtype.concrete_base_type(db).is_disjoint_from_impl(
                    db,
                    other,
                    inferable,
                    disjointness_visitor,
                    relation_visitor,
                )
            }

            (Type::PropertyInstance(_), other) | (other, Type::PropertyInstance(_)) => {
                KnownClass::Property.to_instance(db).is_disjoint_from_impl(
                    db,
                    other,
                    inferable,
                    disjointness_visitor,
                    relation_visitor,
                )
            }

            (Type::BoundSuper(_), Type::BoundSuper(_)) => {
                self.when_equivalent_to(db, other, inferable).negate(db)
            }
            (Type::BoundSuper(_), other) | (other, Type::BoundSuper(_)) => {
                KnownClass::Super.to_instance(db).is_disjoint_from_impl(
                    db,
                    other,
                    inferable,
                    disjointness_visitor,
                    relation_visitor,
                )
            }

            (Type::GenericAlias(_), _) | (_, Type::GenericAlias(_)) => ConstraintSet::from(true),

            (Type::TypedDict(self_typeddict), Type::TypedDict(other_typeddict)) => {
                disjointness_visitor.visit((self, other), || {
                    self_typeddict.is_disjoint_from_impl(
                        db,
                        other_typeddict,
                        inferable,
                        disjointness_visitor,
                        relation_visitor,
                    )
                })
            }

            // For any type `T`, if `dict[str, Any]` is not assignable to `T`, then all `TypedDict`
            // types will always be disjoint from `T`. This doesn't cover all cases -- in fact
            // `dict` *itself* is almost always disjoint from `TypedDict` -- but it's a good
            // approximation, and some false negatives are acceptable.
            (Type::TypedDict(_), other) | (other, Type::TypedDict(_)) => KnownClass::Dict
                .to_specialized_instance(db, [KnownClass::Str.to_instance(db), Type::any()])
                .has_relation_to_impl(
                    db,
                    other,
                    inferable,
                    TypeRelation::Assignability,
                    relation_visitor,
                    disjointness_visitor,
                )
                .negate(db),
        }
    }
}

#[allow(clippy::trivially_copy_pass_by_ref)]
fn is_redundant_with_cycle_initial<'db>(
    _db: &'db dyn Db,
    _id: salsa::Id,
    _subtype: Type<'db>,
    _supertype: Type<'db>,
) -> bool {
    true
}

/// A [`PairVisitor`] that is used in `has_relation_to` methods.
pub(crate) type HasRelationToVisitor<'db> =
    CycleDetector<TypeRelation<'db>, (Type<'db>, Type<'db>, TypeRelation<'db>), ConstraintSet<'db>>;

impl Default for HasRelationToVisitor<'_> {
    fn default() -> Self {
        HasRelationToVisitor::new(ConstraintSet::from(true))
    }
}

/// A [`PairVisitor`] that is used in `is_disjoint_from` methods.
pub(crate) type IsDisjointVisitor<'db> = PairVisitor<'db, IsDisjoint, ConstraintSet<'db>>;

#[derive(Debug)]
pub(crate) struct IsDisjoint;

impl Default for IsDisjointVisitor<'_> {
    fn default() -> Self {
        IsDisjointVisitor::new(ConstraintSet::from(false))
    }
}

/// A [`PairVisitor`] that is used in `is_equivalent` methods.
pub(crate) type IsEquivalentVisitor<'db> = PairVisitor<'db, IsEquivalent, ConstraintSet<'db>>;

#[derive(Debug)]
pub(crate) struct IsEquivalent;

impl Default for IsEquivalentVisitor<'_> {
    fn default() -> Self {
        IsEquivalentVisitor::new(ConstraintSet::from(true))
    }
}
