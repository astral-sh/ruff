use crate::Db;
use crate::reachability::narrow_type_by_constraint;
use crate::subscript::PyIndex;
use crate::types::function::KnownFunction;
use crate::types::infer::{ExpressionInference, infer_same_file_expression_type};
use crate::types::special_form::TypeQualifier;
use crate::types::tuple::{TupleLength, TupleType};
use crate::types::typed_dict::{
    TypedDictField, TypedDictFieldBuilder, TypedDictSchema, TypedDictType,
};
use crate::types::{
    CallableType, ClassLiteral, ClassType, IntersectionBuilder, IntersectionType, KnownClass,
    KnownInstanceType, LiteralValueTypeKind, Parameter, Parameters, Signature, SpecialFormType,
    SubclassOfInner, SubclassOfType, Truthiness, Type, TypeContext, TypeVarBoundOrConstraints,
    UnionBuilder, callable_pattern_type, definite_sequence_pattern_type,
    exact_sequence_pattern_type, infer_expression_types, mapping_pattern_type,
    sequence_pattern_type_builder, singleton_pattern_type, starred_sequence_pattern_type,
};
use ty_python_core::expression::Expression;
use ty_python_core::place::{PlaceExpr, PlaceTable, ScopedPlaceId};
use ty_python_core::predicate::{
    CallableAndCallExpr, ClassPatternKind, PatternPredicate, PatternPredicateKind, Predicate,
    PredicateNode, SequencePatternPredicateKind, SubjectElementPatternPredicate,
};
use ty_python_core::scope::ScopeId;
use ty_python_core::{ExpressionNodeKey, NarrowingEvaluator, place_table, semantic_index};

use ruff_db::parsed::{ParsedModuleRef, parsed_module};
use ruff_python_ast::name::Name;
use ruff_python_stdlib::identifiers::is_identifier;

use super::UnionType;
use super::enums::{enum_member_literals, enum_metadata};
use super::equality::{evaluate_type_equality, evaluate_type_inequality};
use itertools::Itertools;
use ruff_python_ast as ast;
use ruff_python_ast::{BoolOp, ExprBoolOp};
use rustc_hash::FxHashMap;
use smallvec::{SmallVec, smallvec, smallvec_inline};
use std::collections::hash_map::Entry;
use ty_python_core::frozen::FrozenMap;

fn is_union_of_single_valued<'db>(db: &'db dyn Db, ty: Type<'db>) -> bool {
    let ty = ty.resolve_type_alias(db);
    ty.as_union().is_some_and(|union| {
        union
            .elements(db)
            .iter()
            .all(|ty| is_single_valued_union_component(db, *ty))
    }) || is_single_valued_union_component(db, ty)
}

fn is_union_with_single_valued<'db>(db: &'db dyn Db, ty: Type<'db>) -> bool {
    let ty = ty.resolve_type_alias(db);
    ty.as_union().is_some_and(|union| {
        union
            .elements(db)
            .iter()
            .any(|ty| is_single_valued_union_component(db, *ty))
    }) || is_single_valued_union_component(db, ty)
}

/// Return `true` if this type can participate in single-valued-union narrowing.
///
/// A component can be literally single-valued, like `Literal[1]`, or a finite multi-valued
/// domain whose alternatives can each be treated as single-valued, like `bool` or an enum
/// complement.
///
/// ```python
/// from enum import Enum
///
/// class Color(Enum):
///     RED = 1
///     BLUE = 2
///
/// def f(color: Color):
///     if color is not Color.RED:
///         # `color` is a multi-valued component, but its remaining alternatives are
///         # single-valued enum literals.
///         reveal_type(color)  # Literal[Color.BLUE]
/// ```
fn is_single_valued_union_component<'db>(db: &'db dyn Db, ty: Type<'db>) -> bool {
    let ty = ty.resolve_type_alias(db);
    ty.is_single_valued(db)
        || has_finite_single_valued_union_alternatives(db, ty)
        || ty.is_subtype_of(db, Type::literal_string())
}

/// Split a finite domain into the single-valued alternatives used by equality and membership
/// narrowing.
///
/// This covers finite multi-valued types that `is_single_valued_union_component` treats as
/// splittable, such as `bool`, enums, and compact enum complements. `LiteralString` is
/// intentionally excluded because it is not finite.
fn finite_single_valued_union_alternatives<'db>(
    db: &'db dyn Db,
    ty: Type<'db>,
) -> Option<Vec<Type<'db>>> {
    let ty = ty.resolve_type_alias(db);

    match ty {
        Type::EnumComplement(complement) => complement
            .has_finite_single_valued_alternatives(db)
            .then(|| complement.remaining_literal_types(db)),
        Type::Intersection(intersection) => {
            let complement = intersection.enum_complement(db)?;
            complement
                .has_finite_single_valued_alternatives(db)
                .then(|| complement.remaining_literal_types(db))
        }
        Type::NominalInstance(instance) if instance.has_known_class(db, KnownClass::Bool) => {
            Some(vec![Type::bool_literal(true), Type::bool_literal(false)])
        }
        Type::NominalInstance(instance)
            if enum_metadata(db, instance.class_literal(db)).is_some()
                && !ty.overrides_equality(db) =>
        {
            Some(
                enum_member_literals(db, instance.class_literal(db), None)
                    .expect("Calling `enum_member_literals` on an enum class")
                    .collect(),
            )
        }
        _ => None,
    }
}

/// Return `true` if `finite_single_valued_union_alternatives` would produce a non-empty list.
///
/// Keep this separate from the materializing helper above so boolean probes do not eagerly expand
/// large enum domains into literal vectors.
fn has_finite_single_valued_union_alternatives<'db>(db: &'db dyn Db, ty: Type<'db>) -> bool {
    let ty = ty.resolve_type_alias(db);

    match ty {
        Type::EnumComplement(complement) => complement.has_finite_single_valued_alternatives(db),
        Type::Intersection(intersection) => intersection
            .enum_complement(db)
            .is_some_and(|complement| complement.has_finite_single_valued_alternatives(db)),
        Type::NominalInstance(instance) if instance.has_known_class(db, KnownClass::Bool) => true,
        Type::NominalInstance(instance)
            if enum_metadata(db, instance.class_literal(db))
                .is_some_and(|metadata| !metadata.members.is_empty())
                && !ty.overrides_equality(db) =>
        {
            true
        }
        _ => false,
    }
}

/// Return the type constraints that `test` would place on `symbol` if true and false.
///
/// For example, if we have this code:
///
/// ```python
/// y = 1 if flag else None
/// x = 1 if flag else None
/// if x is not None:
///     ...
/// ```
///
/// The `test` expression `x is not None` places the constraint "not None" on the definition of
/// `x`, so in that case we'd return `Some(Type::Intersection(negative=[Type::None]))`.
///
/// But if we called this with the same `test` expression, but the `symbol` of `y`, no
/// constraint is applied to that symbol, so we'd just return `(None, None)`.
pub(crate) fn infer_narrowing_constraints<'db>(
    db: &'db dyn Db,
    predicate: Predicate<'db>,
    place: ScopedPlaceId,
) -> (
    Option<NarrowingConstraint<'db>>,
    Option<NarrowingConstraint<'db>>,
) {
    let constraints = match predicate.node {
        PredicateNode::Expression(expression) => {
            let constraints = all_narrowing_constraints_for_expression(db, expression);
            (
                constraints.get(place, true).cloned(),
                constraints.get(place, false).cloned(),
            )
        }
        PredicateNode::Pattern(pattern) => {
            let positive = all_narrowing_constraints_for_pattern(db, pattern)
                .and_then(|constraints| constraints.get(&place).cloned());
            let negative = all_negative_narrowing_constraints_for_pattern(db, pattern)
                .and_then(|constraints| constraints.get(&place).cloned());
            (positive, negative)
        }
        PredicateNode::SubjectElementPattern(subject_element) => {
            let positive = all_narrowing_constraints_for_subject_element_pattern(
                db,
                subject_element.pattern,
                subject_element.target,
            )
            .and_then(|constraints| constraints.get(&place).cloned());
            (positive, None)
        }
        PredicateNode::IsNonTerminalCall(_) | PredicateNode::StarImportPlaceholder(_) => {
            (None, None)
        }
    };

    if predicate.is_positive {
        constraints
    } else {
        (constraints.1, constraints.0)
    }
}

#[salsa::tracked(returns(as_ref), heap_size=ruff_memory_usage::heap_size)]
fn all_narrowing_constraints_for_pattern<'db>(
    db: &'db dyn Db,
    pattern: PatternPredicate<'db>,
) -> Option<FrozenNarrowingConstraints<'db>> {
    let module = parsed_module(db, pattern.file(db)).load(db);
    NarrowingConstraintsBuilder::new(db, &module, PredicateNode::Pattern(pattern), true).finish()
}

#[salsa::tracked(
    returns(ref),
    cycle_initial=|_, _, _| ExpressionNarrowingConstraints::default(),
    heap_size=ruff_memory_usage::heap_size,
)]
fn all_narrowing_constraints_for_expression<'db>(
    db: &'db dyn Db,
    expression: Expression<'db>,
) -> ExpressionNarrowingConstraints<'db> {
    let module = parsed_module(db, expression.file(db)).load(db);
    let predicate = PredicateNode::Expression(expression);
    ExpressionNarrowingConstraints {
        positive: NarrowingConstraintsBuilder::new(db, &module, predicate, true).finish(),
        negative: NarrowingConstraintsBuilder::new(db, &module, predicate, false).finish(),
    }
}

#[salsa::tracked(returns(as_ref), heap_size=ruff_memory_usage::heap_size)]
fn all_negative_narrowing_constraints_for_pattern<'db>(
    db: &'db dyn Db,
    pattern: PatternPredicate<'db>,
) -> Option<FrozenNarrowingConstraints<'db>> {
    let module = parsed_module(db, pattern.file(db)).load(db);
    NarrowingConstraintsBuilder::new(db, &module, PredicateNode::Pattern(pattern), false).finish()
}

#[salsa::tracked(returns(as_ref), heap_size=ruff_memory_usage::heap_size)]
fn all_narrowing_constraints_for_subject_element_pattern<'db>(
    db: &'db dyn Db,
    pattern: PatternPredicate<'db>,
    target: ExpressionNodeKey,
) -> Option<FrozenNarrowingConstraints<'db>> {
    let module = parsed_module(db, pattern.file(db)).load(db);
    NarrowingConstraintsBuilder::new(
        db,
        &module,
        PredicateNode::SubjectElementPattern(SubjectElementPatternPredicate { pattern, target }),
        true,
    )
    .finish()
}

/// Functions that can be used to narrow the type of a first argument using a "classinfo" second argument.
///
/// A "classinfo" argument is either a class or a tuple of classes, or a tuple of tuples of classes
/// (etc. for arbitrary levels of recursion)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ClassInfoConstraintFunction {
    /// `builtins.isinstance`
    IsInstance,
    /// `builtins.issubclass`
    IsSubclass,
}

