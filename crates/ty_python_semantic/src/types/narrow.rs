use std::borrow::Cow;
use std::collections::{BTreeMap, btree_map::Entry as BTreeEntry, hash_map::Entry};

use crate::Db;
use crate::reachability::{narrow_type_by_constraint, type_narrowed_by_previous_patterns};
use crate::subscript::PyIndex;
use crate::types::function::KnownFunction;
use crate::types::infer::{ExpressionInference, infer_same_file_expression_type};
use crate::types::special_form::TypeQualifier;
use crate::types::tuple::{TupleLength, TupleSpec, TupleSpecBuilder, TupleType, TupleUnpacker};
use crate::types::typed_dict::{
    TypedDictField, TypedDictFieldBuilder, TypedDictSchema, TypedDictType,
};
use crate::types::{
    CallableType, ClassBase, ClassLiteral, ClassPatternPositionalSource, ClassType,
    IntersectionBuilder, IntersectionType, KnownClass, KnownInstanceType, LiteralValueTypeKind,
    Parameter, Parameters, Signature, SpecialFormType, SubclassOfInner, SubclassOfType, Truthiness,
    Type, TypeContext, TypeVarBoundOrConstraints, UnionBuilder, callable_pattern_type,
    class_pattern_positional_sources, definite_match_pattern_type_for_subject,
    exact_sequence_pattern_type, infer_expression_types, mapping_pattern_type,
    pattern_binding_fallthrough_type, sequence_pattern_type_builder, singleton_pattern_type,
    starred_sequence_pattern_type, typed_dict_matches_class_pattern,
};
use ty_python_core::expression::Expression;
use ty_python_core::frozen::FrozenMap;
use ty_python_core::place::{PlaceExpr, PlaceTable, ScopedPlaceId};
use ty_python_core::predicate::{
    CallableAndCallExpr, ClassPatternPredicateKind, MappingPatternPredicateKind, PatternPredicate,
    PatternPredicateKind, Predicate, PredicateNode, SequencePatternPredicateKind,
    SubjectElementPatternPredicate,
};
use ty_python_core::scope::ScopeId;
use ty_python_core::{ExpressionNodeKey, NarrowingEvaluator, place_table, semantic_index};

use ruff_db::parsed::{ParsedModuleRef, parsed_module};
use ruff_python_ast::name::Name;
use ruff_python_stdlib::identifiers::is_identifier;

use super::UnionType;
use super::call::CallArguments;
use super::constraints::{ConstraintSetBuilder, PathBounds, Solutions};
use super::equality::{
    ComparisonSoundnessPolicy, equality_exclusion_constraint, equality_truthiness,
    evaluate_type_equality, evaluate_type_inequality,
};
use super::variance::TypeVarVariance;
use itertools::Itertools;
use ruff_python_ast as ast;
use ruff_python_ast::{BoolOp, ExprBoolOp};
use rustc_hash::FxHashMap;
use smallvec::{SmallVec, smallvec, smallvec_inline};

mod containment;

use self::containment::{elements_of, narrow_string_membership};

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
        PredicateNode::IsNonTerminalCall(_)
        | PredicateNode::IsNonEmptyIterable(_)
        | PredicateNode::StarImportPlaceholder(_) => (None, None),
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

/// The types produced when a match pattern succeeds.
///
/// This positive structural analysis infers the type of each supported name bound by a successful
/// pattern. Definite-match analysis, which is used for negative narrowing and exhaustiveness,
/// intentionally remains separate.
#[derive(Debug, Eq, PartialEq, get_size2::GetSize, salsa::SalsaValue)]
pub(crate) struct PatternSuccessTypes<'db> {
    bindings: FrozenMap<ScopedPlaceId, Type<'db>>,
    missing_binding_ty: Type<'db>,
}

impl<'db> PatternSuccessTypes<'db> {
    /// Return the inferred binding type.
    ///
    /// A missing entry is `Never` when the whole pattern cannot match. Otherwise, it is `Unknown`
    /// as defensive recovery for a malformed pattern or a binding that was not recorded. During
    /// cycle recovery, it is the divergent type for that cycle.
    pub(crate) fn binding_type(&self, place: ScopedPlaceId) -> Type<'db> {
        // An OR pattern's alternatives define one runtime binding, so their types are merged by
        // place.
        self.bindings
            .get(&place)
            .copied()
            .unwrap_or(self.missing_binding_ty)
    }

    fn cycle_initial(missing_binding_ty: Type<'db>) -> Self {
        Self {
            bindings: FrozenMap::default(),
            missing_binding_ty,
        }
    }

    fn cycle_normalized(mut self, db: &'db dyn Db, previous: &Self, cycle: &salsa::Cycle) -> Self {
        for (place, ty) in &mut self.bindings {
            *ty = ty.cycle_normalized(db, previous.binding_type(*place), cycle);
        }
        self.missing_binding_ty =
            self.missing_binding_ty
                .cycle_normalized(db, previous.missing_binding_ty, cycle);
        self
    }
}

/// The types produced by one pattern node when that complete node succeeds.
///
/// Bindings are retained only when the complete pattern succeeds:
///
/// ```python
/// def f(value: tuple[str, str]) -> None:
///     match value:
///         case [item, int()]:
///             ...
/// ```
///
/// The pattern produces `Never` for the matched subject and commits no `item` binding, because the
/// second element prevents the complete sequence pattern from matching.
struct PatternSuccessResult<'db> {
    /// The type established while this pattern is being evaluated.
    matched_subject_ty: Type<'db>,
    /// The subject type safe to assign to a binding created by this pattern.
    ///
    /// Exact tuples can retain matched length and element facts. Other sequence types can have
    /// mutable or stateful length and item access, so their bindings retain only facts that remain
    /// valid after the pattern finishes.
    binding_subject_ty: Type<'db>,
    bindings: BTreeMap<ScopedPlaceId, PatternBindingTypes<'db>>,
}

/// The candidate types for one name bound by a successful pattern.
///
/// Contributions from distinct subject or `or`-pattern arms remain separate until captures that
/// alias the current subject have been restored to the original subject type:
///
/// ```python
/// match value:
///     case [item] as whole:
///         ...
/// ```
///
/// Here, `whole` aliases the subject, while `item` refers to a value extracted from it. Keeping
/// that distinction prevents `item` from being restored as if it referred to `value`.
#[derive(Clone)]
struct PatternBindingTypes<'db> {
    contributions: SmallVec<[PatternBindingType<'db>; 2]>,
}

/// One successful pattern arm's contribution to a binding type.
#[derive(Clone, Copy)]
struct PatternBindingType<'db> {
    /// The type inferred for the binding in this arm.
    ty: Type<'db>,
    /// Whether the binding aliases the subject at the current level of pattern analysis.
    aliases_subject: bool,
}

/// Controls when pattern analysis restores an original subject type after filtering its arms.
///
/// Class and mapping patterns use [`Self::EquivalentTypes`] because their successful subject type
/// only filters the original type. Sequence and OR patterns can construct a more precise success
/// type, so they use [`Self::TypeVariablesOnly`] to retain that precision while still preserving
/// the identity of an original type variable.
#[derive(Clone, Copy)]
enum OriginalSubjectPreservation {
    EquivalentTypes,
    TypeVariablesOnly,
}

impl<'db> PatternBindingTypes<'db> {
    /// Create a binding that aliases the current subject.
    fn subject(subject_ty: Type<'db>) -> Self {
        Self {
            contributions: smallvec![PatternBindingType {
                ty: subject_ty,
                aliases_subject: true,
            }],
        }
    }

    fn extracted(extracted_ty: Type<'db>) -> Self {
        Self {
            contributions: smallvec![PatternBindingType {
                ty: extracted_ty,
                aliases_subject: false,
            }],
        }
    }

    /// Return the union of all contributions to this binding.
    fn ty(&self, db: &'db dyn Db) -> Type<'db> {
        UnionType::from_elements(db, self.contributions.iter().map(|binding| binding.ty))
    }

    /// Mark every contribution as referring to a value extracted from the current subject.
    fn demote_subject(&mut self) {
        for contribution in &mut self.contributions {
            contribution.aliases_subject = false;
        }
    }

    /// Add the contributions from another successful pattern arm.
    fn merge(&mut self, other: Self) {
        self.contributions.extend(other.contributions);
    }

    /// Return the union of the contributions that alias the current subject.
    fn subject_ty(&self, db: &'db dyn Db) -> Type<'db> {
        UnionType::from_elements(
            db,
            self.contributions
                .iter()
                .filter(|binding| binding.aliases_subject)
                .map(|binding| binding.ty),
        )
    }

    /// Replace all subject-aliasing contributions with one restored subject type.
    ///
    /// Contributions for extracted values remain unchanged.
    fn restore_subject(&mut self, restored_subject_ty: Type<'db>) {
        let mut restored = false;
        self.contributions.retain_mut(|contribution| {
            if !contribution.aliases_subject {
                return true;
            }
            if restored {
                return false;
            }
            contribution.ty = restored_subject_ty;
            restored = true;
            true
        });
    }
}

struct ClassPatternContext<'db> {
    class: Option<ClassLiteral<'db>>,
    class_ty: Type<'db>,
    positional_sources: Vec<ClassPatternPositionalSource>,
}

struct ClassPatternArgument<'db> {
    ty: Type<'db>,
    source: PatternValueSource,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum PatternValueSource {
    Subject,
    Extracted,
}

/// Computes the subject and binding types produced by successful match patterns.
///
/// Subject narrowing and binding inference use separate recursive entry points. Structural
/// patterns share the lower-level operations that filter subject arms, extract child values, and
/// preserve type variables.
struct PatternSuccessAnalyzer<'db> {
    db: &'db dyn Db,
    scope: ScopeId<'db>,
}