impl ClassInfoConstraintFunction {
    /// Generate a constraint from the type of a `classinfo` argument to `isinstance` or `issubclass`.
    ///
    /// The `classinfo` argument can be a class literal, a tuple of (tuples of) class literals. PEP 604
    /// union types are not yet supported. Returns `None` if the `classinfo` argument has a wrong type.
    fn generate_constraint<'db>(
        self,
        db: &'db dyn Db,
        classinfo: Type<'db>,
        is_positive: bool,
    ) -> Option<Type<'db>> {
        let constraint_from_class_literal = |class: ClassLiteral<'db>| match self {
            ClassInfoConstraintFunction::IsInstance => {
                Type::instance(db, class.top_materialization(db))
            }
            ClassInfoConstraintFunction::IsSubclass => {
                SubclassOfType::from(db, class.top_materialization(db))
            }
        };

        match classinfo {
            Type::TypeAlias(alias) => {
                self.generate_constraint(db, alias.value_type(db), is_positive)
            }
            Type::ClassLiteral(class_literal) => Some(constraint_from_class_literal(class_literal)),
            Type::SubclassOf(subclass_of_ty) => {
                // We can't narrow negatively from a `SubclassOf` type. `if !isinstance(x, y)`
                // where `y: type[A]` doesn't ensure that `x` is not an instance of `A`, because
                // `y` could be some subclass of `A`.
                if !is_positive {
                    return None;
                }

                match subclass_of_ty.subclass_of() {
                    SubclassOfInner::Class(ClassType::NonGeneric(class_literal)) => {
                        Some(constraint_from_class_literal(class_literal))
                    }
                    // It's not valid to use a generic alias as the second argument to `isinstance()` or `issubclass()`,
                    // e.g. `isinstance(x, list[int])` fails at runtime.
                    SubclassOfInner::Class(ClassType::Generic(_)) => None,
                    SubclassOfInner::Dynamic(dynamic) => Some(Type::Dynamic(dynamic)),
                    SubclassOfInner::TypeVar(bound_typevar) => match self {
                        ClassInfoConstraintFunction::IsSubclass => Some(classinfo),
                        ClassInfoConstraintFunction::IsInstance => {
                            Some(Type::TypeVar(bound_typevar))
                        }
                    },
                }
            }
            Type::Dynamic(_) | Type::Divergent(_) | Type::Projection(_) => Some(classinfo),
            Type::Intersection(intersection) => {
                if intersection.negative(db).is_empty() {
                    let mut builder = IntersectionBuilder::new(db);
                    for element in intersection.positive(db) {
                        builder = builder.add_positive(self.generate_constraint(
                            db,
                            *element,
                            is_positive,
                        )?);
                    }
                    Some(builder.build())
                } else {
                    // TODO: can we do better here?
                    None
                }
            }
            Type::Union(union) => union.try_map(db, |element| {
                self.generate_constraint(db, *element, is_positive)
            }),
            Type::TypeVar(bound_typevar) => {
                match bound_typevar.typevar(db).bound_or_constraints(db)? {
                    TypeVarBoundOrConstraints::UpperBound(bound) => {
                        self.generate_constraint(db, bound, is_positive)
                    }
                    TypeVarBoundOrConstraints::Constraints(constraints) => {
                        self.generate_constraint(db, constraints.as_type(db), is_positive)
                    }
                }
            }

            // It's not valid to use a generic alias as the second argument to `isinstance()` or `issubclass()`,
            // e.g. `isinstance(x, list[int])` fails at runtime.
            Type::GenericAlias(_) => None,

            Type::NominalInstance(nominal) => nominal.tuple_spec(db).and_then(|tuple| {
                UnionType::try_from_elements(
                    db,
                    tuple
                        .iter_all_elements()
                        .map(|element| self.generate_constraint(db, element, is_positive)),
                )
            }),

            Type::KnownInstance(KnownInstanceType::UnionType(instance)) => {
                UnionType::try_from_elements(
                    db,
                    instance.value_expression_types(db).ok()?.map(|element| {
                        // A special case is made for `None` at runtime
                        // (it's implicitly converted to `NoneType` in `int | None`)
                        // which means that `isinstance(x, int | None)` works even though
                        // `None` is not a class literal.
                        if element.is_none(db) {
                            self.generate_constraint(
                                db,
                                KnownClass::NoneType.to_class_literal(db),
                                is_positive,
                            )
                        } else {
                            self.generate_constraint(db, element, is_positive)
                        }
                    }),
                )
            }

            Type::SpecialForm(form) => match form {
                SpecialFormType::LegacyStdlibAlias(alias) => self.generate_constraint(
                    db,
                    alias.aliased_class().to_class_literal(db),
                    is_positive,
                ),
                SpecialFormType::Tuple => self.generate_constraint(
                    db,
                    KnownClass::Tuple.to_class_literal(db),
                    is_positive,
                ),
                SpecialFormType::Type => {
                    self.generate_constraint(db, KnownClass::Type.to_class_literal(db), is_positive)
                }

                // We don't have a good meta-type for `Callable`s right now,
                // so only apply `isinstance()` narrowing, not `issubclass()`
                SpecialFormType::TypingCallable | SpecialFormType::CollectionsAbcCallable => (self
                    == ClassInfoConstraintFunction::IsInstance)
                    .then(|| Type::Callable(CallableType::unknown(db)).top_materialization(db)),

                // `InitVar` is a class at runtime, so can be used in `isinstance()`,
                // but we can't represent internally the type that we should narrow to after an `isinstance()` check,
                // so just intersect with `Any` in those cases.
                SpecialFormType::TypeQualifier(TypeQualifier::InitVar) => Some(Type::any()),

                _ => None,
            },

            Type::AlwaysFalsy
            | Type::AlwaysTruthy
            | Type::EnumComplement(_)
            | Type::LiteralValue(_)
            | Type::BoundMethod(_)
            | Type::BoundSuper(_)
            | Type::Callable(_)
            | Type::DataclassDecorator(_)
            | Type::Never
            | Type::KnownBoundMethod(_)
            | Type::ModuleLiteral(_)
            | Type::FunctionLiteral(_)
            | Type::ProtocolInstance(_)
            | Type::PropertyInstance(_)
            | Type::KnownInstance(_)
            | Type::TypeIs(_)
            | Type::TypeGuard(_)
            | Type::TypeForm(_)
            | Type::WrapperDescriptor(_)
            | Type::DataclassTransformer(_)
            | Type::TypedDict(_)
            | Type::NewTypeInstance(_) => None,
        }
    }
}

#[derive(Hash, PartialEq, Debug, Eq, Clone, salsa::Update, get_size2::GetSize)]
struct Conjunctions<'db> {
    conjuncts: SmallVec<[Type<'db>; 2]>,
}

impl<'db> Conjunctions<'db> {
    fn singleton(ty: Type<'db>) -> Self {
        Self {
            conjuncts: smallvec![ty],
        }
    }

    fn and_with(mut self, other: Self) -> Self {
        if self.conjuncts.iter().any(Type::is_never) || other.conjuncts.iter().any(Type::is_never) {
            return Self::singleton(Type::Never);
        }

        for conjunct in other.conjuncts {
            if !self.conjuncts.contains(&conjunct) {
                self.conjuncts.push(conjunct);
            }
        }
        self
    }

    fn evaluate_constraint_type(self, db: &'db dyn Db) -> Type<'db> {
        if self.conjuncts.len() == 1 {
            return self.conjuncts[0];
        }

        let mut intersection = IntersectionBuilder::new(db);
        for conjunct in self.conjuncts {
            intersection = intersection.add_positive(conjunct);
        }
        intersection.build()
    }
}

/// Represents narrowing constraints in Disjunctive Normal Form (DNF).
///
/// This is a disjunction (OR) of conjunctions (AND) of constraints.
/// The DNF representation allows us to properly track "replacement" constraints
/// (created by `TypeGuard` types and similar) through boolean operations.
///
/// For example:
/// - `f(x) and g(x)` where f returns `TypeIs[A]` and g returns `TypeGuard[B]`
///   => and
///   ===> `NarrowingConstraint { intersection_disjuncts: [A], replacement_disjuncts: [] }`
///   ===> `NarrowingConstraint { intersection_disjuncts: [], replacement_disjuncts: [B] }`
///   => `NarrowingConstraint { intersection_disjuncts: [], replacement_disjuncts: [B] }`
///   => evaluates to `B` (`TypeGuard` clobbers any previous type information)
///
/// - `f(x) or g(x)` where f returns `TypeIs[A]` and g returns `TypeGuard[B]`
///   => or
///   ===> `NarrowingConstraint { intersection_disjuncts: [A], replacement_disjuncts: [] }`
///   ===> `NarrowingConstraint { intersection_disjuncts: [], replacement_disjuncts: [B] }`
///   => `NarrowingConstraint { intersection_disjuncts: [A], replacement_disjuncts: [B] }`
///   => evaluates to `(P & A) | B`, where `P` is our previously-known type
#[derive(Hash, PartialEq, Debug, Eq, Clone, salsa::Update, get_size2::GetSize)]
pub(crate) struct NarrowingConstraint<'db> {
    /// Intersection constraint (from `isinstance()` narrowing comparisons, `TypeIs`, and
    /// similar). We keep these as a disjunction of conjunctions to avoid constructing
    /// union/intersection types while merging constraints.
    intersection_disjuncts: SmallVec<[Conjunctions<'db>; 1]>,

    /// "Replacement" constraints: instead of intersecting the previous type with a new type,
    /// the previous type is simply replaced wholesale with the new type. A common use case for
    /// these constraints is `typing.TypeGuard`. We can't eagerly union disjunctions because
    /// `TypeGuard` clobbers the previously-known type; within each replacement disjunct, however,
    /// we may eagerly intersect conjunctions with a later intersection narrowing.
    replacement_disjuncts: SmallVec<[Conjunctions<'db>; 1]>,
}

impl<'db> NarrowingConstraint<'db> {
    /// Create an "intersection" constraint: the previous type will be
    /// intersected with this constraint
    pub(crate) fn intersection(constraint: Type<'db>) -> Self {
        Self {
            intersection_disjuncts: smallvec_inline![Conjunctions::singleton(constraint)],
            replacement_disjuncts: smallvec![],
        }
    }

    /// Create a "replacement" constraint: the previous type will be
    /// replaced wholesale with this constraint
    fn replacement(constraint: Type<'db>) -> Self {
        Self {
            intersection_disjuncts: smallvec![],
            replacement_disjuncts: smallvec_inline![Conjunctions::singleton(constraint)],
        }
    }

    /// Merge two constraints, taking their intersection but respecting "replacement" semantics (with
    /// `other` winning)
    pub(crate) fn merge_constraint_and(&self, other: Self) -> Self {
        // Distribute AND over OR: (A1 | A2 | ...) AND (B1 | B2 | ...)
        // becomes (A1 & B1) | (A1 & B2) | ... | (A2 & B1) | ...
        //
        // In our representation, the RHS `replacement_disjuncts` will all clobber the LHS disjuncts
        // when they are `and`ed, so they'll just stay as is.
        //
        // The thing we actually need to deal with is the RHS `intersection_disjuncts`. Each RHS
        // disjunct gets intersected with each LHS disjunct, producing the cartesian product.
        // This is still deferred as conjunction lists.
        //
        // We also intersect each LHS `replacement_disjunct` with every RHS intersection disjunct
        // to form new additional `replacement_disjuncts`.
        if other.intersection_disjuncts.is_empty() {
            return other;
        }

        let mut new_intersection_disjuncts = smallvec![];
        for intersection_disjunct in &self.intersection_disjuncts {
            for other_intersection_disjunct in &other.intersection_disjuncts {
                let merged = intersection_disjunct
                    .clone()
                    .and_with(other_intersection_disjunct.clone());
                if !new_intersection_disjuncts.contains(&merged) {
                    new_intersection_disjuncts.push(merged);
                }
            }
        }

        let mut additional_replacement_disjuncts: SmallVec<[Conjunctions<'db>; 1]> = smallvec![];
        for replacement_disjunct in &self.replacement_disjuncts {
            for other_intersection_disjunct in &other.intersection_disjuncts {
                let merged = replacement_disjunct
                    .clone()
                    .and_with(other_intersection_disjunct.clone());
                if !additional_replacement_disjuncts.contains(&merged) {
                    additional_replacement_disjuncts.push(merged);
                }
            }
        }

        let mut new_replacement_disjuncts = other.replacement_disjuncts;

        new_replacement_disjuncts.extend(additional_replacement_disjuncts);

        NarrowingConstraint {
            intersection_disjuncts: new_intersection_disjuncts,
            replacement_disjuncts: new_replacement_disjuncts,
        }
    }

    /// Evaluate the type this effectively constrains to
    ///
    /// Forgets whether each constraint originated from a `replacement` disjunct or not
    pub(crate) fn evaluate_constraint_type(self, db: &'db dyn Db) -> Type<'db> {
        let mut union = UnionBuilder::new(db);
        for conjunctions in self
            .replacement_disjuncts
            .into_iter()
            .chain(self.intersection_disjuncts)
        {
            union = union.add(conjunctions.evaluate_constraint_type(db));
        }
        union.build()
    }
}

impl<'db> From<Type<'db>> for NarrowingConstraint<'db> {
    fn from(constraint: Type<'db>) -> Self {
        Self::intersection(constraint)
    }
}

type NarrowingConstraints<'db> = FxHashMap<ScopedPlaceId, NarrowingConstraint<'db>>;
type FrozenNarrowingConstraints<'db> = FrozenMap<ScopedPlaceId, NarrowingConstraint<'db>>;

/// The narrowing constraints contributed by a match pattern.
///
/// An impossible alternative is omitted from an OR pattern, while a possible alternative with no
/// constraints prevents the OR pattern from narrowing.
enum PatternNarrowingResult<'db> {
    Impossible,
    Possible(Option<NarrowingConstraints<'db>>),
}

impl<'db> PatternNarrowingResult<'db> {
    fn merge_alternatives(
        alternatives: impl Iterator<Item = Self>,
        merge_constraints: fn(
            Option<NarrowingConstraints<'db>>,
            Option<NarrowingConstraints<'db>>,
        ) -> Option<NarrowingConstraints<'db>>,
    ) -> Self {
        let mut alternatives = alternatives.filter_map(|alternative| match alternative {
            Self::Impossible => None,
            Self::Possible(constraints) => Some(constraints),
        });
        let Some(first) = alternatives.next() else {
            return Self::Impossible;
        };

        Self::Possible(alternatives.fold(first, merge_constraints))
    }

    fn into_constraints(self) -> Option<NarrowingConstraints<'db>> {
        match self {
            Self::Impossible | Self::Possible(None) => None,
            Self::Possible(Some(constraints)) => Some(constraints),
        }
    }
}

#[derive(Default, PartialEq, Eq, salsa::Update, get_size2::GetSize)]
struct ExpressionNarrowingConstraints<'db> {
    positive: Option<FrozenNarrowingConstraints<'db>>,
    negative: Option<FrozenNarrowingConstraints<'db>>,
}

impl<'db> ExpressionNarrowingConstraints<'db> {
    fn get(&self, place: ScopedPlaceId, is_positive: bool) -> Option<&NarrowingConstraint<'db>> {
        if is_positive {
            self.positive.as_ref()?.get(&place)
        } else {
            self.negative.as_ref()?.get(&place)
        }
    }
}

fn insert_narrowing_constraint<'db>(
    constraints: &mut NarrowingConstraints<'db>,
    place: ScopedPlaceId,
    constraint: NarrowingConstraint<'db>,
) {
    constraints
        .entry(place)
        .and_modify(|existing| {
            *existing = existing.merge_constraint_and(constraint.clone());
        })
        .or_insert(constraint);
}

/// Merge constraints with AND semantics (intersection/conjunction).
///
/// When we have `constraint1 & constraint2`, we need to distribute AND over the OR
/// in the DNF representations:
/// `(A | B) & (C | D)` becomes `(A & C) | (A & D) | (B & C) | (B & D)`
///
/// For each conjunction pair, we:
/// - Take the right conjunct if it has a `replacement`
/// - Intersect the constraints normally otherwise
fn merge_constraints_and<'db>(
    into: &mut NarrowingConstraints<'db>,
    from: NarrowingConstraints<'db>,
) {
    #[expect(
        clippy::iter_over_hash_type,
        reason = "constraints for distinct places are merged independently"
    )]
    for (key, from_constraint) in from {
        match into.entry(key) {
            Entry::Occupied(mut entry) => {
                let into_constraint = entry.get();

                entry.insert(into_constraint.merge_constraint_and(from_constraint));
            }
            Entry::Vacant(entry) => {
                entry.insert(from_constraint);
            }
        }
    }
}

/// Merge constraints with OR semantics (union/disjunction).
///
/// When we have `constraint1 OR constraint2`, we simply concatenate the disjuncts
/// from both constraints: `(A | B) OR (C | D)` becomes `A | B | C | D`
///
/// However, if a place appears in only one branch of the OR, we need to widen it
/// to `object` in the overall result (because the other branch doesn't constrain it).
fn merge_constraints_or<'db>(
    into: &mut NarrowingConstraints<'db>,
    from: NarrowingConstraints<'db>,
) {
    // For places that appear in `into` but not in `from`, widen to object
    into.retain(|key, _| from.contains_key(key));

    #[expect(
        clippy::iter_over_hash_type,
        reason = "constraints for distinct places are merged independently"
    )]
    for (key, from_constraint) in from {
        match into.entry(key) {
            Entry::Occupied(mut entry) => {
                let into_constraint = entry.get_mut();
                // Union the intersection constraints by concatenating disjunct lists.
                into_constraint
                    .intersection_disjuncts
                    .extend(from_constraint.intersection_disjuncts);

                // Concatenate replacement disjuncts
                into_constraint
                    .replacement_disjuncts
                    .extend(from_constraint.replacement_disjuncts);
            }
            Entry::Vacant(_) => {
                // Place only appears in `from`, not in `into`. No constraint needed.
            }
        }
    }
}

/// Return `true` if it is possible for any two inhabitants of the given types to
/// compare equal to each other; otherwise return `false`.
fn could_compare_equal<'db>(db: &'db dyn Db, left_ty: Type<'db>, right_ty: Type<'db>) -> bool {
    if !left_ty.is_disjoint_from(db, right_ty) {
        // If types overlap, they have inhabitants in common; it's definitely possible
        // for an object to compare equal to itself.
        return true;
    }

    if let Some(left_alternatives) = finite_single_valued_union_alternatives(db, left_ty) {
        return left_alternatives
            .into_iter()
            .any(|ty| could_compare_equal(db, ty, right_ty));
    }

    if let Some(right_alternatives) = finite_single_valued_union_alternatives(db, right_ty) {
        return right_alternatives
            .into_iter()
            .any(|ty| could_compare_equal(db, left_ty, ty));
    }

    match (left_ty, right_ty) {
        // In order to be sure a union type cannot compare equal to another type, it
        // must be true that no element of the union can compare equal to that type.
        (Type::Union(union), _) => union
            .elements(db)
            .iter()
            .any(|ty| could_compare_equal(db, *ty, right_ty)),
        (_, Type::Union(union)) => union
            .elements(db)
            .iter()
            .any(|ty| could_compare_equal(db, left_ty, *ty)),
        (Type::LiteralValue(left), Type::LiteralValue(right)) => {
            match (left.kind(), right.kind()) {
                // Boolean literals and int literals are disjoint, and single valued, and yet
                // `True == 1` and `False == 0`.
                (LiteralValueTypeKind::Bool(b), LiteralValueTypeKind::Int(i))
                | (LiteralValueTypeKind::Int(i), LiteralValueTypeKind::Bool(b)) => {
                    i64::from(b) == i.as_i64()
                }
                _ => !(left_ty.is_single_valued(db) && right_ty.is_single_valued(db)),
            }
        }
        // We assume that tuples use `tuple.__eq__` which only returns True
        // for other tuples, so they cannot compare equal to non-tuple types.
        (Type::NominalInstance(instance), _) if instance.tuple_spec(db).is_some() => false,
        (_, Type::NominalInstance(instance)) if instance.tuple_spec(db).is_some() => false,
        // Other than the above cases, two single-valued disjoint types cannot compare
        // equal.
        _ => !(left_ty.is_single_valued(db) && right_ty.is_single_valued(db)),
    }
}

fn is_exact_membership_value_domain<'db>(db: &'db dyn Db, ty: Type<'db>) -> bool {
    let ty = ty.resolve_type_alias(db);
    ty == Type::Never || ty.is_single_valued(db)
}

struct NarrowingConstraintsBuilder<'db, 'ast> {
    db: &'db dyn Db,
    module: &'ast ParsedModuleRef,
    predicate: PredicateNode<'db>,
    is_positive: bool,
}

impl<'db, 'ast> NarrowingConstraintsBuilder<'db, 'ast> {
    fn new(
        db: &'db dyn Db,
        module: &'ast ParsedModuleRef,
        predicate: PredicateNode<'db>,
        is_positive: bool,
    ) -> Self {
        Self {
            db,
            module,
            predicate,
            is_positive,
        }
    }