/// Infer the types of all names bound when `pattern` succeeds.
///
/// The subject starts with its inferred type after removing values definitely matched by earlier
/// unguarded cases. The analysis then checks the complete pattern before recording any bindings:
///
/// ```python
/// def f(value: int | str) -> None:
///     match value:
///         case int():
///             pass
///         case item:
///             reveal_type(item)  # str
/// ```
#[salsa::tracked(
    returns(ref),
    cycle_initial=|_, id, _| PatternSuccessTypes::cycle_initial(Type::divergent(id)),
    cycle_fn=|db, cycle, previous: &PatternSuccessTypes<'db>, result: PatternSuccessTypes<'db>, _| {
        result.cycle_normalized(db, previous, cycle)
    },
    heap_size=ruff_memory_usage::heap_size
)]
pub(crate) fn pattern_success_types<'db>(
    db: &'db dyn Db,
    pattern: PatternPredicate<'db>,
) -> PatternSuccessTypes<'db> {
    let subject = pattern.subject(db);
    let incoming_subject_ty = infer_same_file_expression_type(db, subject, TypeContext::default());
    let incoming_subject_ty = type_narrowed_by_previous_patterns(db, pattern, incoming_subject_ty);
    let analyzer = PatternSuccessAnalyzer::new(db, pattern.scope(db));
    let result = analyzer.analyze_successful_pattern(pattern.kind(db), incoming_subject_ty);
    PatternSuccessTypes {
        bindings: result
            .bindings
            .into_iter()
            .map(|(place, binding)| (place, binding.ty(db)))
            .collect(),
        missing_binding_ty: if result.matched_subject_ty.is_never() {
            Type::Never
        } else {
            Type::unknown()
        },
    }
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
                    // TODO: This narrowing is not fully sound:
                    // - `type[protocol]` currently admits non-concrete classes, some of which are
                    //   not valid runtime class-info arguments.
                    // - A class can inhabit `type[protocol]` because its metaclass constructs
                    //   protocol-conforming objects even if its nominal instances do not conform.
                    SubclassOfInner::Protocol(protocol) => match self {
                        ClassInfoConstraintFunction::IsInstance => {
                            Some(Type::ProtocolInstance(protocol))
                        }
                        ClassInfoConstraintFunction::IsSubclass => Some(classinfo),
                    },
                    SubclassOfInner::TypeVar(bound_typevar) => match self {
                        ClassInfoConstraintFunction::IsSubclass => Some(classinfo),
                        ClassInfoConstraintFunction::IsInstance => {
                            Some(Type::TypeVar(bound_typevar))
                        }
                    },
                }
            }
            Type::Dynamic(_) | Type::Divergent(_) => Some(classinfo),
            Type::Intersection(intersection) => {
                if intersection.negative(db).is_empty() {
                    let mut builder = IntersectionBuilder::new(db);
                    let mut any_member = false;
                    for element in intersection.positive(db) {
                        // A member that yields no constraint (e.g. a parametrized
                        // generic alias, which is not a valid runtime isinstance
                        // target) should be SKIPPED, not abort narrowing on the
                        // whole intersection. Narrowing on the remaining members
                        // is still sound.
                        if let Some(c) = self.generate_constraint(db, *element, is_positive) {
                            builder = builder.add_positive(c);
                            any_member = true;
                        }
                    }
                    if any_member {
                        Some(builder.build())
                    } else {
                        None
                    }
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
                        .iter_element_types(db)
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

#[derive(Hash, PartialEq, Debug, Eq, Clone, get_size2::GetSize, salsa::SalsaValue)]
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
#[derive(Hash, PartialEq, Debug, Eq, Clone, get_size2::GetSize, salsa::SalsaValue)]
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

    /// Merge two constraints with OR semantics (union/disjunction).
    fn merge_constraint_or(&mut self, other: Self) {
        self.intersection_disjuncts
            .extend(other.intersection_disjuncts);
        self.replacement_disjuncts
            .extend(other.replacement_disjuncts);
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

#[derive(Default, PartialEq, Eq, get_size2::GetSize, salsa::SalsaValue)]
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
                into_constraint.merge_constraint_or(from_constraint);
            }
            Entry::Vacant(_) => {
                // Place only appears in `from`, not in `into`. No constraint needed.
            }
        }
    }
}

/// Return the type established by a successful class pattern.
///
/// This also handles indirect class expressions such as `PatternClass: type[A]`. It is only a
/// positive constraint: failing to match `PatternClass()` does not exclude every `A`, because the
/// value of `PatternClass` may be a subclass of `A`.
fn positive_class_pattern_type<'db>(
    db: &'db dyn Db,
    class_expression_ty: Type<'db>,
) -> Option<Type<'db>> {
    match class_expression_ty {
        Type::SpecialForm(SpecialFormType::CollectionsAbcCallable) => {
            Some(callable_pattern_type(db))
        }
        _ if class_expression_ty.is_assignable_to(db, KnownClass::Type.to_instance(db)) => {
            ClassInfoConstraintFunction::IsInstance.generate_constraint(
                db,
                class_expression_ty,
                true,
            )
        }
        _ => None,
    }
}

/// Refine an exact tuple with the element types established by an exact sequence pattern.
///
/// As elsewhere in tuple-pattern narrowing, this assumes that values represented by a `tuple[...]`
/// annotation preserve the builtin relationship between iteration and indexing. Statically known
/// tuple subclasses are not refined here.
///
/// Gradual tuple elements retain their uncertainty through intersection with the observed pattern
/// type. For example, matching `tuple[Any]` against `[str()]` produces `tuple[Any & str]`.
///
/// In the example below, `subject_ty` is `tuple[int | str]`, `pattern_element_types` is `[str]`,
/// and the refined type returned is `tuple[str]`.
///
/// ```python
/// def f(value: tuple[int | str]) -> None:
///     match value:
///         case [str()]:
///             reveal_type(value)  # tuple[str]
/// ```
fn refine_exact_tuple_for_sequence_pattern<'db>(
    db: &'db dyn Db,
    subject_ty: Type<'db>,
    pattern_element_types: &[Type<'db>],
) -> Option<Type<'db>> {
    let tuple = subject_ty.exact_tuple_instance_spec(db)?;
    let pattern_tuple = TupleSpec::heterogeneous(pattern_element_types.iter().copied());
    Some(
        TupleSpecBuilder::from(tuple.as_ref())
            .intersect(db, &pattern_tuple)
            .map_or(Type::Never, |refined| {
                Type::tuple(TupleType::new(db, &refined.build()))
            }),
    )
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
/// values like `[1]`, which do not match the pattern. In other words, the
/// returned type may include values that do not match, but it must include
/// every value that does.
fn necessary_match_pattern_type<'db>(
    db: &'db dyn Db,
    pattern: &PatternPredicateKind<'db>,
) -> Type<'db> {
    match pattern {
        PatternPredicateKind::Singleton(singleton) => singleton_pattern_type(db, *singleton),
        PatternPredicateKind::Class(kind) => positive_class_pattern_type(
            db,
            infer_same_file_expression_type(db, kind.class, TypeContext::default()),
        )
        .unwrap_or_else(Type::object),
        PatternPredicateKind::Mapping(_) => mapping_pattern_type(db),
        PatternPredicateKind::Sequence(kind) => necessary_sequence_pattern_type(db, kind),
        PatternPredicateKind::Or(predicates) => UnionType::from_elements(
            db,
            predicates
                .iter()
                .map(|predicate| necessary_match_pattern_type(db, predicate)),
        ),
        PatternPredicateKind::As(pattern, _) => pattern
            .as_deref()
            .map(|pattern| necessary_match_pattern_type(db, pattern))
            .unwrap_or_else(Type::object),
        PatternPredicateKind::Value(_) | PatternPredicateKind::Star(_) => Type::object(),
    }
}

/// Preserve the sequence element constraints that can be addressed at fixed indices.
fn necessary_sequence_pattern_type<'db>(
    db: &'db dyn Db,
    kind: &SequencePatternPredicateKind<'db>,
) -> Type<'db> {
    if let Some((prefix_patterns, suffix_patterns)) = kind.split_around_star() {
        let prefix_element_types = prefix_patterns
            .iter()
            .map(|pattern| necessary_match_pattern_type(db, pattern));
        let suffix_element_types = suffix_patterns
            .iter()
            .map(|pattern| necessary_match_pattern_type(db, pattern));

        starred_sequence_pattern_type(db, prefix_element_types, suffix_element_types)
    } else {
        let element_types = kind
            .patterns
            .iter()
            .map(|pattern| necessary_match_pattern_type(db, pattern));
        exact_sequence_pattern_type(db, element_types)
    }
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
            PredicateNode::IsNonEmptyIterable(_) => return None,
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
            ast::Expr::Attribute(attribute) => {
                let constraints = self.evaluate_simple_expr(expression_node, is_positive);
                let inference = infer_expression_types(self.db, expression, TypeContext::default());
                let nominal_constraints = self
                    .narrow_nominal_attribute_by_truthiness(
                        inference.expression_type(&*attribute.value),
                        &attribute.value,
                        attribute.attr.id(),
                        is_positive,
                    )
                    .map(|(place, constraint)| {
                        NarrowingConstraints::from_iter([(place, constraint)])
                    });

                Self::merge_optional_constraints_and(constraints, nominal_constraints)
            }
            ast::Expr::Subscript(subscript) => {
                let constraints = self.evaluate_simple_expr(expression_node, is_positive);
                let inference = infer_expression_types(self.db, expression, TypeContext::default());
                let typeddict_constraints = self
                    .narrow_typeddict_subscript_by_truthiness(
                        inference.expression_type(&*subscript.value),
                        &subscript.value,
                        inference.expression_type(&*subscript.slice),
                        is_positive,
                    )
                    .map(|(place, constraint)| {
                        NarrowingConstraints::from_iter([(place, constraint)])
                    });

                Self::merge_optional_constraints_and(constraints, typeddict_constraints)
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
            ast::Expr::Named(expr_named) => {
                self.evaluate_expr_named(expr_named, expression, is_positive)
            }
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

    fn evaluate_negative_pattern_predicate_kind(
        &mut self,
        pattern_predicate_kind: &PatternPredicateKind<'db>,
        subject: Expression<'db>,
    ) -> PatternNarrowingResult<'db> {
        match pattern_predicate_kind {
            PatternPredicateKind::Singleton(singleton) => PatternNarrowingResult::Possible(
                self.evaluate_negative_match_pattern_singleton(subject, *singleton),
            ),
            PatternPredicateKind::Class(_) | PatternPredicateKind::Mapping(_) => {
                PatternNarrowingResult::Possible(
                    self.evaluate_negative_match_pattern(subject, pattern_predicate_kind),
                )
            }
            PatternPredicateKind::Sequence(kind) => {
                self.evaluate_negative_match_pattern_sequence(subject, kind, pattern_predicate_kind)
            }
            PatternPredicateKind::Value(expr) => PatternNarrowingResult::Possible(
                self.evaluate_match_pattern_value(subject, *expr, false),
            ),
            PatternPredicateKind::Or(predicates) => PatternNarrowingResult::merge_alternatives(
                predicates.iter().map(|predicate| {
                    self.evaluate_negative_pattern_predicate_kind(predicate, subject)
                }),
                Self::merge_optional_constraints_and,
            ),
            PatternPredicateKind::As(Some(pattern), _) => {
                self.evaluate_negative_pattern_predicate_kind(pattern, subject)
            }
            PatternPredicateKind::As(None, _) | PatternPredicateKind::Star(_) => {
                PatternNarrowingResult::Possible(None)
            }
        }
    }

    fn evaluate_pattern_predicate(
        &mut self,
        pattern: PatternPredicate<'db>,
        is_positive: bool,
    ) -> Option<NarrowingConstraints<'db>> {
        let kind = pattern.kind(self.db);
        let subject = pattern.subject(self.db);
        if !is_positive {
            return self
                .evaluate_negative_pattern_predicate_kind(kind, subject)
                .into_constraints();
        }

        let subject_node = subject.node_ref(self.db).node(self.module);
        let expression_constraints = self
            .evaluate_positive_pattern_related_expressions(kind, subject, subject_node)
            .into_constraints();

        let Some(subject_place) = PlaceExpr::try_from_expr(subject_node) else {
            return expression_constraints;
        };
        let place = self.expect_place(&subject_place);
        let subject_ty = infer_same_file_expression_type(self.db, subject, TypeContext::default());
        let mut constraints = expression_constraints.unwrap_or_default();
        constraints.remove(&place);
        if let Some(subject_constraint) = self.positive_subject_constraint(kind, subject_ty) {
            constraints.insert(place, subject_constraint);
        }
        (!constraints.is_empty()).then_some(constraints)
    }

    fn evaluate_positive_pattern_related_expressions(
        &mut self,
        pattern: &PatternPredicateKind<'db>,
        subject: Expression<'db>,
        subject_node: &ast::Expr,
    ) -> PatternNarrowingResult<'db> {
        if Self::sequence_expression_elements(subject_node).is_some() {
            return self.evaluate_match_pattern_for_subject_element(
                subject,
                subject_node,
                pattern,
                None,
            );
        }

        match pattern {
            PatternPredicateKind::Value(value)
                if matches!(
                    subject_node,
                    ast::Expr::Attribute(_) | ast::Expr::Subscript(_)
                ) =>
            {
                PatternNarrowingResult::Possible(
                    self.evaluate_match_pattern_value(subject, *value, true),
                )
            }
            PatternPredicateKind::Or(patterns) => PatternNarrowingResult::merge_alternatives(
                patterns.iter().map(|pattern| {
                    self.evaluate_positive_pattern_related_expressions(
                        pattern,
                        subject,
                        subject_node,
                    )
                }),
                Self::merge_optional_constraints_or,
            ),
            PatternPredicateKind::As(Some(pattern), _) => {
                self.evaluate_positive_pattern_related_expressions(pattern, subject, subject_node)
            }
            _ => PatternNarrowingResult::Possible(None),
        }
    }

    /// Return the positive constraint produced when `pattern` matches `subject_ty`.
    ///
    /// Leaf patterns return their direct runtime-test constraint; the caller applies that constraint
    /// to the current subject type. Structural patterns use the type established by successful
    /// pattern analysis.
    fn positive_subject_constraint(
        &mut self,
        pattern: &PatternPredicateKind<'db>,
        subject_ty: Type<'db>,
    ) -> Option<NarrowingConstraint<'db>> {
        match pattern {
            PatternPredicateKind::Value(value) => {
                let value_ty =
                    infer_same_file_expression_type(self.db, *value, TypeContext::default());
                self.evaluate_expr_compare_op(subject_ty, value_ty, ast::CmpOp::Eq, true)
                    .map(NarrowingConstraint::intersection)
            }
            PatternPredicateKind::Singleton(singleton) => Some(NarrowingConstraint::intersection(
                singleton_pattern_type(self.db, *singleton),
            )),
            PatternPredicateKind::As(Some(pattern), _) => {
                self.positive_subject_constraint(pattern, subject_ty)
            }
            PatternPredicateKind::As(None, _) | PatternPredicateKind::Star(_) => None,
            PatternPredicateKind::Or(patterns) => {
                let mut patterns = patterns.iter();
                let mut constraint =
                    self.positive_subject_constraint(patterns.next()?, subject_ty)?;
                for pattern in patterns {
                    constraint.merge_constraint_or(
                        self.positive_subject_constraint(pattern, subject_ty)?,
                    );
                }
                Some(constraint)
            }
            _ => {
                let matched_subject_ty = PatternSuccessAnalyzer::new(self.db, self.scope())
                    .matched_subject_type(pattern, subject_ty);
                (!matched_subject_ty.is_equivalent_to(self.db, subject_ty))
                    .then(|| NarrowingConstraint::intersection(matched_subject_ty))
            }
        }
    }
}

impl<'db> PatternSuccessAnalyzer<'db> {
    fn new(db: &'db dyn Db, scope: ScopeId<'db>) -> Self {
        Self { db, scope }
    }

    fn comparison_soundness_policy(&self) -> ComparisonSoundnessPolicy {
        ComparisonSoundnessPolicy::from_strict_literal_narrowing(
            self.db
                .analysis_settings(self.scope.file(self.db))
                .strict_literal_narrowing,
        )
    }

    fn merge_binding(
        bindings: &mut BTreeMap<ScopedPlaceId, PatternBindingTypes<'db>>,
        place: ScopedPlaceId,
        binding: PatternBindingTypes<'db>,
    ) {
        match bindings.entry(place) {
            BTreeEntry::Occupied(mut entry) => {
                entry.get_mut().merge(binding);
            }
            BTreeEntry::Vacant(entry) => {
                entry.insert(binding);
            }
        }
    }

    fn merge_bindings(
        into: &mut BTreeMap<ScopedPlaceId, PatternBindingTypes<'db>>,
        from: BTreeMap<ScopedPlaceId, PatternBindingTypes<'db>>,
    ) {
        for (place, binding) in from {
            Self::merge_binding(into, place, binding);
        }
    }

    fn demote_subject_bindings(bindings: &mut BTreeMap<ScopedPlaceId, PatternBindingTypes<'db>>) {
        for binding in bindings.values_mut() {
            binding.demote_subject();
        }
    }