    fn finish(mut self) -> Option<FrozenNarrowingConstraints<'db>> {
        let constraints: Option<NarrowingConstraints<'db>> = match self.predicate {
            PredicateNode::Expression(expression) => {
                self.evaluate_expression_predicate(expression, self.is_positive)
            }
            PredicateNode::Pattern(pattern) => {
                self.evaluate_pattern_predicate(pattern, self.is_positive)
            }
            PredicateNode::SubjectElementPattern(subject_element) => {
                self.evaluate_subject_element_pattern(subject_element)
            }
            PredicateNode::IsNonTerminalCall(_) => return None,
            PredicateNode::StarImportPlaceholder(_) => return None,
        };

        constraints.map(FrozenNarrowingConstraints::from)
    }

    fn evaluate_expression_predicate(
        &mut self,
        expression: Expression<'db>,
        is_positive: bool,
    ) -> Option<NarrowingConstraints<'db>> {
        let expression_node = expression.node_ref(self.db).node(self.module);
        self.evaluate_expression_node_predicate(expression_node, expression, is_positive)
    }

    fn evaluate_expression_node_predicate(
        &mut self,
        expression_node: &ruff_python_ast::Expr,
        expression: Expression<'db>,
        is_positive: bool,
    ) -> Option<NarrowingConstraints<'db>> {
        match expression_node {
            ast::Expr::Name(_) => {
                let file = expression.file(self.db);
                let index = semantic_index(self.db, file);
                let constraints = self.evaluate_simple_expr(expression_node, is_positive);
                if let Some(alias_predicate) = index.narrowing_alias_predicate(expression_node) {
                    let aliased_constraints =
                        self.evaluate_expression_predicate(alias_predicate.expression, is_positive);
                    // For example, suppose we have an alias `is_none = x is None`.
                    // When this alias is used for narrowing, that is, within a block like `if is_none: ...`,
                    // both the constraint `is_none: Literal[True]` and the constraint `x: None` should be imposed.
                    // The former is `constraints` and the latter is `aliased_constraints`.
                    Self::merge_optional_constraints_and(constraints, aliased_constraints)
                } else {
                    constraints
                }
            }
            ast::Expr::Attribute(_) | ast::Expr::Subscript(_) => {
                self.evaluate_simple_expr(expression_node, is_positive)
            }
            ast::Expr::Compare(expr_compare) => {
                self.evaluate_expr_compare(expr_compare, expression, is_positive)
            }
            ast::Expr::Call(expr_call) => {
                self.evaluate_expr_call(expr_call, expression, is_positive)
            }
            ast::Expr::UnaryOp(unary_op) if unary_op.op == ast::UnaryOp::Not => {
                self.evaluate_expression_node_predicate(&unary_op.operand, expression, !is_positive)
            }
            ast::Expr::BoolOp(bool_op) => self.evaluate_bool_op(bool_op, expression, is_positive),
            ast::Expr::If(expr_if) => self.evaluate_expr_if(expr_if, expression, is_positive),
            ast::Expr::Named(expr_named) => self.evaluate_expr_named(expr_named, is_positive),
            _ => None,
        }
    }

    fn merge_optional_constraints_and(
        left: Option<NarrowingConstraints<'db>>,
        right: Option<NarrowingConstraints<'db>>,
    ) -> Option<NarrowingConstraints<'db>> {
        match (left, right) {
            (Some(mut left), Some(right)) => {
                merge_constraints_and(&mut left, right);
                Some(left)
            }
            (Some(left), None) => Some(left),
            (None, Some(right)) => Some(right),
            (None, None) => None,
        }
    }

    fn merge_optional_constraints_or(
        left: Option<NarrowingConstraints<'db>>,
        right: Option<NarrowingConstraints<'db>>,
    ) -> Option<NarrowingConstraints<'db>> {
        match (left, right) {
            (Some(mut left), Some(right)) => {
                merge_constraints_or(&mut left, right);
                Some(left)
            }
            _ => None,
        }
    }

    fn evaluate_expr_if(
        &mut self,
        expr_if: &ast::ExprIf,
        expression: Expression<'db>,
        is_positive: bool,
    ) -> Option<NarrowingConstraints<'db>> {
        let test_truthiness = infer_expression_types(self.db, expression, TypeContext::default())
            .expression_type(&expr_if.test)
            .bool(self.db);

        match test_truthiness {
            Truthiness::AlwaysTrue => {
                self.evaluate_expression_node_predicate(&expr_if.body, expression, is_positive)
            }
            Truthiness::AlwaysFalse => {
                self.evaluate_expression_node_predicate(&expr_if.orelse, expression, is_positive)
            }
            Truthiness::Ambiguous => {
                let body_constraints = Self::merge_optional_constraints_and(
                    self.evaluate_expression_node_predicate(&expr_if.test, expression, true),
                    self.evaluate_expression_node_predicate(&expr_if.body, expression, is_positive),
                );
                let orelse_constraints = Self::merge_optional_constraints_and(
                    self.evaluate_expression_node_predicate(&expr_if.test, expression, false),
                    self.evaluate_expression_node_predicate(
                        &expr_if.orelse,
                        expression,
                        is_positive,
                    ),
                );

                // `a if c else b` is equivalent to `(c and a) or (not c and b)`.
                Self::merge_optional_constraints_or(body_constraints, orelse_constraints)
            }
        }
    }

    fn evaluate_pattern_predicate_kind(
        &mut self,
        pattern_predicate_kind: &PatternPredicateKind<'db>,
        subject: Expression<'db>,
        is_positive: bool,
    ) -> PatternNarrowingResult<'db> {
        match pattern_predicate_kind {
            PatternPredicateKind::Singleton(singleton) => PatternNarrowingResult::Possible(
                self.evaluate_match_pattern_singleton(subject, *singleton, is_positive),
            ),
            PatternPredicateKind::Class(cls, kind) => PatternNarrowingResult::Possible(
                self.evaluate_match_pattern_class(subject, *cls, *kind, is_positive),
            ),
            PatternPredicateKind::Mapping(kind) => PatternNarrowingResult::Possible(
                self.evaluate_match_pattern_mapping(subject, *kind, is_positive),
            ),
            PatternPredicateKind::Sequence(kind) => {
                self.evaluate_match_pattern_sequence(subject, kind, is_positive)
            }
            PatternPredicateKind::Value(expr) => PatternNarrowingResult::Possible(
                self.evaluate_match_pattern_value(subject, *expr, is_positive),
            ),
            PatternPredicateKind::Or(predicates) => {
                self.evaluate_match_pattern_or(subject, predicates, is_positive)
            }
            PatternPredicateKind::As(Some(pattern), _) => {
                self.evaluate_pattern_predicate_kind(pattern, subject, is_positive)
            }
            PatternPredicateKind::As(None, _) | PatternPredicateKind::MatchStar => {
                PatternNarrowingResult::Possible(None)
            }
        }
    }

    fn evaluate_pattern_predicate(
        &mut self,
        pattern: PatternPredicate<'db>,
        is_positive: bool,
    ) -> Option<NarrowingConstraints<'db>> {
        self.evaluate_pattern_predicate_kind(
            pattern.kind(self.db),
            pattern.subject(self.db),
            is_positive,
        )
        .into_constraints()
    }

    fn evaluate_subject_element_pattern(
        &mut self,
        subject_element: SubjectElementPatternPredicate<'db>,
    ) -> Option<NarrowingConstraints<'db>> {
        let pattern = subject_element.pattern;
        let subject = pattern.subject(self.db).node_ref(self.db).node(self.module);
        self.evaluate_match_pattern_for_subject_element(
            subject,
            pattern.kind(self.db),
            Some(subject_element.target),
        )
        .into_constraints()
    }

    fn places(&self) -> &'db PlaceTable {
        place_table(self.db, self.scope())
    }

    fn scope(&self) -> ScopeId<'db> {
        match self.predicate {
            PredicateNode::Expression(expression) => expression.scope(self.db),
            PredicateNode::Pattern(pattern) => pattern.scope(self.db),
            PredicateNode::SubjectElementPattern(subject_element) => {
                subject_element.pattern.scope(self.db)
            }
            PredicateNode::IsNonTerminalCall(CallableAndCallExpr { callable, .. }) => {
                callable.scope(self.db)
            }
            PredicateNode::StarImportPlaceholder(definition) => definition.scope(self.db),
        }
    }

    #[track_caller]
    fn expect_place(&self, place_expr: &PlaceExpr) -> ScopedPlaceId {
        self.places()
            .place_id(place_expr)
            .expect("We should always have a place for every `PlaceExpr`")
    }

    /// Check if a type is directly narrowable by `len()` (without considering unions or intersections).
    ///
    /// In order for this to return `true`, we must know that the truthiness of the object returned by
    /// `len(obj)` will always be consistent with the truthiness of `obj` for all `obj`s of type `ty`.
    ///
    /// We know that this is true for:
    /// - Certain `Literal` types where we know that `__len__` is always well-behaved, and where we
    ///   know that the type cannot be subclassed (because it's a `Literal` type).
    /// - Tuple types (we generally assume that tuples have well-behaved `__len__` methods,
    ///   and much of our special-casing for tuples elsewhere depends on this assumption).
    /// - Arbitrary user types that return `Literal` types from both `__len__` and `__bool__`,
    ///   where the returned `Literal` types are mutually consistent in their truthiness.
    fn is_base_type_narrowable_by_len(db: &'db dyn Db, ty: Type<'db>) -> bool {
        match ty {
            Type::NominalInstance(instance) if instance.tuple_spec(db).is_some() => true,
            Type::LiteralValue(literal)
                if matches!(
                    literal.kind(),
                    LiteralValueTypeKind::String(_)
                        | LiteralValueTypeKind::LiteralString
                        | LiteralValueTypeKind::Bytes(_)
                ) =>
            {
                true
            }
            _ => ty.len(db).is_some_and(|len_ty| {
                let len_ty_bool = len_ty.bool(db);
                len_ty_bool != Truthiness::Ambiguous && len_ty_bool == ty.bool(db)
            }),
        }
    }

    /// Narrow a type based on `len()`, only narrowing the parts that are safe to narrow.
    ///
    /// For narrowable types (literals, tuples), we apply `~AlwaysFalsy` (positive) or
    /// `~AlwaysTruthy` (negative). For non-narrowable types, we return them unchanged.
    ///
    /// Returns `None` if no part of the type is narrowable.
    fn narrow_type_by_len(db: &'db dyn Db, ty: Type<'db>, is_positive: bool) -> Option<Type<'db>> {
        match ty {
            Type::Union(union) => {
                let mut has_narrowable = false;
                let narrowed_elements: Vec<_> = union
                    .elements(db)
                    .iter()
                    .map(|element| {
                        if let Some(narrowed) = Self::narrow_type_by_len(db, *element, is_positive)
                        {
                            has_narrowable = true;
                            narrowed
                        } else {
                            // Non-narrowable elements are kept unchanged.
                            *element
                        }
                    })
                    .collect();

                if has_narrowable {
                    Some(UnionType::from_elements(db, narrowed_elements))
                } else {
                    None
                }
            }
            Type::Intersection(intersection) => {
                // For intersections, check if any positive element is narrowable.
                let positive = intersection.positive(db);
                let has_narrowable = positive
                    .iter()
                    .any(|element| Self::is_base_type_narrowable_by_len(db, *element));

                if has_narrowable {
                    // Apply the narrowing constraint to the whole intersection.
                    let mut builder = IntersectionBuilder::new(db).add_positive(ty);
                    if is_positive {
                        builder = builder.add_negative(Type::AlwaysFalsy);
                    } else {
                        builder = builder.add_negative(Type::AlwaysTruthy);
                    }
                    Some(builder.build())
                } else {
                    None
                }
            }
            _ if Self::is_base_type_narrowable_by_len(db, ty) => {
                let mut builder = IntersectionBuilder::new(db).add_positive(ty);
                if is_positive {
                    builder = builder.add_negative(Type::AlwaysFalsy);
                } else {
                    builder = builder.add_negative(Type::AlwaysTruthy);
                }
                Some(builder.build())
            }
            _ => None,
        }
    }

    /// Filter a type based on an equality or inequality comparison against an exact length.
    ///
    /// Exact tuple types are specialized to the observed length. Other types that encode their
    /// possible lengths are filtered. Unknown-length types are left unchanged because persisting
    /// an observed length would become stale after mutation.
    fn narrow_type_by_exact_len(
        db: &'db dyn Db,
        ty: Type<'db>,
        length: usize,
        is_equality: bool,
    ) -> Type<'db> {
        let resolved = ty.resolve_type_alias(db);

        let narrowed = match resolved {
            Type::Union(union) => union.map(db, |element| {
                Self::narrow_type_by_exact_len(db, *element, length, is_equality)
            }),
            Type::Intersection(intersection) => intersection.map_positive(db, |element| {
                Self::narrow_type_by_exact_len(db, *element, length, is_equality)
            }),
            _ => {
                if is_equality && let Some(tuple) = resolved.exact_tuple_instance_spec(db) {
                    match tuple.resize(db, TupleLength::Fixed(length)) {
                        Ok(tuple) => Type::tuple(TupleType::new(db, &tuple)),
                        Err(_) => Type::Never,
                    }
                } else {
                    let tuple_length = resolved
                        .as_nominal_instance()
                        .and_then(|instance| instance.tuple_spec(db))
                        .map(|spec| spec.len());
                    let satisfies_comparison = |length_type: Type<'db>| {
                        length_type
                            .as_int_literal()
                            .and_then(|actual| usize::try_from(actual).ok())
                            .is_some_and(|actual| (actual == length) == is_equality)
                    };
                    let comparison_possible = resolved
                        .len(db)
                        .map(|length_type| match length_type {
                            Type::Union(union) => union
                                .elements(db)
                                .iter()
                                .any(|element| satisfies_comparison(*element)),
                            _ => satisfies_comparison(length_type),
                        })
                        .or_else(|| {
                            tuple_length
                                .and_then(TupleLength::into_fixed_length)
                                .map(|actual| (actual == length) == is_equality)
                        });

                    match comparison_possible {
                        Some(false) => Type::Never,
                        None if is_equality
                            && tuple_length
                                .is_some_and(|tuple_length| length < tuple_length.minimum()) =>
                        {
                            Type::Never
                        }
                        _ => resolved,
                    }
                }
            }
        };

        if narrowed == resolved { ty } else { narrowed }
    }

    fn evaluate_simple_expr(
        &mut self,
        expr: &ast::Expr,
        is_positive: bool,
    ) -> Option<NarrowingConstraints<'db>> {
        let target = PlaceExpr::try_from_expr(expr)?;
        let place = self.expect_place(&target);

        let ty = if is_positive {
            Type::AlwaysFalsy.negate(self.db)
        } else {
            Type::AlwaysTruthy.negate(self.db)
        };

        Some(NarrowingConstraints::from_iter([(
            place,
            NarrowingConstraint::intersection(ty),
        )]))
    }

    fn evaluate_expr_named(
        &mut self,
        expr_named: &ast::ExprNamed,
        is_positive: bool,
    ) -> Option<NarrowingConstraints<'db>> {
        let target_constraints = self.evaluate_simple_expr(&expr_named.target, is_positive);
        let value_constraints = self.evaluate_simple_expr(&expr_named.value, is_positive);
        match (target_constraints, value_constraints) {
            (Some(mut target), Some(value)) => {
                merge_constraints_and(&mut target, value);
                Some(target)
            }
            (Some(constraints), None) | (None, Some(constraints)) => Some(constraints),
            (None, None) => None,
        }
    }

    fn exact_fixed_length_membership_values(&self, rhs_ty: Type<'db>) -> Option<Type<'db>> {
        let iterable = rhs_ty.try_iterate(self.db).ok()?;
        let fixed_length = iterable.as_fixed_length()?;
        let mut builder = UnionBuilder::new(self.db);

        for element_ty in fixed_length.all_elements().iter().copied() {
            if is_exact_membership_value_domain(self.db, element_ty) {
                builder = builder.add(element_ty);
            }
        }

        builder.try_build()
    }

    // TODO `expr_in` and `expr_not_in` should perhaps be unified with `expr_eq` and `expr_ne`,
    // since `eq` and `ne` are equivalent to `in` and `not in` with only one element in the RHS.
    fn evaluate_expr_in(&mut self, lhs_ty: Type<'db>, rhs_ty: Type<'db>) -> Option<Type<'db>> {
        let lhs_ty = lhs_ty.resolve_type_alias(self.db);

        if is_union_of_single_valued(self.db, lhs_ty) {
            rhs_ty
                .try_iterate(self.db)
                .ok()
                .map(|iterable| iterable.homogeneous_element_type(self.db))
        } else if is_union_with_single_valued(self.db, lhs_ty) {
            let rhs_values = rhs_ty
                .try_iterate(self.db)
                .ok()?
                .homogeneous_element_type(self.db);

            let mut builder = UnionBuilder::new(self.db);

            // Add the narrowed values from the RHS first, to keep literals before broader types.
            builder = builder.add(rhs_values);

            if let Some(lhs_union) = lhs_ty.as_union() {
                for element in lhs_union.elements(self.db) {
                    // Skip types that are handled specially by RHS matching.
                    if is_single_valued_union_component(self.db, *element) {
                        continue;
                    }
                    // Skip types that cannot compare equal to any RHS value.
                    if !could_compare_equal(self.db, *element, rhs_values) {
                        continue;
                    }
                    builder = builder.add(*element);
                }
            }
            Some(builder.build())
        } else {
            None
        }
    }

    fn evaluate_expr_not_in(&mut self, lhs_ty: Type<'db>, rhs_ty: Type<'db>) -> Option<Type<'db>> {
        let lhs_ty = lhs_ty.resolve_type_alias(self.db);
        let rhs_values = self.exact_fixed_length_membership_values(rhs_ty)?;

        if is_union_of_single_valued(self.db, lhs_ty) {
            // Exclude the RHS values from the entire (single-valued) LHS domain.
            let complement = IntersectionBuilder::new(self.db)
                .add_positive(lhs_ty)
                .add_negative(rhs_values)
                .build();
            Some(complement)
        } else if is_union_with_single_valued(self.db, lhs_ty) {
            // Split LHS into single-valued portion and the rest. Exclude RHS values from the
            // single-valued portion, keep the rest intact.
            let mut single_builder = UnionBuilder::new(self.db);
            let mut rest_builder = UnionBuilder::new(self.db);

            if let Some(lhs_union) = lhs_ty.as_union() {
                for element in lhs_union.elements(self.db) {
                    if is_single_valued_union_component(self.db, *element) {
                        single_builder = single_builder.add(*element);
                    } else {
                        rest_builder = rest_builder.add(*element);
                    }
                }
            }

            let single_union = single_builder.build();
            let rest_union = rest_builder.build();

            let narrowed_single = IntersectionBuilder::new(self.db)
                .add_positive(single_union)
                .add_negative(rhs_values)
                .build();

            // Keep order: first literal complement, then broader arms.
            let result = UnionType::from_two_elements(self.db, narrowed_single, rest_union);
            Some(result)
        } else {
            None
        }
    }

    fn evaluate_expr_compare_op(
        &mut self,
        lhs_ty: Type<'db>,
        rhs_ty: Type<'db>,
        op: ast::CmpOp,
        is_positive: bool,
    ) -> Option<Type<'db>> {
        if op == ast::CmpOp::Eq {
            return evaluate_type_equality(self.db, lhs_ty, rhs_ty, is_positive);
        }
        if op == ast::CmpOp::NotEq {
            return evaluate_type_inequality(self.db, lhs_ty, rhs_ty, is_positive);
        }

        let op = if is_positive { op } else { op.negate() };

        match op {
            ast::CmpOp::IsNot => {
                if rhs_ty.is_singleton(self.db) {
                    Some(rhs_ty.negate(self.db))
                } else {
                    // Non-singletons cannot be safely narrowed using `is not`
                    None
                }
            }
            ast::CmpOp::Is => Some(rhs_ty),
            ast::CmpOp::In => self.evaluate_expr_in(lhs_ty, rhs_ty),
            ast::CmpOp::NotIn => self.evaluate_expr_not_in(lhs_ty, rhs_ty),
            _ => None,
        }
    }

    fn evaluate_expr_compare(
        &mut self,
        expr_compare: &ast::ExprCompare,
        expression: Expression<'db>,
        is_positive: bool,
    ) -> Option<NarrowingConstraints<'db>> {
        fn is_narrowing_target_candidate(expr: &ast::Expr) -> bool {
            matches!(
                expr,
                ast::Expr::Name(_)
                    | ast::Expr::Attribute(_)
                    | ast::Expr::Subscript(_)
                    | ast::Expr::Call(_)
                    | ast::Expr::Named(_)
            )
        }

        fn narrowable_ast(expr: &ast::Expr) -> bool {
            matches!(
                expr,
                ast::Expr::Name(_)
                    | ast::Expr::Attribute(_)
                    | ast::Expr::Subscript(_)
                    | ast::Expr::Named(_)
            )
        }

        /// Attempt to find an underlying class literal for purposes of `if type(x) is Y` narrowing.
        ///
        /// We deliberately return `None` for generic-alias types, since narrowing based
        /// on `if type(x) is Y[int]` isn't valid (this expression will never return `true`
        /// at runtime). Similarly, we return `None` for `type[Y[int]]`, type variables
        /// bound to `type[Y[int]]`, and type aliases where the underlying value is a
        /// generic class.
        fn find_underlying_class<'db>(db: &'db dyn Db, ty: Type<'db>) -> Option<ClassLiteral<'db>> {
            match ty {
                Type::ClassLiteral(class) => Some(class),
                Type::SubclassOf(subclass_of) => {
                    match subclass_of.subclass_of().with_transposed_type_var(db) {
                        SubclassOfInner::Class(ClassType::NonGeneric(class)) => Some(class),
                        SubclassOfInner::Class(ClassType::Generic(_))
                        | SubclassOfInner::Dynamic(_) => None,
                        SubclassOfInner::TypeVar(tvar) => {
                            find_underlying_class(db, tvar.typevar(db).upper_bound(db)?)
                        }
                    }
                }
                Type::TypeVar(tvar) => find_underlying_class(db, tvar.typevar(db).upper_bound(db)?),
                Type::TypeAlias(alias) => find_underlying_class(db, alias.value_type(db)),
                _ => None,
            }
        }

        /// Return the expression being tested by an exact runtime-class check.
        ///
        /// `x.__class__` is modeled as equivalent to `type(x)` by [`Type::dunder_class`], so class
        /// identity checks against either expression can narrow `x`.
        fn exact_class_narrowing_target<'a, 'db>(
            db: &'db dyn Db,
            inference: &ExpressionInference<'db>,
            expr: &'a ast::Expr,
        ) -> Option<&'a ast::Expr> {
            match expr.expression_value() {
                ast::Expr::Call(ast::ExprCall {
                    func,
                    arguments: ast::Arguments { args, keywords, .. },
                    ..
                }) => {
                    if keywords.is_empty()
                        && let [single_argument] = &**args
                        && let Type::ClassLiteral(called_class) = inference.expression_type(func)
                        && called_class.is_known(db, KnownClass::Type)
                    {
                        Some(single_argument)
                    } else {
                        None
                    }
                }
                ast::Expr::Attribute(ast::ExprAttribute { value, attr, .. })
                    if attr.as_str() == "__class__" =>
                {
                    Some(value)
                }
                _ => None,
            }
        }

        let ast::ExprCompare {
            range: _,
            node_index: _,
            left,
            ops,
            comparators,
        } = expr_compare;

        // Performance optimization: early return if there are no potential narrowing targets.
        if !is_narrowing_target_candidate(left)
            && comparators
                .iter()
                .all(|c| !is_narrowing_target_candidate(c))
        {
            return None;
        }

        if !is_positive && comparators.len() > 1 {
            // We can't negate a constraint made by a multi-comparator expression, since we can't
            // know which comparison part is the one being negated.
            // For example, the negation of  `x is 1 is y is 2`, would be `(x is not 1) or (y is not 1) or (y is not 2)`
            // and that requires cross-symbol constraints, which we don't support yet.
            return None;
        }

        let inference = infer_expression_types(self.db, expression, TypeContext::default());

        let comparator_tuples = std::iter::once(&**left)
            .chain(comparators)
            .tuple_windows::<(&ruff_python_ast::Expr, &ruff_python_ast::Expr)>();
        let mut constraints = NarrowingConstraints::default();

        // Narrow unions of tuples based on element checks. For example:
        //
        //     def _(t: tuple[int, int] | tuple[None, None]):
        //         if t[0] is not None:
        //             reveal_type(t)  # tuple[int, int]
        if matches!(&**ops, [ast::CmpOp::Is | ast::CmpOp::IsNot])
            && let ast::Expr::Subscript(subscript) = left.expression_value()
            && let Type::Union(union) = inference
                .expression_type(&*subscript.value)
                .resolve_type_alias(self.db)
            && let Some(subscript_place_expr) = PlaceExpr::try_from_expr(&subscript.value)
            && let Some(index) = inference
                .expression_type(&*subscript.slice)
                .as_int_literal()
            && let Ok(index) = i32::try_from(index)
            && let rhs_ty = inference.expression_type(&comparators[0])
            && rhs_ty.is_singleton(self.db)
        {
            let is_positive_check = is_positive == (ops[0] == ast::CmpOp::Is);
            let filtered = union.filter(self.db, |elem| {
                elem.as_nominal_instance()
                    .and_then(|inst| inst.tuple_spec(self.db))
                    .and_then(|spec| spec.py_index(self.db, index).ok())
                    .is_none_or(|el_ty| {
                        if is_positive_check {
                            // `is X` context: keep tuples where element could be X
                            !el_ty.is_disjoint_from(self.db, rhs_ty)
                        } else {
                            // `is not X` context: keep tuples where element is not always X
                            !el_ty.is_subtype_of(self.db, rhs_ty)
                        }
                    })
            });
            if filtered != Type::Union(union) {
                let place = self.expect_place(&subscript_place_expr);
                constraints.insert(place, NarrowingConstraint::replacement(filtered));
            }
        }

        // Narrow tagged unions of `TypedDict`s with `Literal` keys, for example:
        //
        //     class Foo(TypedDict):
        //         tag: Literal["foo"]
        //     class Bar(TypedDict):
        //         tag: Literal["bar"]
        //     def _(union: Foo | Bar):
        //         if union["tag"] == "foo":
        //             reveal_type(union)  # Foo
        //
        // Importantly, `my_typeddict_union["tag"]` isn't the place we're going to constrain.
        // Instead, we're going to constrain `my_typeddict_union` itself.
        if matches!(&**ops, [ast::CmpOp::Eq | ast::CmpOp::NotEq]) {
            // For `==`, we use equality semantics on the `if` branch (is_positive=true).
            // For `!=`, we use equality semantics on the `else` branch (is_positive=false).
            let is_equality = is_positive == (ops[0] == ast::CmpOp::Eq);

            let mut narrow_len_call = |call: &ast::ExprCall, length_type: Type<'db>| {
                let Type::FunctionLiteral(function_type) = inference.expression_type(&*call.func)
                else {
                    return;
                };
                if function_type.known(self.db) != Some(KnownFunction::Len)
                    || !call.arguments.keywords.is_empty()
                {
                    return;
                }
                let [arg] = &*call.arguments.args else {
                    return;
                };
                let Some(length_literal) = length_type
                    .resolve_type_alias(self.db)
                    .as_int_like_literal()
                else {
                    return;
                };
                let Ok(length) = usize::try_from(length_literal) else {
                    return;
                };
                let Some(target) = PlaceExpr::try_from_expr(arg) else {
                    return;
                };

                let arg_type = inference.expression_type(arg);
                let narrowed =
                    Self::narrow_type_by_exact_len(self.db, arg_type, length, is_equality);
                if narrowed != arg_type {
                    insert_narrowing_constraint(
                        &mut constraints,
                        self.expect_place(&target),
                        NarrowingConstraint::replacement(narrowed),
                    );
                }
            };

            // E.g., `len(items) == 2`
            if let ast::Expr::Call(call) = left.expression_value() {
                narrow_len_call(call, inference.expression_type(&comparators[0]));
            }

            // E.g., `2 == len(items)`
            if let ast::Expr::Call(call) = comparators[0].expression_value() {
                narrow_len_call(call, inference.expression_type(&**left));
            }

            let mut narrow_subscript = |subscript: &ast::ExprSubscript, other_type: Type<'db>| {
                let value_type = inference.expression_type(&*subscript.value);
                let slice_type = inference.expression_type(&*subscript.slice);

                if let Some((place, constraint)) = self.narrow_typeddict_subscript(
                    value_type,
                    &subscript.value,
                    slice_type,
                    other_type,
                    is_equality,
                ) {
                    insert_narrowing_constraint(&mut constraints, place, constraint);
                } else if let Some((place, constraint)) = self.narrow_tuple_subscript(
                    value_type,
                    &subscript.value,
                    slice_type,
                    other_type,
                    is_equality,
                ) {
                    insert_narrowing_constraint(&mut constraints, place, constraint);
                }
            };

            if let ast::Expr::Subscript(subscript) = left.expression_value() {
                narrow_subscript(subscript, inference.expression_type(&comparators[0]));
            }

            if let ast::Expr::Subscript(subscript) = comparators[0].expression_value() {
                narrow_subscript(subscript, inference.expression_type(&**left));
            }

            let mut narrow_attribute = |attribute: &ast::ExprAttribute, other_type: Type<'db>| {
                let value_type = inference.expression_type(&*attribute.value);

                if let Some((place, constraint)) = self.narrow_nominal_attribute(
                    value_type,
                    &attribute.value,
                    attribute.attr.id(),
                    other_type,
                    is_equality,
                ) {
                    insert_narrowing_constraint(&mut constraints, place, constraint);
                }
            };

            if let ast::Expr::Attribute(attribute) = &**left {
                narrow_attribute(attribute, inference.expression_type(&comparators[0]));
            }

            if let ast::Expr::Attribute(attribute) = &comparators[0] {
                narrow_attribute(attribute, inference.expression_type(&**left));
            }
        }

        // Narrow types when a key membership test proves that a key is present, and narrow unions
        // and intersections of `TypedDict` when a key membership test proves that a required key is
        // absent:
        //
        // class Foo(TypedDict):
        //     foo: int
        // class Bar(TypedDict):
        //     bar: int
        //
        // def _(u: Foo | Bar):
        //     if "foo" not in u:
        //         reveal_type(u)  # revealed: Bar
        if matches!(&**ops, [ast::CmpOp::In | ast::CmpOp::NotIn])
            && let Some(key) = inference.expression_type(&**left).as_string_literal()
            && let rhs_expr = comparators[0].expression_value()
            && let rhs_type = inference.expression_type(&comparators[0])
            && is_or_contains_typeddict(self.db, rhs_type)
        {
            let key = key.value(self.db);
            let apply_constraint =
                |constraints: &mut NarrowingConstraints<'db>,
                 constraint: NarrowingConstraint<'db>| {
                    let comparator_place = PlaceExpr::try_from_expr(&comparators[0])
                        .and_then(|place_expr| self.places().place_id(&place_expr));
                    if let Some(place) = comparator_place {
                        constraints.insert(place, constraint.clone());
                    }

                    let value_place = PlaceExpr::try_from_expr(rhs_expr)
                        .and_then(|place_expr| self.places().place_id(&place_expr));
                    if value_place != comparator_place
                        && let Some(place) = value_place
                    {
                        constraints.insert(place, constraint);
                    }
                };

            if is_positive == (ops[0] == ast::CmpOp::In) {
                let narrowed = self.narrow_with_present_key(rhs_type, key);
                if narrowed != rhs_type.resolve_type_alias(self.db) {
                    apply_constraint(&mut constraints, NarrowingConstraint::replacement(narrowed));
                }
            } else {
                let requires_key = |td: TypedDictType<'db>| -> bool {
                    td.items(self.db)
                        .get(key)
                        .is_some_and(TypedDictField::is_required)
                };

                let resolved_rhs_type = rhs_type.resolve_type_alias(self.db);

                let narrowed = match resolved_rhs_type {
                    Type::TypedDict(td) => {
                        if requires_key(td) {
                            Type::Never
                        } else {
                            resolved_rhs_type
                        }
                    }
                    Type::Intersection(intersection) => {
                        if intersection
                            .positive(self.db)
                            .iter()
                            .copied()
                            .filter_map(Type::as_typed_dict)
                            .any(requires_key)
                        {
                            Type::Never
                        } else {
                            resolved_rhs_type
                        }
                    }
                    Type::Union(union) => {
                        // remove all members of the union that would require the key
                        union.filter(self.db, |ty| match ty {
                            Type::TypedDict(td) => !requires_key(*td),
                            Type::Intersection(intersection) => !intersection
                                .positive(self.db)
                                .iter()
                                .copied()
                                .filter_map(Type::as_typed_dict)
                                .any(requires_key),
                            _ => true,
                        })
                    }
                    _ => resolved_rhs_type,
                };

                if narrowed != resolved_rhs_type {
                    apply_constraint(&mut constraints, NarrowingConstraint::replacement(narrowed));
                }
            }
        }

        let mut last_rhs_ty: Option<Type> = None;

        for (op, (left, right)) in std::iter::zip(&**ops, comparator_tuples) {
            let lhs_ty = last_rhs_ty.unwrap_or_else(|| inference.expression_type(left));
            let rhs_ty = inference.expression_type(right);

            // Narrowing for:
            // - `if type(x) is Y`
            // - `if type(x) is not Y`
            // - `if Y is type(x)`
            // - `if Y is not type(x)`
            // - `if type(x) is type(y)`
            // - `if type(x) is not type(y)`
            // - `if x.__class__ is Y`
            // - `if x.__class__ is not Y`
            // - `if Y is x.__class__`
            // - `if Y is not x.__class__`
            // - `if x.__class__ is y.__class__`
            // - `if x.__class__ is not y.__class__`
            let exact_class_checks = match (
                exact_class_narrowing_target(self.db, inference, left),
                exact_class_narrowing_target(self.db, inference, right),
            ) {
                (Some(left_target), Some(right_target)) => {
                    [Some((left_target, rhs_ty)), Some((right_target, lhs_ty))]
                }
                (Some(target), None) => [Some((target, rhs_ty)), None],
                (None, Some(target)) => [Some((target, lhs_ty)), None],
                (None, None) => [None, None],
            };
            for (target_expr, other) in exact_class_checks.into_iter().flatten() {
                // If this is `None`, it indicates that we cannot do `if type(x) is Y`
                // narrowing: we can only do narrowing for `if type(x) is Y` and
                // `if type(x) is not Y`, not for `if type(x) == Y` or `if type(x) != Y`.
                let is_positive = match op {
                    ast::CmpOp::Is => Some(is_positive),
                    ast::CmpOp::IsNot => Some(!is_positive),
                    _ => None,
                };

                if let Some(is_positive) = is_positive
                    && let Some(target) = PlaceExpr::try_from_expr(target_expr)
                    && let Some(other_class) = find_underlying_class(self.db, other)
                    // `else`-branch narrowing for `if type(x) is Y` can only be done
                    // if `Y` is a final class
                    && (is_positive || other_class.is_final(self.db))
                {
                    let place = self.expect_place(&target);
                    constraints.insert(
                        place,
                        NarrowingConstraint::intersection(
                            Type::instance(self.db, other_class.top_materialization(self.db))
                                .negate_if(self.db, !is_positive),
                        ),
                    );
                }
            }

            // Left-hand-side narrowing for:
            // - `if x == y`
            // - `if x != y`
            // - `if x is y`
            // - `if x is not y`
            // - `if x in y`
            // - `if x not in y`
            if narrowable_ast(left)
                && let Some(narrowable) = PlaceExpr::try_from_expr(left)
                && let Some(ty) = self.evaluate_expr_compare_op(lhs_ty, rhs_ty, *op, is_positive)
            {
                let place = self.expect_place(&narrowable);
                let constraint = NarrowingConstraint::intersection(ty);
                constraints
                    .entry(place)
                    .and_modify(|existing| {
                        *existing = existing.merge_constraint_and(constraint.clone());
                    })
                    .or_insert(constraint);
            }

            // Right-hand side narrowing for:
            // - `if y == x`
            // - `if y != x`
            // - `if y is x`
            // - `if y is not x`
            //
            // `in` and `not in` are not symmetric, so we don't narrow the right-hand side.
            if !matches!(op, ast::CmpOp::In | ast::CmpOp::NotIn)
                && narrowable_ast(right)
                && let Some(narrowable) = PlaceExpr::try_from_expr(right)
                && let Some(ty) = self.evaluate_expr_compare_op(rhs_ty, lhs_ty, *op, is_positive)
            {
                let place = self.expect_place(&narrowable);
                let constraint = NarrowingConstraint::intersection(ty);
                constraints
                    .entry(place)
                    .and_modify(|existing| {
                        *existing = existing.merge_constraint_and(constraint.clone());
                    })
                    .or_insert(constraint);

                // Use the narrowed type for subsequent comparisons in a chain.
                last_rhs_ty = Some(IntersectionType::from_two_elements(self.db, rhs_ty, ty));
            } else {
                last_rhs_ty = Some(rhs_ty);
            }
        }
        Some(constraints)
    }

    fn evaluate_expr_call(
        &mut self,
        expr_call: &ast::ExprCall,
        expression: Expression<'db>,
        is_positive: bool,
    ) -> Option<NarrowingConstraints<'db>> {
        let inference = infer_expression_types(self.db, expression, TypeContext::default());

        if let Some(type_guard_call_constraints) =
            self.evaluate_type_guard_call(inference, expr_call, is_positive)
        {
            return Some(type_guard_call_constraints);
        }

        let callable_ty = inference.expression_type(&*expr_call.func);

        match callable_ty {
            // For the expression `len(E)`, we narrow the type based on whether len(E) is truthy
            // (i.e., whether E is non-empty). We only narrow the parts of the type where we know
            // `__bool__` and `__len__` are consistent (literals, tuples). Non-narrowable parts
            // (str, list, etc.) are kept unchanged.
            Type::FunctionLiteral(function_type)
                if expr_call.arguments.args.len() == 1
                    && expr_call.arguments.keywords.is_empty()
                    && function_type.known(self.db) == Some(KnownFunction::Len) =>
            {
                let arg = &expr_call.arguments.args[0];
                let arg_ty = inference.expression_type(arg);

                // Narrow only the parts of the type that are safe to narrow based on len().
                if let Some(narrowed_ty) = Self::narrow_type_by_len(self.db, arg_ty, is_positive) {
                    let target = PlaceExpr::try_from_expr(arg)?;
                    let place = self.expect_place(&target);
                    Some(NarrowingConstraints::from_iter([(
                        place,
                        NarrowingConstraint::intersection(narrowed_ty),
                    )]))
                } else {
                    None
                }
            }
            Type::FunctionLiteral(function_type) if expr_call.arguments.keywords.is_empty() => {
                let [first_arg, second_arg] = &*expr_call.arguments.args else {
                    return None;
                };
                let first_arg = PlaceExpr::try_from_expr(first_arg)?;
                let function = function_type.known(self.db)?;
                let place = self.expect_place(&first_arg);

                if function == KnownFunction::HasAttr {
                    let attr = inference
                        .expression_type(second_arg)
                        .as_string_literal()?
                        .value(self.db);

                    if !is_identifier(attr) {
                        return None;
                    }

                    // Since `hasattr` only checks if an attribute is readable,
                    // the type of the protocol member should be a read-only property that returns `object`.
                    let constraint =
                        Type::protocol_with_readonly_members(self.db, [(attr, Type::object())]);

                    return Some(NarrowingConstraints::from_iter([(
                        place,
                        NarrowingConstraint::intersection(
                            constraint.negate_if(self.db, !is_positive),
                        ),
                    )]));
                }

                let function = function.into_classinfo_constraint_function()?;

                let class_info_ty = inference.expression_type(second_arg);

                function
                    .generate_constraint(self.db, class_info_ty, is_positive)
                    .map(|constraint| {
                        NarrowingConstraints::from_iter([(
                            place,
                            NarrowingConstraint::intersection(
                                constraint.negate_if(self.db, !is_positive),
                            ),
                        )])
                    })
            }
            // for the expression `bool(E)`, we further narrow the type based on `E`
            Type::ClassLiteral(class_type)
                if expr_call.arguments.args.len() == 1
                    && expr_call.arguments.keywords.is_empty()
                    && class_type.is_known(self.db, KnownClass::Bool) =>
            {
                self.evaluate_expression_node_predicate(
                    &expr_call.arguments.args[0],
                    expression,
                    is_positive,
                )
            }
            _ => None,
        }
    }

    // Helper to evaluate TypeGuard/TypeIs narrowing for a call expression.
    // This is based on the call expression's return type, so it applies to any callable type.
    fn evaluate_type_guard_call(
        &mut self,
        inference: &ExpressionInference<'db>,
        expr_call: &ast::ExprCall,
        is_positive: bool,
    ) -> Option<NarrowingConstraints<'db>> {
        let return_ty = inference.expression_type(expr_call);

        let place_and_constraint = match return_ty {
            Type::TypeIs(type_is) => {
                let (_, place) = type_is.place_info(self.db)?;
                Some((
                    place,
                    NarrowingConstraint::intersection(
                        type_is
                            .return_type(self.db)
                            .negate_if(self.db, !is_positive),
                    ),
                ))
            }
            // TypeGuard only narrows in the positive case
            Type::TypeGuard(type_guard) if is_positive => {
                let (_, place) = type_guard.place_info(self.db)?;
                Some((
                    place,
                    NarrowingConstraint::replacement(type_guard.return_type(self.db)),
                ))
            }
            _ => None,
        }?;

        Some(NarrowingConstraints::from_iter([place_and_constraint]))
    }

    fn evaluate_match_pattern_singleton(
        &mut self,
        subject: Expression<'db>,
        singleton: ast::Singleton,
        is_positive: bool,
    ) -> Option<NarrowingConstraints<'db>> {
        let subject = PlaceExpr::try_from_expr(subject.node_ref(self.db).node(self.module))?;
        let place = self.expect_place(&subject);

        let ty = singleton_pattern_type(self.db, singleton);
        let ty = ty.negate_if(self.db, !is_positive);
        Some(NarrowingConstraints::from_iter([(
            place,
            NarrowingConstraint::intersection(ty),
        )]))
    }

    fn evaluate_match_pattern_class(
        &mut self,
        subject: Expression<'db>,
        cls: Expression<'db>,
        kind: ClassPatternKind,
        is_positive: bool,
    ) -> Option<NarrowingConstraints<'db>> {
        if !kind.is_irrefutable() && !is_positive {
            // A class pattern like `case Point(x=0, y=0)` is not irrefutable. In the positive case,
            // we can still narrow the type of the match subject to `Point`. But in the negative case,
            // we cannot exclude `Point` as a possibility.
            return None;
        }

        let subject = PlaceExpr::try_from_expr(subject.node_ref(self.db).node(self.module))?;
        let place = self.expect_place(&subject);

        let class_type = infer_same_file_expression_type(self.db, cls, TypeContext::default());

        let narrowed_type = match class_type {
            Type::ClassLiteral(class) => {
                Type::instance(self.db, class.top_materialization(self.db))
                    .negate_if(self.db, !is_positive)
            }
            Type::SpecialForm(SpecialFormType::CollectionsAbcCallable) => {
                callable_pattern_type(self.db).negate_if(self.db, !is_positive)
            }
            dynamic @ Type::Dynamic(_) => dynamic,
            _ => return None,
        };

        Some(NarrowingConstraints::from_iter([(
            place,
            NarrowingConstraint::intersection(narrowed_type),
        )]))
    }

    fn evaluate_match_pattern_mapping(
        &mut self,
        subject: Expression<'db>,
        kind: ClassPatternKind,
        is_positive: bool,
    ) -> Option<NarrowingConstraints<'db>> {
        if !kind.is_irrefutable() && !is_positive {
            return None;
        }

        let subject = PlaceExpr::try_from_expr(subject.node_ref(self.db).node(self.module))?;
        let place = self.expect_place(&subject);

        let mapping_type = ClassInfoConstraintFunction::IsInstance
            .generate_constraint(
                self.db,
                KnownClass::Mapping.to_class_literal(self.db),
                is_positive,
            )?
            .negate_if(self.db, !is_positive);

        Some(NarrowingConstraints::from_iter([(
            place,
            NarrowingConstraint::intersection(mapping_type),
        )]))
    }

    /// Return a type that contains every value that can match `pattern`.
    ///
    /// For example, given:
    ///
    /// ```python
    /// def f(x: list[object]):
    ///     match x:
    ///         case [int(real=0)]:
    ///             reveal_type(x)
    /// ```
    ///
    /// Every `x` that matches `[int(real=0)]` must be a one-element sequence
    /// containing an `int`. We can represent this information in the returned
    /// type; however, that type omits the `real=0` constraint, and so includes
    /// values like as `[1]`, which do not match the pattern. In other words,
    /// the returned type may include values that do not match, but it must include
    /// every value that does.
    fn necessary_match_pattern_type(&self, pattern: &PatternPredicateKind<'db>) -> Type<'db> {
        match pattern {
            PatternPredicateKind::Singleton(singleton) => {
                singleton_pattern_type(self.db, *singleton)
            }
            PatternPredicateKind::Class(cls, _) => {
                match infer_same_file_expression_type(self.db, *cls, TypeContext::default()) {
                    Type::ClassLiteral(class) => {
                        Type::instance(self.db, class.top_materialization(self.db))
                    }
                    Type::SpecialForm(SpecialFormType::CollectionsAbcCallable) => {
                        callable_pattern_type(self.db)
                    }
                    _ => Type::object(),
                }
            }
            PatternPredicateKind::Mapping(_) => mapping_pattern_type(self.db),
            PatternPredicateKind::Sequence(kind) => self.necessary_sequence_pattern_type(kind),
            PatternPredicateKind::Or(predicates) => UnionType::from_elements(
                self.db,
                predicates
                    .iter()
                    .map(|predicate| self.necessary_match_pattern_type(predicate)),
            ),
            PatternPredicateKind::As(pattern, _) => pattern
                .as_deref()
                .map(|pattern| self.necessary_match_pattern_type(pattern))
                .unwrap_or_else(Type::object),
            PatternPredicateKind::Value(_) | PatternPredicateKind::MatchStar => Type::object(),
        }
    }

    /// Preserve the element constraints that can be addressed at fixed indices.
    fn necessary_sequence_pattern_type(
        &self,
        kind: &SequencePatternPredicateKind<'db>,
    ) -> Type<'db> {
        if kind.is_exact_length() {
            let element_types = kind
                .patterns
                .iter()
                .map(|pattern| self.necessary_match_pattern_type(pattern));
            exact_sequence_pattern_type(self.db, element_types)
        } else {
            let Some((prefix_patterns, suffix_patterns)) = kind.split_around_star() else {
                return sequence_pattern_type_builder(self.db).build();
            };

            let prefix_element_types = prefix_patterns
                .iter()
                .map(|pattern| self.necessary_match_pattern_type(pattern));
            let suffix_element_types = suffix_patterns
                .iter()
                .map(|pattern| self.necessary_match_pattern_type(pattern));

            starred_sequence_pattern_type(self.db, prefix_element_types, suffix_element_types)
        }
    }

    fn evaluate_match_pattern_sequence(
        &mut self,
        subject: Expression<'db>,
        kind: &SequencePatternPredicateKind<'db>,
        is_positive: bool,
    ) -> PatternNarrowingResult<'db> {
        let subject_node = subject.node_ref(self.db).node(self.module);

        // A tuple or list expression has no place that can be narrowed as a whole. For example:
        //
        //     match (a, b):
        //         case (A(), B()):
        //
        // Apply the element constraints to the narrowable elements of the subject expression.
        if let Some(elements) = Self::sequence_expression_elements(subject_node) {
            return self.evaluate_match_pattern_sequence_for_subject_element(
                elements,
                kind,
                is_positive,
                None,
            );
        }

        let Some(subject) = PlaceExpr::try_from_expr(subject_node) else {
            return PatternNarrowingResult::Possible(None);
        };

        let constraint = if is_positive {
            NarrowingConstraint::intersection(self.necessary_sequence_pattern_type(kind))
        } else {
            let sequence_type = definite_sequence_pattern_type(self.db, kind);
            if sequence_type.is_never() {
                return PatternNarrowingResult::Possible(None);
            }
            NarrowingConstraint::intersection(sequence_type.negate(self.db))
        };

        let place = self.expect_place(&subject);

        PatternNarrowingResult::Possible(Some(NarrowingConstraints::from_iter([(
            place, constraint,
        )])))
    }

    fn sequence_expression_elements(expression: &ast::Expr) -> Option<&[ast::Expr]> {
        match expression {
            ast::Expr::List(list) => Some(&list.elts),
            ast::Expr::Tuple(tuple) => Some(&tuple.elts),
            _ => None,
        }
    }

    fn evaluate_match_pattern_sequence_for_subject_element(
        &mut self,
        elements: &[ast::Expr],
        kind: &SequencePatternPredicateKind<'db>,
        is_positive: bool,
        target: Option<ExpressionNodeKey>,
    ) -> PatternNarrowingResult<'db> {
        // A starred display has variable runtime length, so its elements cannot be aligned with
        // the pattern without separately modeling the value consumed by the star.
        if elements.iter().any(ast::Expr::is_starred_expr) {
            return PatternNarrowingResult::Possible(None);
        }

        let (prefix_patterns, suffix_patterns) =
            if let Some((prefix, suffix)) = kind.split_around_star() {
                if elements.len() < prefix.len() + suffix.len() {
                    return PatternNarrowingResult::Impossible;
                }
                (prefix, suffix)
            } else {
                if elements.len() != kind.patterns.len() {
                    return PatternNarrowingResult::Impossible;
                }
                (kind.patterns.as_ref(), &[][..])
            };

        if !is_positive {
            return PatternNarrowingResult::Possible(None);
        }

        let element_patterns = elements
            .iter()
            .zip(prefix_patterns)
            .chain(elements.iter().rev().zip(suffix_patterns.iter().rev()));
        let mut constraints = None;
        for (element, pattern) in element_patterns {
            match self.evaluate_match_pattern_for_subject_element(element, pattern, target) {
                PatternNarrowingResult::Impossible => return PatternNarrowingResult::Impossible,
                PatternNarrowingResult::Possible(element_constraints) => {
                    constraints =
                        Self::merge_optional_constraints_and(constraints, element_constraints);
                }
            }
        }

        PatternNarrowingResult::Possible(constraints)
    }

    fn evaluate_match_pattern_for_subject_element(
        &mut self,
        subject: &ast::Expr,
        pattern: &PatternPredicateKind<'db>,
        target: Option<ExpressionNodeKey>,
    ) -> PatternNarrowingResult<'db> {
        if let Some(elements) = Self::sequence_expression_elements(subject) {
            return match pattern {
                PatternPredicateKind::Sequence(kind) => self
                    .evaluate_match_pattern_sequence_for_subject_element(
                        elements, kind, true, target,
                    ),
                PatternPredicateKind::As(Some(pattern), _) => {
                    self.evaluate_match_pattern_for_subject_element(subject, pattern, target)
                }
                PatternPredicateKind::Or(patterns) => PatternNarrowingResult::merge_alternatives(
                    patterns.iter().map(|pattern| {
                        self.evaluate_match_pattern_for_subject_element(subject, pattern, target)
                    }),
                    Self::merge_optional_constraints_or,
                ),
                _ => PatternNarrowingResult::Possible(None),
            };
        }

        let subject_expr = subject;
        let Some(subject) = PlaceExpr::try_from_expr(subject_expr) else {
            return PatternNarrowingResult::Possible(None);
        };
        if let Some(target) = target
            && ExpressionNodeKey::from(subject_expr) != target
        {
            return PatternNarrowingResult::Possible(None);
        }
        PatternNarrowingResult::Possible(Some(NarrowingConstraints::from_iter([(
            self.expect_place(&subject),
            NarrowingConstraint::intersection(self.necessary_match_pattern_type(pattern)),
        )])))
    }

    fn evaluate_match_pattern_value(
        &mut self,
        subject: Expression<'db>,
        value: Expression<'db>,
        is_positive: bool,
    ) -> Option<NarrowingConstraints<'db>> {
        let subject_node = subject.node_ref(self.db).node(self.module);
        let place = {
            let subject = PlaceExpr::try_from_expr(subject_node)?;
            self.expect_place(&subject)
        };
        let subject_ty = infer_same_file_expression_type(self.db, subject, TypeContext::default());
        let value_ty = infer_same_file_expression_type(self.db, value, TypeContext::default());

        let mut constraints = self
            .evaluate_expr_compare_op(subject_ty, value_ty, ast::CmpOp::Eq, is_positive)
            .map(|ty| {
                NarrowingConstraints::from_iter([(place, NarrowingConstraint::intersection(ty))])
            })
            .unwrap_or_default();

        // Narrow tagged unions of `TypedDict`s with `Literal` keys, for example:
        //
        //     class Foo(TypedDict):
        //         tag: Literal["foo"]
        //     class Bar(TypedDict):
        //         tag: Literal["bar"]
        //     def _(union: Foo | Bar):
        //         match union["tag"]:
        //             case "foo":
        //                 reveal_type(union)  # Foo
        //
        // Like in the `if` statement case, we're constraining `union` itself, not `union["tag"]`.
        if let ast::Expr::Subscript(subscript) = subject_node {
            let inference = infer_expression_types(self.db, subject, TypeContext::default());
            if let Some((place, constraint)) = self.narrow_typeddict_subscript(
                inference.expression_type(&*subscript.value),
                &subscript.value,
                inference.expression_type(&*subscript.slice),
                value_ty,
                is_positive,
            ) {
                constraints.insert(place, constraint);
            }
            // Narrow tagged unions of tuples with `Literal` elements, just like `if` statements.
            else if let Some((place, constraint)) = self.narrow_tuple_subscript(
                inference.expression_type(&*subscript.value),
                &subscript.value,
                inference.expression_type(&*subscript.slice),
                value_ty,
                is_positive,
            ) {
                constraints.insert(place, constraint);
            }
        } else if let ast::Expr::Attribute(attribute) = subject_node {
            let inference = infer_expression_types(self.db, subject, TypeContext::default());
            if let Some((place, constraint)) = self.narrow_nominal_attribute(
                inference.expression_type(&*attribute.value),
                &attribute.value,
                attribute.attr.id(),
                value_ty,
                is_positive,
            ) {
                constraints.insert(place, constraint);
            }
        }

        Some(constraints)
    }

    fn evaluate_match_pattern_or(
        &mut self,
        subject: Expression<'db>,
        predicates: &Vec<PatternPredicateKind<'db>>,
        is_positive: bool,
    ) -> PatternNarrowingResult<'db> {
        let merge_constraints = if is_positive {
            Self::merge_optional_constraints_or
        } else {
            Self::merge_optional_constraints_and
        };

        PatternNarrowingResult::merge_alternatives(
            predicates.iter().map(|predicate| {
                self.evaluate_pattern_predicate_kind(predicate, subject, is_positive)
            }),
            merge_constraints,
        )
    }

    fn evaluate_bool_op(
        &mut self,
        expr_bool_op: &ExprBoolOp,
        expression: Expression<'db>,
        is_positive: bool,
    ) -> Option<NarrowingConstraints<'db>> {
        let inference = infer_expression_types(self.db, expression, TypeContext::default());
        let sub_constraints = expr_bool_op
            .values
            .iter()
            // filter our arms with statically known truthiness
            .filter(|expr| {
                inference.expression_type(*expr).bool(self.db)
                    != match expr_bool_op.op {
                        BoolOp::And => Truthiness::AlwaysTrue,
                        BoolOp::Or => Truthiness::AlwaysFalse,
                    }
            })
            .map(|sub_expr| {
                self.evaluate_expression_node_predicate(sub_expr, expression, is_positive)
            })
            .collect::<Vec<_>>();
        match (expr_bool_op.op, is_positive) {
            (BoolOp::And, true) | (BoolOp::Or, false) => {
                let mut aggregation: Option<NarrowingConstraints> = None;
                for sub_constraint in sub_constraints.into_iter().flatten() {
                    if let Some(ref mut some_aggregation) = aggregation {
                        merge_constraints_and(some_aggregation, sub_constraint);
                    } else {
                        aggregation = Some(sub_constraint);
                    }
                }
                aggregation
            }
            (BoolOp::Or, true) | (BoolOp::And, false) => {
                let (mut first, rest) = {
                    let mut it = sub_constraints.into_iter();
                    (it.next()?, it)
                };

                if let Some(ref mut first) = first {
                    for rest_constraint in rest {
                        if let Some(rest_constraint) = rest_constraint {
                            merge_constraints_or(first, rest_constraint);
                        } else {
                            return None;
                        }
                    }
                }
                first
            }
        }
    }

    /// Narrow tagged unions of `TypedDict`s with `Literal` keys.
    ///
    /// Given a subscript expression like `union["tag"]` where `union` is a `TypedDict` (or union
    /// containing `TypedDict`s), and a comparison value like `"foo"`, this method creates a
    /// constraint on `union` (not `union["tag"]`) that narrows it based on the tag value.
    ///
    /// Returns `Some((place, constraint))` if narrowing is possible, `None` otherwise.
    fn narrow_typeddict_subscript(
        &self,
        subscript_value_type: Type<'db>,
        subscript_value_expr: &ast::Expr,
        subscript_key_type: Type<'db>,
        rhs_type: Type<'db>,
        is_equality: bool,
    ) -> Option<(ScopedPlaceId, NarrowingConstraint<'db>)> {
        // Check preconditions: we need a TypedDict, a string key, and a supported tag literal.
        if !is_or_contains_typeddict(self.db, subscript_value_type) {
            return None;
        }
        let subscript_place_expr = PlaceExpr::try_from_expr(subscript_value_expr)?;
        let key_literal = subscript_key_type.as_string_literal()?;
        if !is_supported_tag_literal(rhs_type) {
            return None;
        }

        // If we have an equality constraint, we have to be careful. If all the matching fields
        // in all the `TypedDict`s here have literal types, then yes, equality is as good as a
        // type check. However, if any of them are e.g. `int` or `str` or some random class,
        // then we can't narrow their type at all, because subclasses of those types can
        // implement `__eq__` in any perverse way they like. On the other hand, if this is an
        // *inequality* constraint, then we can go ahead and assert "you can't be this exact
        // literal type" without worrying about what other types might be present.
        if is_equality
            && !all_matching_typeddict_fields_have_literal_types(
                self.db,
                subscript_value_type,
                key_literal.value(self.db),
            )
        {
            return None;
        }

        let field_name = Name::from(key_literal.value(self.db));
        // To avoid excluding non-`TypedDict` types, our constraints are always expressed
        // as a negative intersection (i.e. "you're *not* this kind of `TypedDict`"). If
        // `is_equality` is true, the whole constraint is going to be a double
        // negative, i.e. "you're *not* a `TypedDict` *without* this literal field". As the
        // first step of building that, we negate the right hand side.
        let field_type = rhs_type.negate_if(self.db, is_equality);
        // Create the synthesized `TypedDict` with that (possibly negated) field. We don't
        // want to constrain the mutability or required-ness of the field, so the most
        // compatible form is not-required and read-only.
        let field = TypedDictFieldBuilder::new(field_type)
            .required(false)
            .read_only(true)
            .build();
        let schema = TypedDictSchema::from_iter([(field_name, field)]);
        let synthesized_typeddict = TypedDictType::from_schema_items(self.db, schema);
        // As mentioned above, the synthesized `TypedDict` is always negated.
        let intersection = Type::TypedDict(synthesized_typeddict).negate(self.db);
        let place = self.expect_place(&subscript_place_expr);
        Some((place, NarrowingConstraint::intersection(intersection)))
    }

    // TODO: Restructure this helper to return the key-presence constraint and apply it with
    // `NarrowingConstraint::intersection` at the call site instead of constructing a replacement
    // type here.
    fn narrow_with_present_key(&self, ty: Type<'db>, key: &str) -> Type<'db> {
        let db = self.db;
        let constrain = |ty, key_presence_constraint| {
            IntersectionType::from_two_elements(db, ty, key_presence_constraint)
        };

        match ty.resolve_type_alias(self.db) {
            Type::Union(union) => union.map(self.db, |element| {
                self.narrow_with_present_key(*element, key)
            }),
            resolved if typeddict_declares_key(self.db, resolved, key) => resolved,
            // TODO: Extend this to subtypes of `Mapping[str, object]` whose membership and
            // subscript operations obey the `Mapping` contract.
            resolved if is_or_contains_typeddict(self.db, resolved) => constrain(
                ty,
                Type::TypedDict(required_typeddict_key(self.db, key, Type::object())),
            ),
            _ => constrain(ty, key_membership_contains_protocol(self.db, key)),
        }
    }

    /// Narrow tagged unions of tuples with `Literal` elements.
    ///
    /// Given a subscript expression like `t[0]` where `t` is a union of tuple types, and a
    /// comparison value like `"foo"`, this method creates a constraint on `t` that narrows it
    /// based on the element value at that index.
    ///
    /// For example:
    /// ```python
    /// def _(t: tuple[Literal["a"], A] | tuple[Literal["b"], B]):
    ///     if t[0] == "a":
    ///         reveal_type(t)  # tuple[Literal["a"], A]
    /// ```
    ///
    /// Returns `Some((place, constraint))` if narrowing is possible, `None` otherwise.
    fn narrow_tuple_subscript(
        &self,
        subscript_value_type: Type<'db>,
        subscript_value_expr: &ast::Expr,
        subscript_index_type: Type<'db>,
        rhs_type: Type<'db>,
        is_equality: bool,
    ) -> Option<(ScopedPlaceId, NarrowingConstraint<'db>)> {
        // We need a union type for narrowing to be useful.
        let Type::Union(union) = subscript_value_type.resolve_type_alias(self.db) else {
            return None;
        };

        // The subscript index must be an integer literal.
        let index = subscript_index_type.as_int_literal()?;
        let index = i32::try_from(index).ok()?;

        // The comparison value must be a supported literal type.
        if !is_supported_tag_literal(rhs_type) {
            return None;
        }

        let subscript_place_expr = PlaceExpr::try_from_expr(subscript_value_expr)?;

        // Skip narrowing if any tuple in the union has an out-of-bounds index.
        // A diagnostic will be emitted elsewhere for the out-of-bounds access.
        if any_tuple_has_out_of_bounds_index(self.db, union, index) {
            return None;
        }

        // For equality constraints, all matching elements must have literal types to safely narrow.
        // For inequality constraints, we can narrow even with non-literal element types.
        if is_equality && !all_matching_tuple_elements_have_literal_types(self.db, union, index) {
            return None;
        }

        // Filter the union based on whether each tuple element at the index could match the rhs.
        let filtered = union.filter(self.db, |elem| {
            elem.as_nominal_instance()
                .and_then(|inst| inst.tuple_spec(self.db))
                .and_then(|spec| spec.py_index(self.db, index).ok())
                .is_none_or(|el_ty| {
                    if is_equality {
                        // Keep tuples where element could be equal to rhs.
                        !el_ty.is_disjoint_from(self.db, rhs_type)
                    } else {
                        // Keep tuples where element is not always equal to rhs.
                        !el_ty.is_subtype_of(self.db, rhs_type)
                    }
                })
        });

        // Only create a constraint if we actually narrowed something.
        if filtered != Type::Union(union) {
            let place = self.expect_place(&subscript_place_expr);
            Some((place, NarrowingConstraint::replacement(filtered)))
        } else {
            None
        }
    }

    fn narrow_nominal_attribute(
        &self,
        attribute_value_type: Type<'db>,
        attribute_value_expr: &ast::Expr,
        attribute_name: &str,
        rhs_type: Type<'db>,
        is_equality: bool,
    ) -> Option<(ScopedPlaceId, NarrowingConstraint<'db>)> {
        let Type::Union(union) = attribute_value_type.resolve_type_alias(self.db) else {
            return None;
        };
        if !is_supported_tag_literal(rhs_type) {
            return None;
        }

        let narrowed = union.filter(self.db, |element| {
            nominal_attribute_type(self.db, *element, attribute_name).is_none_or(|attribute_type| {
                if is_equality {
                    !is_supported_tag_literal(attribute_type)
                        || !attribute_type.is_disjoint_from(self.db, rhs_type)
                } else {
                    !attribute_type.is_subtype_of(self.db, rhs_type)
                }
            })
        });

        if narrowed == Type::Union(union) {
            return None;
        }

        let attribute_value_place_expr = PlaceExpr::try_from_expr(attribute_value_expr)?;
        let place = self.expect_place(&attribute_value_place_expr);
        Some((place, NarrowingConstraint::replacement(narrowed)))
    }
}