    /// Compute the subject and binding types produced by a successful pattern.
    ///
    /// `subject_ty` is the type of the value reaching this pattern node. Structural patterns
    /// determine the type reaching each child before recursively recording that child's bindings:
    ///
    /// ```python
    /// def f(value: tuple[int, str, bool]) -> None:
    ///     match value:
    ///         case [first, *rest]:
    ///             reveal_type(first)  # int
    ///             reveal_type(rest)  # list[str | bool]
    /// ```
    ///
    /// A failed pattern binds no names. For an `or` pattern, each later alternative sees only the
    /// values not definitely matched by an earlier alternative.
    fn analyze_successful_pattern(
        &self,
        pattern: &PatternPredicateKind<'db>,
        subject_ty: Type<'db>,
    ) -> PatternSuccessResult<'db> {
        match pattern {
            PatternPredicateKind::Class(kind) => {
                self.analyze_successful_class_pattern(kind, subject_ty)
            }
            PatternPredicateKind::Mapping(kind) => {
                self.analyze_successful_mapping_pattern(kind, subject_ty)
            }
            PatternPredicateKind::Sequence(kind) => {
                self.analyze_successful_sequence_pattern(kind, subject_ty)
            }
            PatternPredicateKind::Or(patterns) => {
                self.analyze_successful_or_pattern(patterns, subject_ty)
            }
            PatternPredicateKind::As(pattern, name) => {
                let mut result = pattern.as_deref().map_or_else(
                    || PatternSuccessResult {
                        matched_subject_ty: subject_ty,
                        binding_subject_ty: subject_ty,
                        bindings: BTreeMap::new(),
                    },
                    |pattern| self.analyze_successful_pattern(pattern, subject_ty),
                );
                if !result.matched_subject_ty.is_never()
                    && let Some(place) = name
                        .as_ref()
                        .and_then(|name| self.places().symbol_id(name.as_str()))
                {
                    Self::merge_binding(
                        &mut result.bindings,
                        place.into(),
                        PatternBindingTypes::subject(result.binding_subject_ty),
                    );
                }
                result
            }
            PatternPredicateKind::Star(name) => {
                let mut bindings = BTreeMap::new();
                if let Some(place) = name
                    .as_ref()
                    .and_then(|name| self.places().symbol_id(name.as_str()))
                {
                    bindings.insert(place.into(), PatternBindingTypes::subject(subject_ty));
                }
                PatternSuccessResult {
                    matched_subject_ty: subject_ty,
                    binding_subject_ty: subject_ty,
                    bindings,
                }
            }
            PatternPredicateKind::Value(value) => {
                let matched_subject_ty = self.match_value_pattern_subject_type(*value, subject_ty);
                PatternSuccessResult {
                    matched_subject_ty,
                    binding_subject_ty: matched_subject_ty,
                    bindings: BTreeMap::new(),
                }
            }
            PatternPredicateKind::Singleton(_) => {
                let matched_subject_ty = self
                    .intersect_types(subject_ty, necessary_match_pattern_type(self.db, pattern));
                PatternSuccessResult {
                    matched_subject_ty,
                    binding_subject_ty: matched_subject_ty,
                    bindings: BTreeMap::new(),
                }
            }
        }
    }

    /// Return the type established when `pattern` matches `subject_ty`.
    ///
    /// This analysis is independent of binding inference. In particular, every `or`-pattern
    /// alternative is checked against the original subject, and sequence patterns retain the
    /// structural facts established by their child patterns.
    fn matched_subject_type(
        &self,
        pattern: &PatternPredicateKind<'db>,
        subject_ty: Type<'db>,
    ) -> Type<'db> {
        match pattern {
            PatternPredicateKind::Class(kind) => {
                self.matched_class_pattern_subject_type(kind, subject_ty)
            }
            PatternPredicateKind::Mapping(kind) => {
                self.matched_mapping_pattern_subject_type(kind, subject_ty)
            }
            PatternPredicateKind::Sequence(kind) => {
                self.matched_sequence_pattern_subject_type(kind, subject_ty)
            }
            PatternPredicateKind::Or(patterns) => {
                self.matched_or_pattern_subject_type(patterns, subject_ty)
            }
            PatternPredicateKind::As(Some(pattern), _) => {
                self.matched_subject_type(pattern, subject_ty)
            }
            PatternPredicateKind::As(None, _) | PatternPredicateKind::Star(_) => subject_ty,
            PatternPredicateKind::Value(value) => {
                self.match_value_pattern_subject_type(*value, subject_ty)
            }
            PatternPredicateKind::Singleton(_) => {
                self.intersect_types(subject_ty, necessary_match_pattern_type(self.db, pattern))
            }
        }
    }

    fn matched_or_pattern_subject_type(
        &self,
        patterns: &[PatternPredicateKind<'db>],
        subject_ty: Type<'db>,
    ) -> Type<'db> {
        self.analyze_matched_subject_arms(
            subject_ty,
            OriginalSubjectPreservation::TypeVariablesOnly,
            |analyzer, _, subject_ty| {
                Some(UnionType::from_elements(
                    analyzer.db,
                    patterns
                        .iter()
                        .map(|pattern| analyzer.matched_subject_type(pattern, subject_ty)),
                ))
            },
        )
    }

    /// Return the type of the subject when a value pattern succeeds.
    ///
    /// A value pattern uses Python equality, so a successful match does not always narrow to the
    /// type of the value in the pattern:
    ///
    /// ```python
    /// def f(target: int | str) -> None:
    ///     match target:
    ///         case 1 as item:
    ///             reveal_type(item)  # int | str
    /// ```
    ///
    /// The `str` arm remains because a `str` subclass can compare equal to `1`.
    fn match_value_pattern_subject_type(
        &self,
        value: Expression<'db>,
        subject_ty: Type<'db>,
    ) -> Type<'db> {
        let value_ty = infer_same_file_expression_type(self.db, value, TypeContext::default());
        evaluate_type_equality(
            self.db,
            subject_ty,
            value_ty,
            true,
            self.comparison_soundness_policy(),
        )
        .map(|constraint| self.intersect_types(subject_ty, constraint))
        .unwrap_or(subject_ty)
    }

    fn analyze_successful_or_pattern(
        &self,
        patterns: &[PatternPredicateKind<'db>],
        subject_ty: Type<'db>,
    ) -> PatternSuccessResult<'db> {
        self.analyze_pattern_subject_arms(
            subject_ty,
            OriginalSubjectPreservation::TypeVariablesOnly,
            |analyzer, _, arm_ty| {
                Some(analyzer.analyze_successful_or_pattern_arm(patterns, arm_ty))
            },
        )
    }

    fn analyze_successful_or_pattern_arm(
        &self,
        patterns: &[PatternPredicateKind<'db>],
        subject_ty: Type<'db>,
    ) -> PatternSuccessResult<'db> {
        let mut patterns = patterns.iter();
        let Some(first_pattern) = patterns.next() else {
            return PatternSuccessResult {
                matched_subject_ty: Type::Never,
                binding_subject_ty: Type::Never,
                bindings: BTreeMap::new(),
            };
        };
        let first = self.analyze_successful_pattern(first_pattern, subject_ty);
        let mut matched_subject_types = UnionBuilder::new(self.db);
        matched_subject_types.add_in_place(first.matched_subject_ty);
        let mut binding_subject_types = UnionBuilder::new(self.db);
        binding_subject_types.add_in_place(first.binding_subject_ty);
        // All alternatives bind the same names. Merge by logical place so the case body sees the
        // union even though the semantic walk visits the definitions in order.
        let mut bindings = first.bindings;
        let mut remaining_subject_ty = subject_ty;
        let mut previous_pattern = first_pattern;

        for pattern in patterns {
            remaining_subject_ty =
                pattern_binding_fallthrough_type(self.db, previous_pattern, remaining_subject_ty);
            let alternative = self.analyze_successful_pattern(pattern, remaining_subject_ty);
            binding_subject_types.add_in_place(alternative.binding_subject_ty);
            Self::merge_bindings(&mut bindings, alternative.bindings);

            matched_subject_types.add_in_place(self.matched_subject_type(pattern, subject_ty));
            previous_pattern = pattern;
        }

        PatternSuccessResult {
            matched_subject_ty: matched_subject_types.build(),
            binding_subject_ty: binding_subject_types.build(),
            bindings,
        }
    }

    /// Narrow a class-pattern binding while keeping the original type arguments and aliases.
    ///
    /// For example, `children` keeps the recursive element type from `Node` instead of becoming a
    /// broad `list` type:
    ///
    /// ```python
    /// type Node = int | list[Node]
    ///
    /// def visit(node: Node) -> None:
    ///     match node:
    ///         case list() as children:
    ///             for child in children:
    ///                 visit(child)
    /// ```
    fn filter_class_pattern_subject_type(
        &self,
        class: Option<ClassLiteral<'db>>,
        class_ty: Type<'db>,
        subject_ty: Type<'db>,
    ) -> Type<'db> {
        match subject_ty {
            Type::TypeAlias(alias) => {
                self.filter_class_pattern_subject_type(class, class_ty, alias.value_type(self.db))
            }
            Type::Union(union) => union.map(self.db, |element| {
                self.filter_class_pattern_subject_type(class, class_ty, *element)
            }),
            Type::Intersection(intersection) if intersection.positive(self.db).is_empty() => {
                self.intersect_types(subject_ty, class_ty)
            }
            Type::Intersection(intersection) => intersection.map_positive(self.db, |positive| {
                self.filter_class_pattern_subject_type(class, class_ty, *positive)
            }),
            Type::NominalInstance(instance) => {
                let Some(class) = class else {
                    return self.intersect_types(subject_ty, class_ty);
                };
                let subject_class = instance.class(self.db);
                if subject_class.is_subtype_of_class_literal(self.db, class) {
                    subject_ty
                } else if subject_ty.is_disjoint_from(self.db, class_ty) {
                    Type::Never
                } else {
                    self.intersect_types(subject_ty, class_ty)
                }
            }
            Type::TypedDict(_)
                if class.is_some_and(|class| typed_dict_matches_class_pattern(self.db, class)) =>
            {
                subject_ty
            }
            _ if subject_ty.is_subtype_of(self.db, class_ty) => subject_ty,
            _ if subject_ty.is_disjoint_from(self.db, class_ty) => Type::Never,
            _ => self.intersect_types(subject_ty, class_ty),
        }
    }

    fn class_pattern_arguments_for_arm(
        &self,
        kind: &ClassPatternPredicateKind<'db>,
        context: &ClassPatternContext<'db>,
        original_subject_ty: Type<'db>,
        filtering_subject_ty: Type<'db>,
        subject_ty: Type<'db>,
    ) -> Option<Vec<ClassPatternArgument<'db>>> {
        let subject_is_final = subject_ty
            .nominal_class(self.db)
            .is_some_and(|class| class.is_final(self.db));
        let specialized_pattern_class =
            if context.positional_sources.is_empty() && kind.keywords.is_empty() {
                None
            } else {
                context
                    .class
                    .zip(filtering_subject_ty.nominal_class(self.db))
                    .and_then(|(pattern_class, subject_class)| {
                        self.specialize_pattern_class_for_subject(pattern_class, subject_class)
                    })
            };
        let member_type = |name: &Name| {
            let original_member_ty = original_subject_ty
                .member(self.db, name.as_str())
                .place
                .ignore_possibly_undefined();
            let place = subject_ty.member(self.db, name.as_str()).place;
            let mut member_ty = place.ignore_possibly_undefined();
            if original_subject_ty.nominal_class(self.db).is_some()
                && let Type::Intersection(intersection) = subject_ty
            {
                let overlapping_member_ty = UnionType::from_elements(
                    self.db,
                    intersection
                        .positive(self.db)
                        .iter()
                        .filter_map(|positive| {
                            positive
                                .member(self.db, name.as_str())
                                .place
                                .ignore_possibly_undefined()
                        }),
                );
                if !overlapping_member_ty.is_never() {
                    member_ty = Some(overlapping_member_ty);
                }
            }

            if let Some(specialized_pattern_class) = specialized_pattern_class {
                member_ty = Type::instance(self.db, specialized_pattern_class)
                    .member(self.db, name.as_str())
                    .place
                    .ignore_possibly_undefined();
            } else if let Some(pattern_class) = context.class
                && pattern_class
                    .generic_context(self.db)
                    .and_then(|generic_context| {
                        pattern_class
                            .instance_member(
                                self.db,
                                Some(generic_context.identity_specialization(self.db)),
                                name.as_str(),
                            )
                            .place
                            .ignore_possibly_undefined()
                    })
                    .is_some_and(|ty| ty.has_typevar(self.db))
            {
                let unknown_pattern_class = pattern_class.unknown_specialization(self.db);
                let unknown_pattern_member_ty = Type::instance(self.db, unknown_pattern_class)
                    .member(self.db, name.as_str())
                    .place
                    .ignore_possibly_undefined();
                // For example, `Child[int]` and `Base[T]` share a generic hierarchy, so a `Base`
                // pattern can reuse `int` from the subject. This is also the conservative fallback
                // when the subject does not determine one exact specialization of the pattern
                // subclass.
                if original_subject_ty
                    .nominal_class(self.db)
                    .is_some_and(|original_class| {
                        unknown_pattern_class.is_subtype_of_class_literal(
                            self.db,
                            original_class.class_literal(self.db),
                        ) || original_class.is_subtype_of_class_literal(
                            self.db,
                            unknown_pattern_class.class_literal(self.db),
                        )
                    })
                {
                    // The pattern class's unknown specialization loses type arguments known
                    // through the related subject type. Prefer the subject's member type when it
                    // exists, but retain a member declared only by the pattern class.
                    member_ty = Some(
                        original_member_ty
                            .or(unknown_pattern_member_ty)
                            .unwrap_or_else(Type::unknown),
                    );
                } else if let Some(pattern_member_ty) = unknown_pattern_member_ty {
                    // Unrelated classes can overlap through multiple inheritance, so retain the
                    // generic pattern class's member as a possible runtime value.
                    member_ty = Some(UnionType::from_elements(
                        self.db,
                        member_ty.into_iter().chain([pattern_member_ty]),
                    ));
                }
            }
            member_ty.or_else(|| (!subject_is_final).then_some(Type::unknown()))
        };

        context
            .positional_sources
            .iter()
            .map(|source| match source {
                ClassPatternPositionalSource::MatchSelf => Some(ClassPatternArgument {
                    ty: subject_ty,
                    source: PatternValueSource::Subject,
                }),
                ClassPatternPositionalSource::Attribute(name) => {
                    member_type(name).map(|ty| ClassPatternArgument {
                        ty,
                        source: PatternValueSource::Extracted,
                    })
                }
                ClassPatternPositionalSource::Unknown => Some(ClassPatternArgument {
                    ty: Type::unknown(),
                    source: PatternValueSource::Extracted,
                }),
            })
            .chain(kind.keywords.iter().map(|keyword| {
                member_type(&keyword.attr).map(|ty| ClassPatternArgument {
                    ty,
                    source: PatternValueSource::Extracted,
                })
            }))
            .collect()
    }

    /// Infer an exact specialization of a generic pattern subclass from a specialized base-class
    /// subject.
    ///
    /// This intentionally handles only the case where every pattern-class type variable has one
    /// exact solution. Variant base classes and pattern classes with unconstrained parameters keep
    /// the existing conservative member type.
    ///
    /// ```python
    /// class Base[T]: ...
    ///
    /// class Child[T](Base[T]):
    ///     item: T
    ///
    /// def f(value: Base[int]) -> None:
    ///     match value:
    ///         case Child(item=item):
    ///             reveal_type(item)  # int
    /// ```
    fn specialize_pattern_class_for_subject(
        &self,
        pattern_class: ClassLiteral<'db>,
        subject_class: ClassType<'db>,
    ) -> Option<ClassType<'db>> {
        let generic_context = pattern_class.generic_context(self.db)?;
        let pattern_base = pattern_class
            .identity_specialization(self.db)
            .iter_mro(self.db)
            .filter_map(ClassBase::into_class)
            .find(|base| base.class_literal(self.db) == subject_class.class_literal(self.db))?;

        let constraints = ConstraintSetBuilder::new();
        let solutions = Type::instance(self.db, pattern_base)
            .assignable_solutions_with_inferable(
                self.db,
                Type::instance(self.db, subject_class),
                generic_context.inferable_typevars(self.db),
            )
            .solve_with(|variance, path_bound| {
                let Some(lower) = path_bound.lower else {
                    return Ok(None);
                };
                if variance != TypeVarVariance::Invariant
                    || path_bound.upper.materialize_exact(self.db) != lower
                {
                    return Ok(None);
                }
                PathBounds::default_solve(self.db, &constraints, path_bound)
            });
        let Solutions::Constrained(solutions) = solutions else {
            return None;
        };
        let [solution] = solutions.as_slice() else {
            return None;
        };

        let typevars = generic_context.variables(self.db);
        let types = typevars
            .clone()
            .map(|typevar| {
                solution
                    .iter()
                    .find(|binding| binding.bound_typevar == typevar)
                    .map(|binding| binding.solution)
            })
            .collect::<Option<Vec<_>>>()?;
        if types.iter().any(|ty| {
            typevars.clone().any(|typevar| {
                ty.references_typevar(self.db, typevar.typevar(self.db).identity(self.db))
            })
        }) {
            return None;
        }
        Some(
            pattern_class
                .apply_specialization(self.db, |_| generic_context.specialize(self.db, types)),
        )
    }

    fn class_pattern_contexts(
        &self,
        kind: &ClassPatternPredicateKind<'db>,
    ) -> SmallVec<[ClassPatternContext<'db>; 2]> {
        let class_expr_ty =
            infer_same_file_expression_type(self.db, kind.class, TypeContext::default())
                .resolve_type_alias(self.db);
        let context = |class_expr_ty: Type<'db>| {
            let class = class_expr_ty.as_class_literal();
            ClassPatternContext {
                class,
                class_ty: positive_class_pattern_type(self.db, class_expr_ty)
                    .unwrap_or_else(Type::object),
                positional_sources: class.map_or_else(
                    || vec![ClassPatternPositionalSource::Unknown; kind.positional.len()],
                    |class| class_pattern_positional_sources(self.db, class, kind.positional.len()),
                ),
            }
        };
        match class_expr_ty {
            Type::Union(union) => union
                .elements(self.db)
                .iter()
                .copied()
                .map(context)
                .collect(),
            _ => smallvec![context(class_expr_ty)],
        }
    }

    fn class_pattern_arm(
        &self,
        kind: &ClassPatternPredicateKind<'db>,
        context: &ClassPatternContext<'db>,
        original_subject_ty: Type<'db>,
        subject_ty: Type<'db>,
    ) -> Option<(Type<'db>, Vec<ClassPatternArgument<'db>>)> {
        let narrowed_subject_ty =
            self.filter_class_pattern_subject_type(context.class, context.class_ty, subject_ty);
        if narrowed_subject_ty.is_never() {
            return None;
        }
        let arguments = self.class_pattern_arguments_for_arm(
            kind,
            context,
            original_subject_ty,
            subject_ty,
            narrowed_subject_ty,
        )?;
        Some((narrowed_subject_ty, arguments))
    }

    fn matched_class_pattern_subject_type(
        &self,
        kind: &ClassPatternPredicateKind<'db>,
        subject_ty: Type<'db>,
    ) -> Type<'db> {
        UnionType::from_elements(
            self.db,
            self.class_pattern_contexts(kind).iter().map(|context| {
                self.matched_class_pattern_subject_type_for_context(kind, context, subject_ty)
            }),
        )
    }

    fn matched_class_pattern_subject_type_for_context(
        &self,
        kind: &ClassPatternPredicateKind<'db>,
        context: &ClassPatternContext<'db>,
        subject_ty: Type<'db>,
    ) -> Type<'db> {
        self.analyze_matched_subject_arms(
            subject_ty,
            OriginalSubjectPreservation::EquivalentTypes,
            |analyzer, original_subject_ty, subject_ty| {
                let (narrowed_subject_ty, arguments) =
                    analyzer.class_pattern_arm(kind, context, original_subject_ty, subject_ty)?;
                let mut matched_subject_ty = narrowed_subject_ty;
                for (pattern, argument) in kind
                    .positional
                    .iter()
                    .chain(kind.keywords.iter().map(|keyword| &keyword.pattern))
                    .zip(arguments)
                {
                    let child_ty = analyzer.matched_subject_type(pattern, argument.ty);
                    if child_ty.is_never() {
                        return None;
                    }
                    if argument.source == PatternValueSource::Subject {
                        matched_subject_ty = child_ty;
                    }
                }
                Some(matched_subject_ty)
            },
        )
    }

    fn analyze_successful_class_pattern(
        &self,
        kind: &ClassPatternPredicateKind<'db>,
        subject_ty: Type<'db>,
    ) -> PatternSuccessResult<'db> {
        let mut matched_subject_types = UnionBuilder::new(self.db);
        let mut binding_subject_types = UnionBuilder::new(self.db);
        let mut bindings = BTreeMap::new();
        for context in self.class_pattern_contexts(kind) {
            let result =
                self.analyze_successful_class_pattern_for_context(kind, &context, subject_ty);
            matched_subject_types.add_in_place(result.matched_subject_ty);
            binding_subject_types.add_in_place(result.binding_subject_ty);
            Self::merge_bindings(&mut bindings, result.bindings);
        }
        PatternSuccessResult {
            matched_subject_ty: matched_subject_types.build(),
            binding_subject_ty: binding_subject_types.build(),
            bindings,
        }
    }

    fn analyze_successful_class_pattern_for_context(
        &self,
        kind: &ClassPatternPredicateKind<'db>,
        context: &ClassPatternContext<'db>,
        subject_ty: Type<'db>,
    ) -> PatternSuccessResult<'db> {
        self.analyze_pattern_subject_arms(
            subject_ty,
            OriginalSubjectPreservation::EquivalentTypes,
            |analyzer, original_subject_ty, subject_ty| {
                let (narrowed_subject_ty, arguments) =
                    analyzer.class_pattern_arm(kind, context, original_subject_ty, subject_ty)?;
                let mut matched_subject_ty = narrowed_subject_ty;
                let mut binding_subject_ty = narrowed_subject_ty;
                let mut bindings = BTreeMap::new();
                for (pattern, argument) in kind
                    .positional
                    .iter()
                    .chain(kind.keywords.iter().map(|keyword| &keyword.pattern))
                    .zip(arguments)
                {
                    let mut child = analyzer.analyze_successful_pattern(pattern, argument.ty);
                    if child.matched_subject_ty.is_never() {
                        return None;
                    }
                    if argument.source != PatternValueSource::Subject {
                        Self::demote_subject_bindings(&mut child.bindings);
                    } else {
                        matched_subject_ty = child.matched_subject_ty;
                        binding_subject_ty = child.binding_subject_ty;
                    }
                    Self::merge_bindings(&mut bindings, child.bindings);
                }

                Some(PatternSuccessResult {
                    matched_subject_ty,
                    binding_subject_ty,
                    bindings,
                })
            },
        )
    }

    fn mapping_pattern_value_type_for_arm(
        &self,
        subject_ty: Type<'db>,
        key_ty: Type<'db>,
    ) -> Option<Type<'db>> {
        if let Type::TypedDict(typed_dict) = subject_ty.resolve_type_alias(self.db) {
            let key_ty = key_ty.resolve_type_alias(self.db);
            let typed_dict_key_ty = typed_dict.key_type(self.db);
            if typed_dict_key_ty.is_never()
                || equality_truthiness(self.db, typed_dict_key_ty, key_ty)
                    == Truthiness::AlwaysFalse
            {
                return None;
            }
            if let Some(key) = key_ty.as_string_literal() {
                return typed_dict
                    .item(self.db, key.value(self.db))
                    .map(|field| field.declared_ty)
                    .or_else(|| {
                        typed_dict
                            .openness(self.db)
                            .is_implicitly_open()
                            .then_some(Type::object())
                    });
            }
            return Some(typed_dict.value_type(self.db));
        }

        let Some((_, mapping_value_ty)) = subject_ty.unpack_keys_and_items(self.db) else {
            return Some(Type::unknown());
        };
        let Some(get_method) = subject_ty
            .member(self.db, "get")
            .place
            .ignore_possibly_undefined()
        else {
            return Some(Type::unknown());
        };
        let default_ty = if self.mapping_pattern_uses_standard_get(subject_ty) {
            mapping_value_ty
        } else {
            Type::object()
        };
        Some(
            get_method
                .try_call(self.db, &CallArguments::positional([key_ty, default_ty]))
                .map(|bindings| bindings.return_type(self.db))
                .unwrap_or_else(|error| error.return_type(self.db)),
        )
    }

    fn mapping_pattern_uses_standard_get(&self, subject_ty: Type<'db>) -> bool {
        let Some(class) = subject_ty.nominal_class(self.db) else {
            return false;
        };
        for base in class.iter_mro(self.db) {
            let class = match base {
                ClassBase::Class(class) => class,
                ClassBase::Generic | ClassBase::Protocol => continue,
                ClassBase::Any
                | ClassBase::Dynamic(_)
                | ClassBase::Divergent(_)
                | ClassBase::TypedDict(_) => {
                    return false;
                }
            };
            if !class.own_instance_member(self.db, "get").is_undefined() {
                return false;
            }
            if class.own_class_member(self.db, None, "get").is_undefined() {
                continue;
            }
            return matches!(
                class.known(self.db),
                Some(KnownClass::Dict | KnownClass::Mapping)
            );
        }
        false
    }

    fn mapping_pattern_key_types(&self, kind: &MappingPatternPredicateKind<'db>) -> Vec<Type<'db>> {
        kind.entries
            .iter()
            .map(|entry| {
                infer_same_file_expression_type(self.db, entry.key, TypeContext::default())
            })
            .collect()
    }

    fn mapping_pattern_arm(
        &self,
        subject_ty: Type<'db>,
        key_types: &[Type<'db>],
    ) -> Option<(Type<'db>, Vec<Type<'db>>)> {
        let narrowed_subject_ty = self.intersect_types(subject_ty, mapping_pattern_type(self.db));
        if narrowed_subject_ty.is_never() {
            return None;
        }
        let value_types = key_types
            .iter()
            .map(|key_ty| self.mapping_pattern_value_type_for_arm(narrowed_subject_ty, *key_ty))
            .collect::<Option<Vec<_>>>()?;
        Some((narrowed_subject_ty, value_types))
    }

    fn matched_mapping_pattern_subject_type(
        &self,
        kind: &MappingPatternPredicateKind<'db>,
        subject_ty: Type<'db>,
    ) -> Type<'db> {
        let key_types = self.mapping_pattern_key_types(kind);
        self.analyze_matched_subject_arms(
            subject_ty,
            OriginalSubjectPreservation::EquivalentTypes,
            |analyzer, _, subject_ty| {
                let (narrowed_subject_ty, value_types) =
                    analyzer.mapping_pattern_arm(subject_ty, &key_types)?;
                for (entry, value_ty) in kind.entries.iter().zip(value_types) {
                    if analyzer
                        .matched_subject_type(&entry.pattern, value_ty)
                        .is_never()
                    {
                        return None;
                    }
                }
                Some(narrowed_subject_ty)
            },
        )
    }

    fn analyze_successful_mapping_pattern(
        &self,
        kind: &MappingPatternPredicateKind<'db>,
        subject_ty: Type<'db>,
    ) -> PatternSuccessResult<'db> {
        let key_types = self.mapping_pattern_key_types(kind);
        self.analyze_pattern_subject_arms(
            subject_ty,
            OriginalSubjectPreservation::EquivalentTypes,
            |analyzer, _, subject_ty| {
                let (narrowed_subject_ty, value_types) =
                    analyzer.mapping_pattern_arm(subject_ty, &key_types)?;
                let mut bindings = BTreeMap::new();
                for (entry, value_ty) in kind.entries.iter().zip(value_types) {
                    let mut child = analyzer.analyze_successful_pattern(&entry.pattern, value_ty);
                    if child.matched_subject_ty.is_never() {
                        return None;
                    }
                    Self::demote_subject_bindings(&mut child.bindings);
                    Self::merge_bindings(&mut bindings, child.bindings);
                }

                if let Some(place) = kind
                    .rest
                    .as_ref()
                    .and_then(|name| analyzer.places().symbol_id(name.as_str()))
                {
                    Self::merge_binding(
                        &mut bindings,
                        place.into(),
                        PatternBindingTypes::extracted(
                            analyzer.mapping_pattern_rest_type_for_arm(narrowed_subject_ty),
                        ),
                    );
                }

                Some(PatternSuccessResult {
                    matched_subject_ty: narrowed_subject_ty,
                    binding_subject_ty: narrowed_subject_ty,
                    bindings,
                })
            },
        )
    }

    fn mapping_pattern_rest_type_for_arm(&self, subject_ty: Type<'db>) -> Type<'db> {
        let (key_ty, value_ty) = match subject_ty.resolve_type_alias(self.db) {
            Type::TypedDict(_) => (KnownClass::Str.to_instance(self.db), Type::object()),
            _ => subject_ty
                .unpack_keys_and_items(self.db)
                .unwrap_or_else(|| (Type::unknown(), Type::unknown())),
        };
        KnownClass::Dict.to_specialized_instance(self.db, &[key_ty, value_ty])
    }

    fn matched_sequence_pattern_subject_type(
        &self,
        kind: &SequencePatternPredicateKind<'db>,
        subject_ty: Type<'db>,
    ) -> Type<'db> {
        let target_len = Self::sequence_pattern_target_len(kind);
        let sequence_ty = sequence_pattern_type_builder(self.db).build();
        self.analyze_matched_subject_arms(
            subject_ty,
            OriginalSubjectPreservation::TypeVariablesOnly,
            |analyzer, _, subject_ty| {
                let (narrowed_subject_ty, element_types) =
                    analyzer.sequence_pattern_arm(subject_ty, target_len, sequence_ty)?;
                let mut persistent_element_types = Vec::with_capacity(kind.patterns.len());
                for (pattern, element_ty) in kind.patterns.iter().zip(element_types) {
                    let child = analyzer.analyze_successful_pattern(pattern, element_ty);
                    if child.matched_subject_ty.is_never() {
                        return None;
                    }
                    // Use the mutation-safe type so an exact tuple does not retain stale facts
                    // about a mutable sequence stored in one of its elements.
                    persistent_element_types.push(child.binding_subject_ty);
                }
                Some(analyzer.successful_sequence_subject_type(
                    kind,
                    subject_ty,
                    narrowed_subject_ty,
                    &persistent_element_types,
                ))
            },
        )
    }

    /// Return the subject and binding types of a successful sequence pattern.
    ///
    /// Each union member is checked against the complete pattern before element types are combined.
    /// This keeps related tuple elements together:
    ///
    /// ```python
    /// from typing import Literal
    ///
    /// def f(value: tuple[Literal[1], int] | tuple[Literal[2], str]) -> None:
    ///     match value:
    ///         case [1, item]:
    ///             reveal_type(item)  # int
    /// ```
    fn analyze_successful_sequence_pattern(
        &self,
        kind: &SequencePatternPredicateKind<'db>,
        subject_ty: Type<'db>,
    ) -> PatternSuccessResult<'db> {
        let target_len = Self::sequence_pattern_target_len(kind);
        let sequence_ty = sequence_pattern_type_builder(self.db).build();
        self.analyze_pattern_subject_arms(
            subject_ty,
            OriginalSubjectPreservation::TypeVariablesOnly,
            |analyzer, _, subject_ty| {
                let (narrowed_subject_ty, element_types) =
                    analyzer.sequence_pattern_arm(subject_ty, target_len, sequence_ty)?;
                let mut bindings = BTreeMap::new();
                let mut matched_element_types = Vec::with_capacity(kind.patterns.len());
                let mut binding_element_types = Vec::with_capacity(kind.patterns.len());
                for (pattern, element_ty) in kind.patterns.iter().zip(element_types) {
                    let mut child = analyzer.analyze_successful_pattern(pattern, element_ty);
                    if child.matched_subject_ty.is_never() {
                        return None;
                    }
                    matched_element_types.push(child.matched_subject_ty);
                    binding_element_types.push(child.binding_subject_ty);
                    Self::demote_subject_bindings(&mut child.bindings);
                    Self::merge_bindings(&mut bindings, child.bindings);
                }
                let matched_subject_ty = analyzer.successful_sequence_subject_type(
                    kind,
                    subject_ty,
                    narrowed_subject_ty,
                    &matched_element_types,
                );
                let binding_subject_ty = analyzer.successful_sequence_binding_type(
                    kind,
                    subject_ty,
                    &binding_element_types,
                );
                Some(PatternSuccessResult {
                    matched_subject_ty,
                    binding_subject_ty,
                    bindings,
                })
            },
        )
    }

    /// Return the type established while a sequence pattern is being evaluated.
    ///
    /// Exact tuples can refine their tuple element types with the facts established by successful
    /// child patterns. For other sequences, retain the observed length and indexed-element facts
    /// while type-checking the successful case branch.
    fn successful_sequence_subject_type(
        &self,
        kind: &SequencePatternPredicateKind<'db>,
        subject_ty: Type<'db>,
        narrowed_subject_ty: Type<'db>,
        matched_element_types: &[Type<'db>],
    ) -> Type<'db> {
        if kind.split_around_star().is_none()
            && let Some(refined) =
                refine_exact_tuple_for_sequence_pattern(self.db, subject_ty, matched_element_types)
        {
            return refined;
        }

        self.intersect_types(
            narrowed_subject_ty,
            self.successful_sequence_pattern_type(kind, matched_element_types),
        )
    }

    /// Return the sequence type safe to assign to a binding created by the pattern.
    ///
    /// An exact tuple encodes immutable length and element types, so it can retain facts established
    /// by successful child patterns. Other sequence types retain only sequence-pattern eligibility;
    /// observed length and indexed element facts could become stale after mutation or stateful
    /// access.
    fn successful_sequence_binding_type(
        &self,
        kind: &SequencePatternPredicateKind<'db>,
        subject_ty: Type<'db>,
        binding_element_types: &[Type<'db>],
    ) -> Type<'db> {
        if kind.split_around_star().is_none()
            && let Some(refined) =
                refine_exact_tuple_for_sequence_pattern(self.db, subject_ty, binding_element_types)
        {
            return refined;
        }

        if subject_ty.exact_tuple_instance_spec(self.db).is_some() {
            self.intersect_types(
                subject_ty,
                self.successful_sequence_pattern_type(kind, binding_element_types),
            )
        } else {
            self.intersect_types(subject_ty, sequence_pattern_type_builder(self.db).build())
        }
    }

    fn successful_sequence_pattern_type(
        &self,
        kind: &SequencePatternPredicateKind<'db>,
        matched_element_types: &[Type<'db>],
    ) -> Type<'db> {
        if let Some((prefix, suffix)) = kind.split_around_star() {
            let prefix_types = matched_element_types.iter().copied().take(prefix.len());
            let suffix_types = matched_element_types
                .iter()
                .copied()
                .skip(matched_element_types.len().saturating_sub(suffix.len()));
            starred_sequence_pattern_type(self.db, prefix_types, suffix_types)
        } else {
            exact_sequence_pattern_type(self.db, matched_element_types.iter().copied())
        }
    }

    /// Check one subject union member against the complete sequence pattern.
    ///
    /// Return the narrowed sequence and its elements when the length and every element can match.
    /// Return `None` when any part cannot match, so callers can discard the whole union member
    /// before combining capture types.
    fn sequence_pattern_arm(
        &self,
        subject_ty: Type<'db>,
        target_len: TupleLength,
        sequence_ty: Type<'db>,
    ) -> Option<(Type<'db>, Vec<Type<'db>>)> {
        let narrowed_subject_ty = self.intersect_types(subject_ty, sequence_ty);
        if narrowed_subject_ty.is_never() {
            return None;
        }

        let tuple = subject_ty.try_iterate(self.db).unwrap_or_else(|error| {
            let fallback_element_ty = error.fallback_element_type(self.db);
            Cow::Owned(TupleSpec::homogeneous(
                if fallback_element_ty.is_unknown() {
                    Type::object()
                } else {
                    fallback_element_ty
                },
            ))
        });
        let mut unpacker = TupleUnpacker::new(self.db, target_len);
        unpacker.unpack_tuple(tuple.as_ref()).ok()?;
        Some((narrowed_subject_ty, unpacker.into_types().collect()))
    }

    fn analyze_matched_subject_arms(
        &self,
        subject_ty: Type<'db>,
        preservation: OriginalSubjectPreservation,
        analyze_arm: impl Fn(&Self, Type<'db>, Type<'db>) -> Option<Type<'db>>,
    ) -> Type<'db> {
        let subject_arms = self.match_pattern_subject_arms(subject_ty);
        let grouped_arms = subject_arms
            .into_iter()
            .chunk_by(|(original_subject_ty, _)| *original_subject_ty);
        let mut matched_subject_types = UnionBuilder::new(self.db);

        for (original_subject_ty, arms) in &grouped_arms {
            let matched_types = UnionType::from_elements(
                self.db,
                arms.filter_map(|(_, filtering_subject_ty)| {
                    analyze_arm(self, original_subject_ty, filtering_subject_ty)
                }),
            );
            matched_subject_types.add_in_place(self.preserve_original_subject_type(
                original_subject_ty,
                matched_types,
                preservation,
            ));
        }

        matched_subject_types.build()
    }

    fn analyze_pattern_subject_arms(
        &self,
        subject_ty: Type<'db>,
        preservation: OriginalSubjectPreservation,
        analyze_arm: impl Fn(&Self, Type<'db>, Type<'db>) -> Option<PatternSuccessResult<'db>>,
    ) -> PatternSuccessResult<'db> {
        let subject_arms = self.match_pattern_subject_arms(subject_ty);
        let grouped_arms = subject_arms
            .into_iter()
            .chunk_by(|(original_subject_ty, _)| *original_subject_ty);
        let mut matched_subject_types = UnionBuilder::new(self.db);
        let mut binding_subject_types = UnionBuilder::new(self.db);
        let mut bindings = BTreeMap::new();

        for (original_subject_ty, arms) in &grouped_arms {
            let mut matched_types = UnionBuilder::new(self.db);
            let mut binding_types = UnionBuilder::new(self.db);
            let mut arm_bindings = BTreeMap::new();

            for (_, filtering_subject_ty) in arms {
                if let Some(arm) = analyze_arm(self, original_subject_ty, filtering_subject_ty) {
                    matched_types.add_in_place(arm.matched_subject_ty);
                    binding_types.add_in_place(arm.binding_subject_ty);
                    Self::merge_bindings(&mut arm_bindings, arm.bindings);
                }
            }

            for binding in arm_bindings.values_mut() {
                let subject_ty = binding.subject_ty(self.db);
                if !subject_ty.is_never() {
                    binding.restore_subject(self.preserve_original_subject_type(
                        original_subject_ty,
                        subject_ty,
                        preservation,
                    ));
                }
            }
            Self::merge_bindings(&mut bindings, arm_bindings);

            matched_subject_types.add_in_place(self.preserve_original_subject_type(
                original_subject_ty,
                matched_types.build(),
                preservation,
            ));
            binding_subject_types.add_in_place(self.preserve_original_subject_type(
                original_subject_ty,
                binding_types.build(),
                preservation,
            ));
        }

        let matched_subject_ty = matched_subject_types.build();
        PatternSuccessResult {
            matched_subject_ty,
            binding_subject_ty: binding_subject_types.build(),
            bindings,
        }
    }

    fn preserve_original_subject_type(
        &self,
        original_subject_ty: Type<'db>,
        filtered_ty: Type<'db>,
        preservation: OriginalSubjectPreservation,
    ) -> Type<'db> {
        let filtering_ty = self.pattern_filtering_type(original_subject_ty);
        if filtered_ty.is_equivalent_to(self.db, filtering_ty)
            && (matches!(preservation, OriginalSubjectPreservation::EquivalentTypes)
                || original_subject_ty.has_typevar(self.db))
        {
            original_subject_ty
        } else if original_subject_ty.has_typevar(self.db) {
            self.intersect_types(original_subject_ty, filtered_ty)
        } else {
            filtered_ty
        }
    }

    /// Pair each original subject type with the union members used to test the pattern.
    ///
    /// An upper-bounded type variable uses its bound for matching, but each arm keeps the original
    /// type so a successful result can preserve that type variable. Constrained type variables are
    /// left opaque; selecting and correlating one of their constraints is handled separately.
    fn match_pattern_subject_arms(
        &self,
        subject_ty: Type<'db>,
    ) -> SmallVec<[(Type<'db>, Type<'db>); 2]> {
        let subject_ty = subject_ty.resolve_type_alias(self.db);
        let mut arms = SmallVec::new();
        let mut add_arm = |original_subject_ty: Type<'db>| {
            let filtering_subject_ty = self.pattern_filtering_type(original_subject_ty);
            match filtering_subject_ty {
                Type::Union(union) => arms.extend(
                    union
                        .elements(self.db)
                        .iter()
                        .map(|element| (original_subject_ty, *element)),
                ),
                _ => arms.push((original_subject_ty, filtering_subject_ty)),
            }
        };

        match subject_ty {
            Type::Union(union) => union
                .elements(self.db)
                .iter()
                .copied()
                .for_each(&mut add_arm),
            _ => add_arm(subject_ty),
        }

        arms
    }

    fn pattern_filtering_type(&self, ty: Type<'db>) -> Type<'db> {
        let ty = ty.resolve_type_alias(self.db);
        if let Type::TypeVar(typevar) = ty
            && let Some(bound) = typevar.typevar(self.db).upper_bound(self.db)
        {
            bound.resolve_type_alias(self.db)
        } else {
            ty
        }
    }

    fn sequence_pattern_target_len(kind: &SequencePatternPredicateKind<'db>) -> TupleLength {
        if let Some((prefix, suffix)) = kind.split_around_star() {
            TupleLength::Variable(prefix.len(), suffix.len())
        } else {
            TupleLength::Fixed(kind.patterns.len())
        }
    }

    fn intersect_types(&self, left: Type<'db>, right: Type<'db>) -> Type<'db> {
        IntersectionBuilder::new(self.db)
            .add_positive(left)
            .add_positive(right)
            .build()
    }

    fn places(&self) -> &'db PlaceTable {
        place_table(self.db, self.scope)
    }
}