// Return true if the given type is a `TypedDict` or a union or intersection that includes at least
// one `TypedDict` (even if other types are also present), or a type alias to such a type.
fn is_or_contains_typeddict<'db>(db: &'db dyn Db, ty: Type<'db>) -> bool {
    match ty {
        Type::TypedDict(_) => true,
        Type::Intersection(intersection) => intersection
            .positive(db)
            .iter()
            .any(|intersection_element_ty| is_or_contains_typeddict(db, *intersection_element_ty)),
        Type::Union(union) => union
            .elements(db)
            .iter()
            .any(|union_member_ty| is_or_contains_typeddict(db, *union_member_ty)),
        Type::TypeAlias(alias) => is_or_contains_typeddict(db, alias.value_type(db)),

        Type::Dynamic(_)
        | Type::Divergent(_)
        | Type::Projection(_)
        | Type::Never
        | Type::EnumComplement(_)
        | Type::FunctionLiteral(_)
        | Type::BoundMethod(_)
        | Type::KnownBoundMethod(_)
        | Type::WrapperDescriptor(_)
        | Type::DataclassDecorator(_)
        | Type::DataclassTransformer(_)
        | Type::Callable(_)
        | Type::ModuleLiteral(_)
        | Type::ClassLiteral(_)
        | Type::GenericAlias(_)
        | Type::SubclassOf(_)
        | Type::NominalInstance(_)
        | Type::ProtocolInstance(_)
        | Type::SpecialForm(_)
        | Type::KnownInstance(_)
        | Type::PropertyInstance(_)
        | Type::AlwaysTruthy
        | Type::AlwaysFalsy
        | Type::LiteralValue(_)
        | Type::TypeVar(_)
        | Type::BoundSuper(_)
        | Type::TypeIs(_)
        | Type::TypeGuard(_)
        | Type::TypeForm(_)
        | Type::NewTypeInstance(_) => false,
    }
}

fn typeddict_declares_key<'db>(db: &'db dyn Db, ty: Type<'db>, key: &str) -> bool {
    match ty {
        Type::TypedDict(typed_dict) => typed_dict.items(db).contains_key(key),
        Type::Intersection(intersection) => intersection
            .positive(db)
            .iter()
            .any(|element| typeddict_declares_key(db, *element, key)),
        Type::Union(union) => union
            .elements(db)
            .iter()
            .any(|element| typeddict_declares_key(db, *element, key)),
        Type::TypeAlias(alias) => typeddict_declares_key(db, alias.value_type(db), key),
        _ => false,
    }
}

/// Return a synthesized `TypedDict` that represents safe subscript access for a present key on a
/// `TypedDict`-containing type.
///
/// For `TypedDict`s, a positive key-membership test proves more than containment: it also makes
/// string-literal subscript access with that key valid. In the `if` branch below, the `Bar` arm
/// keeps its original shape but is intersected with this schema so `u["foo"]` is accepted:
///
/// ```python
/// class Foo(TypedDict):
///     foo: int
///
/// class Bar(TypedDict):
///     bar: int
///
/// def f(u: Foo | Bar):
///     if "foo" in u:
///         reveal_type(u["foo"])  # object
/// ```
fn required_typeddict_key<'db>(
    db: &'db dyn Db,
    key: &str,
    value_ty: Type<'db>,
) -> TypedDictType<'db> {
    let field = TypedDictFieldBuilder::new(value_ty)
        .required(true)
        .read_only(true)
        .build();
    let schema = TypedDictSchema::from_iter([(Name::from(key), field)]);
    TypedDictType::from_schema_items(db, schema)
}