impl<'db> NarrowingConstraintsBuilder<'db, '_> {
    fn evaluate_subject_element_pattern(
        &mut self,
        subject_element: SubjectElementPatternPredicate<'db>,
    ) -> Option<NarrowingConstraints<'db>> {
        let pattern = subject_element.pattern;
        let subject_expression = pattern.subject(self.db);
        let subject = subject_expression.node_ref(self.db).node(self.module);
        self.evaluate_match_pattern_for_subject_element(
            subject_expression,
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
            PredicateNode::IsNonEmptyIterable(expression) => expression.scope(self.db),
            PredicateNode::StarImportPlaceholder(definition) => definition.scope(self.db),
        }
    }

    fn comparison_soundness_policy(&self) -> ComparisonSoundnessPolicy {
        ComparisonSoundnessPolicy::from_strict_literal_narrowing(
            self.db
                .analysis_settings(self.scope().file(self.db))
                .strict_literal_narrowing,
        )
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
            Type::TypeVar(typevar) => {
                let Some(bound_or_constraints) = typevar.typevar(db).bound_or_constraints(db)
                else {
                    return ty;
                };

                let upper_bound = bound_or_constraints.as_type(db);
                let narrowed_upper_bound = match bound_or_constraints {
                    TypeVarBoundOrConstraints::UpperBound(bound) => {
                        Self::narrow_type_by_exact_len(db, bound, length, is_equality)
                    }
                    TypeVarBoundOrConstraints::Constraints(constraints) => {
                        UnionType::from_elements(
                            db,
                            constraints.elements(db).iter().map(|constraint| {
                                Self::narrow_type_by_exact_len(db, *constraint, length, is_equality)
                            }),
                        )
                    }
                };

                if narrowed_upper_bound == upper_bound {
                    resolved
                } else {
                    IntersectionType::from_two_elements(db, resolved, narrowed_upper_bound)
                }
            }
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
        expression: Expression<'db>,
        is_positive: bool,
    ) -> Option<NarrowingConstraints<'db>> {
        let target_constraints = self.evaluate_simple_expr(&expr_named.target, is_positive);
        let mut value_constraints =
            self.evaluate_expression_node_predicate(&expr_named.value, expression, is_positive);

        if let Some(value_constraints) = value_constraints.as_mut()
            && let Some(target) = PlaceExpr::try_from_expr(&expr_named.target)
            && let Some(target_place) = self.places().place_id(&target)
        {
            let places = self.places();
            // The target is rebound after the value is evaluated, invalidating constraints on the
            // target and any member places rooted at it.
            value_constraints.retain(|place, _| {
                *place != target_place
                    && !places
                        .parents(places.place(*place))
                        .any(|parent| parent == target_place)
            });
        }

        match (target_constraints, value_constraints) {
            (Some(mut target), Some(value)) => {
                merge_constraints_and(&mut target, value);
                Some(target)
            }
            (Some(constraints), None) | (None, Some(constraints)) => Some(constraints),
            (None, None) => None,
        }
    }

    fn evaluate_expr_in(&self, lhs_ty: Type<'db>, rhs_ty: Type<'db>) -> Option<Type<'db>> {
        let rhs_ty = rhs_ty.resolve_type_alias(self.db);

        // The supported containers compare against their iterated elements, so union arms can be
        // combined. String membership also accepts multi-character substrings, so evaluate literal
        // haystacks separately, including when they occur in a union.
        if let Some(haystack) = rhs_ty.as_string_literal() {
            return narrow_string_membership(self.db, lhs_ty, haystack.value(self.db), true);
        }
        if let Type::Union(union) = rhs_ty
            && union.elements(self.db).iter().any(|element| {
                element
                    .resolve_type_alias(self.db)
                    .as_string_literal()
                    .is_some()
            })
        {
            let mut builder = UnionBuilder::new(self.db);
            for element in union.elements(self.db) {
                builder = builder.add(self.evaluate_expr_in(lhs_ty, *element)?);
            }
            let narrowed = builder.build();
            return (narrowed != lhs_ty).then_some(narrowed);
        }
        let membership_type = elements_of(self.db, rhs_ty)?;
        let iterable = membership_type.try_iterate(self.db).ok()?;

        if iterable
            .as_fixed_length()
            .is_some_and(|fixed| fixed.all_elements().is_empty())
        {
            return Some(Type::Never);
        }
        let rhs_values = iterable
            .homogeneous_element_type(self.db)
            .resolve_type_alias(self.db);
        let soundness_policy = self.comparison_soundness_policy();

        if let Type::Union(union) = rhs_values {
            let mut builder = UnionBuilder::new(self.db);
            for rhs_value in union.elements(self.db) {
                builder = builder.add(evaluate_type_equality(
                    self.db,
                    lhs_ty,
                    *rhs_value,
                    true,
                    soundness_policy,
                )?);
            }
            Some(builder.build())
        } else {
            evaluate_type_equality(self.db, lhs_ty, rhs_values, true, soundness_policy)
        }
    }

    fn evaluate_expr_not_in(&self, lhs_ty: Type<'db>, rhs_ty: Type<'db>) -> Option<Type<'db>> {
        if let Some(haystack) = rhs_ty.resolve_type_alias(self.db).as_string_literal() {
            return narrow_string_membership(self.db, lhs_ty, haystack.value(self.db), false);
        }
        let membership_type = elements_of(self.db, rhs_ty)?;
        let iterable = membership_type.try_iterate(self.db).ok()?;
        let fixed_length = iterable.as_fixed_length()?;
        let mut builder = IntersectionBuilder::new(self.db);
        let mut constrained = false;

        // `not in` negates equality with every element; it does not use `__ne__`. Only add an
        // exclusion when every value represented by a slot is known to compare equal.
        for element_ty in fixed_length.all_elements().iter().copied() {
            if let Some(constraint) = equality_exclusion_constraint(self.db, element_ty) {
                builder = builder.add_positive(constraint);
                constrained = true;
            }
        }

        constrained.then(|| builder.build())
    }

    /// Preserve the precise element types of an immediately consumed list or set literal.
    ///
    /// These expressions cannot be mutated before membership is evaluated. Representing them as
    /// fixed-length tuples also lets negative narrowing exclude values that are guaranteed present.
    fn inline_membership_rhs_type(
        &self,
        rhs: &ast::Expr,
        inference: &ExpressionInference<'db>,
    ) -> Option<Type<'db>> {
        let elements = match rhs.expression_value() {
            ast::Expr::List(list) => &list.elts,
            ast::Expr::Set(set) => &set.elts,
            _ => return None,
        };

        if elements.iter().any(ast::Expr::is_starred_expr) {
            return None;
        }

        Some(Type::heterogeneous_tuple(
            self.db,
            elements
                .iter()
                .map(|element| inference.expression_type(element)),
        ))
    }

    fn evaluate_expr_compare_op(
        &mut self,
        lhs_ty: Type<'db>,
        rhs_ty: Type<'db>,
        op: ast::CmpOp,
        is_positive: bool,
    ) -> Option<Type<'db>> {
        if op == ast::CmpOp::Eq {
            return evaluate_type_equality(
                self.db,
                lhs_ty,
                rhs_ty,
                is_positive,
                self.comparison_soundness_policy(),
            );
        }
        if op == ast::CmpOp::NotEq {
            return evaluate_type_inequality(
                self.db,
                lhs_ty,
                rhs_ty,
                is_positive,
                self.comparison_soundness_policy(),
            );
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
                        | SubclassOfInner::Dynamic(_)
                        | SubclassOfInner::Protocol(_) => None,
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
            let lhs_narrowing_rhs_ty = if matches!(op, ast::CmpOp::In | ast::CmpOp::NotIn) {
                self.inline_membership_rhs_type(right, inference)
                    .unwrap_or(rhs_ty)
            } else {
                rhs_ty
            };

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
                && let Some(ty) =
                    self.evaluate_expr_compare_op(lhs_ty, lhs_narrowing_rhs_ty, *op, is_positive)
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

    fn evaluate_negative_match_pattern_singleton(
        &mut self,
        subject: Expression<'db>,
        singleton: ast::Singleton,
    ) -> Option<NarrowingConstraints<'db>> {
        let subject = PlaceExpr::try_from_expr(subject.node_ref(self.db).node(self.module))?;
        let place = self.expect_place(&subject);

        let ty = singleton_pattern_type(self.db, singleton).negate(self.db);
        Some(NarrowingConstraints::from_iter([(
            place,
            NarrowingConstraint::intersection(ty),
        )]))
    }

    fn evaluate_negative_match_pattern(
        &self,
        subject: Expression<'db>,
        pattern: &PatternPredicateKind<'db>,
    ) -> Option<NarrowingConstraints<'db>> {
        let subject_place = PlaceExpr::try_from_expr(subject.node_ref(self.db).node(self.module))?;
        let place = self.expect_place(&subject_place);
        let subject_ty = infer_same_file_expression_type(self.db, subject, TypeContext::default());
        let definitely_matched =
            definite_match_pattern_type_for_subject(self.db, pattern, subject_ty);
        if definitely_matched.is_never() {
            return None;
        }

        Some(NarrowingConstraints::from_iter([(
            place,
            NarrowingConstraint::intersection(definitely_matched.negate(self.db)),
        )]))
    }

    fn evaluate_negative_match_pattern_sequence(
        &mut self,
        subject: Expression<'db>,
        kind: &SequencePatternPredicateKind<'db>,
        pattern: &PatternPredicateKind<'db>,
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
                subject, elements, kind, false, None,
            );
        }

        let Some(subject_place) = PlaceExpr::try_from_expr(subject_node) else {
            return PatternNarrowingResult::Possible(None);
        };

        let subject_ty = infer_same_file_expression_type(self.db, subject, TypeContext::default());
        let narrowed_ty = pattern_binding_fallthrough_type(self.db, pattern, subject_ty);
        if narrowed_ty == subject_ty {
            return PatternNarrowingResult::Possible(None);
        }

        let place = self.expect_place(&subject_place);

        PatternNarrowingResult::Possible(Some(NarrowingConstraints::from_iter([(
            place,
            NarrowingConstraint::intersection(narrowed_ty),
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
        subject_expression: Expression<'db>,
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
            match self.evaluate_match_pattern_for_subject_element(
                subject_expression,
                element,
                pattern,
                target,
            ) {
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
        subject_expression: Expression<'db>,
        subject: &ast::Expr,
        pattern: &PatternPredicateKind<'db>,
        target: Option<ExpressionNodeKey>,
    ) -> PatternNarrowingResult<'db> {
        if let Some(elements) = Self::sequence_expression_elements(subject) {
            return match pattern {
                PatternPredicateKind::Sequence(kind) => self
                    .evaluate_match_pattern_sequence_for_subject_element(
                        subject_expression,
                        elements,
                        kind,
                        true,
                        target,
                    ),
                PatternPredicateKind::As(Some(pattern), _) => self
                    .evaluate_match_pattern_for_subject_element(
                        subject_expression,
                        subject,
                        pattern,
                        target,
                    ),
                PatternPredicateKind::Or(patterns) => PatternNarrowingResult::merge_alternatives(
                    patterns.iter().map(|pattern| {
                        self.evaluate_match_pattern_for_subject_element(
                            subject_expression,
                            subject,
                            pattern,
                            target,
                        )
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
        let subject_ty =
            infer_expression_types(self.db, subject_expression, TypeContext::default())
                .expression_type(subject_expr);
        let Some(constraint) = self.positive_subject_constraint(pattern, subject_ty) else {
            return PatternNarrowingResult::Possible(None);
        };
        if NarrowingConstraint::intersection(subject_ty)
            .merge_constraint_and(constraint.clone())
            .evaluate_constraint_type(self.db)
            .is_never()
        {
            return PatternNarrowingResult::Impossible;
        }
        if let Some(target) = target
            && ExpressionNodeKey::from(subject_expr) != target
        {
            return PatternNarrowingResult::Possible(None);
        }
        PatternNarrowingResult::Possible(Some(NarrowingConstraints::from_iter([(
            self.expect_place(&subject),
            constraint,
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
                        merge_constraints_or(first, rest_constraint?);
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

    /// Narrow tagged unions of `TypedDict`s based on the truthiness of a `Literal` key.
    fn narrow_typeddict_subscript_by_truthiness(
        &self,
        subscript_value_type: Type<'db>,
        subscript_value_expr: &ast::Expr,
        subscript_key_type: Type<'db>,
        is_positive: bool,
    ) -> Option<(ScopedPlaceId, NarrowingConstraint<'db>)> {
        if !is_or_contains_typeddict(self.db, subscript_value_type) {
            return None;
        }
        let subscript_place_expr = PlaceExpr::try_from_expr(subscript_value_expr)?;
        let key_literal = subscript_key_type.as_string_literal()?;

        let excluded_field_type = if is_positive {
            Type::AlwaysFalsy
        } else {
            Type::AlwaysTruthy
        };
        let field = TypedDictFieldBuilder::new(excluded_field_type)
            .required(false)
            .read_only(true)
            .build();
        let schema = TypedDictSchema::from_iter([(Name::from(key_literal.value(self.db)), field)]);
        let synthesized_typeddict = TypedDictType::from_schema_items(self.db, schema);
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

    fn narrow_nominal_attribute_by_truthiness(
        &self,
        attribute_value_type: Type<'db>,
        attribute_value_expr: &ast::Expr,
        attribute_name: &str,
        is_positive: bool,
    ) -> Option<(ScopedPlaceId, NarrowingConstraint<'db>)> {
        let Type::Union(union) = attribute_value_type.resolve_type_alias(self.db) else {
            return None;
        };

        let narrowed = union.filter(self.db, |element| {
            nominal_attribute_type(self.db, *element, attribute_name).is_none_or(|attribute_type| {
                let truthiness = attribute_type.bool(self.db);
                if is_positive {
                    !truthiness.is_always_false()
                } else {
                    !truthiness.is_always_true()
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
        Parameters::standard([
            Parameter::positional_only(Some(Name::new_static("self"))),
            Parameter::positional_only(Some(Name::new_static("key")))
                .with_annotated_type(Type::string_literal(db, key)),
        ]),
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