/// Return a synthesized protocol that records a true key-membership test without implying
/// subscript access.
///
/// For non-`TypedDict` types, `"key" in value` only proves that membership is true. It does not
/// prove that `value["key"]` is valid:
///
/// ```python
/// def f(s: Literal["abc"]):
///     if "a" in s:
///         s["a"]  # Runtime `TypeError`
/// ```
///
/// Non-`TypedDict` union arms therefore receive this `__contains__` protocol instead of the
/// synthesized `TypedDict` used for `TypedDict` arms.
fn key_membership_contains_protocol<'db>(db: &'db dyn Db, key: &str) -> Type<'db> {
    let signature = Signature::new(
        Parameters::new(
            db,
            [
                Parameter::positional_only(Some(Name::new_static("self"))),
                Parameter::positional_only(Some(Name::new_static("key")))
                    .with_annotated_type(Type::string_literal(db, key)),
            ],
        ),
        Type::bool_literal(true),
    );

    Type::protocol_with_methods(
        db,
        [("__contains__", CallableType::function_like(db, signature))],
    )
}

fn is_supported_tag_literal(ty: Type) -> bool {
    matches!(
        ty.as_literal_value_kind(),
        Some(
            LiteralValueTypeKind::String(_)
                | LiteralValueTypeKind::Bytes(_)
                | LiteralValueTypeKind::Int(_)
                | LiteralValueTypeKind::Enum(_)
        )
    )
}

fn nominal_attribute_type<'db>(
    db: &'db dyn Db,
    ty: Type<'db>,
    attribute_name: &str,
) -> Option<Type<'db>> {
    let resolved_ty = ty.resolve_type_alias(db);
    if resolved_ty.is_nominal_instance() {
        resolved_ty
            .member(db, attribute_name)
            .place
            .ignore_possibly_undefined()
    } else {
        None
    }
}

// Return true if the given type is a `TypedDict` whose `field_name` field has a supported tag literal
// type, or a union in which all elements that are `TypedDict`s have a supported tag literal type
// for that field, or an intersection in which all positive elements that are `TypedDict`s have a
// supported tag literal type for that field, or a type alias to such a type.
fn all_matching_typeddict_fields_have_literal_types<'db>(
    db: &'db dyn Db,
    ty: Type<'db>,
    field_name: &str,
) -> bool {
    let matching_field_is_literal = |typeddict: &TypedDictType<'db>| {
        // There's no matching field to check if `.get()` returns `None`.
        typeddict
            .items(db)
            .get(field_name)
            .is_none_or(|field| is_supported_tag_literal(field.declared_ty))
    };

    match ty {
        Type::TypedDict(td) => matching_field_is_literal(&td),
        Type::Union(union) => union.elements(db).iter().all(|union_member_ty| {
            !is_or_contains_typeddict(db, *union_member_ty)
                || all_matching_typeddict_fields_have_literal_types(
                    db,
                    *union_member_ty,
                    field_name,
                )
        }),
        Type::TypeAlias(alias) => {
            all_matching_typeddict_fields_have_literal_types(db, alias.value_type(db), field_name)
        }
        Type::Intersection(intersection) => {
            intersection
                .positive(db)
                .iter()
                .all(|intersection_member_ty| {
                    !is_or_contains_typeddict(db, *intersection_member_ty)
                        || all_matching_typeddict_fields_have_literal_types(
                            db,
                            *intersection_member_ty,
                            field_name,
                        )
                })
        }

        // Only the four variants above can pass `is_or_contains_typeddict`, and this function is
        // always guarded by that check.
        Type::Dynamic(_)
        | Type::Divergent(_)
        | Type::Projection(_)
        | Type::Never
        | Type::EnumComplement(_)
        | Type::FunctionLiteral(_)
        | Type::BoundMethod(_)
        | Type::KnownBoundMethod(_)
        | Type::WrapperDescriptor(_)
        | Type::DataclassDecorator(_)
        | Type::DataclassTransformer(_)
        | Type::Callable(_)
        | Type::ModuleLiteral(_)
        | Type::ClassLiteral(_)
        | Type::GenericAlias(_)
        | Type::SubclassOf(_)
        | Type::NominalInstance(_)
        | Type::ProtocolInstance(_)
        | Type::SpecialForm(_)
        | Type::KnownInstance(_)
        | Type::PropertyInstance(_)
        | Type::AlwaysTruthy
        | Type::AlwaysFalsy
        | Type::LiteralValue(_)
        | Type::TypeVar(_)
        | Type::BoundSuper(_)
        | Type::TypeIs(_)
        | Type::TypeGuard(_)
        | Type::TypeForm(_)
        | Type::NewTypeInstance(_) => {
            unreachable!(
                "invalid type {} in all_matching_typeddict_fields_have_literal_types",
                ty.display(db)
            )
        }
    }
}

/// Check if any tuple in the union has an out-of-bounds index.
///
/// If the index is out of bounds for any tuple, we should skip narrowing entirely
/// since a diagnostic will be emitted elsewhere for the out-of-bounds access.
fn any_tuple_has_out_of_bounds_index<'db>(
    db: &'db dyn Db,
    union: UnionType<'db>,
    index: i32,
) -> bool {
    union.elements(db).iter().any(|elem| {
        elem.as_nominal_instance()
            .and_then(|inst| inst.tuple_spec(db))
            .is_some_and(|spec| spec.py_index(db, index).is_err())
    })
}

/// Check that all tuple elements at the given index have literal types.
///
/// For equality narrowing to be safe, we need to ensure that the element types
/// at the discriminating index are literals (which have well-defined equality).
/// Non-literal types (like `str` or `int`) could have subclasses that override
/// `__eq__` in unexpected ways.
fn all_matching_tuple_elements_have_literal_types<'db>(
    db: &'db dyn Db,
    union: UnionType<'db>,
    index: i32,
) -> bool {
    union.elements(db).iter().all(|elem| {
        elem.as_nominal_instance()
            .and_then(|inst| inst.tuple_spec(db))
            .and_then(|spec| spec.py_index(db, index).ok())
            .is_none_or(is_supported_tag_literal)
    })
}

pub(crate) trait NarrowingEvaluatorExtension<'db> {
    fn narrow(&self, db: &'db dyn Db, base_type: Type<'db>, place: ScopedPlaceId) -> Type<'db>;
}

impl<'db> NarrowingEvaluatorExtension<'db> for NarrowingEvaluator<'_, 'db> {
    fn narrow(&self, db: &'db dyn Db, base_type: Type<'db>, place: ScopedPlaceId) -> Type<'db> {
        narrow_type_by_constraint(
            db,
            self.narrowing_constraints(),
            self.predicates(),
            self.constraint(),
            base_type,
            place,
        )
    }
}
