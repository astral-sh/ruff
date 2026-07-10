//! Equality and inequality reasoning for type narrowing and reachability.
//!
//! This module evaluates comparisons with statically known Python semantics, producing branch
//! constraints and definite truthiness while remaining conservative around custom comparison
//! methods.

use ruff_python_ast::name::Name;
use rustc_hash::FxHashSet;

use crate::{Db, place::PlaceAndQualifiers};

use super::{
    EnumLiteralType, IntersectionBuilder, KnownBoundMethodType, KnownClass, LiteralValueType,
    LiteralValueTypeKind, MemberLookupPolicy, Truthiness, Type, TypeVarBoundOrConstraints,
    UnionBuilder,
    enums::{enum_member_literals, enum_metadata},
};

mod enums;

use self::enums::evaluate_enum_domains;

/// The result of evaluating a runtime comparison between two types.
///
/// Definite truthiness is represented separately from a constraint for the operand currently being
/// narrowed. A comparison can therefore be ambiguous at runtime while still constraining that
/// operand in either branch.
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
enum ComparisonResult<'db> {
    /// The comparison always evaluates to true.
    ///
    /// For equality comparisons, this does not necessarily indicate anything about whether the
    /// two types are the same type, or even whether they have any subtyping or assignability
    /// relationship. For example, an object of type `Literal[1]` will always compare equal to an
    /// object of type `Literal[Foo.X]` in the following example, despite the fact that
    /// `Literal[1]` is disjoint from `Literal[Foo.X]`:
    ///
    /// ```python
    /// from enum import IntEnum
    ///
    /// class Foo(IntEnum):
    ///     X = 1
    /// ```
    AlwaysTrue,

    /// The comparison always evaluates to false.
    ///
    /// Similar to [`Self::AlwaysTrue`], this only describes the runtime comparison result; it does not
    /// necessarily indicate whether the two types are disjoint.
    AlwaysFalse,

    /// The comparison allows the operand being constrained to be narrowed to the wrapped type.
    ///
    /// For example, if an object of type `LiteralString` compares equal to an object of type
    /// `Literal["foo"]`, the equality branch can safely narrow either operand to `Literal["foo"]`.
    CanNarrow(Type<'db>),

    /// The comparison may evaluate to true or false, depending on runtime values.
    Ambiguous,
}

/// The branch of a comparison for which a narrowing constraint is being computed.
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
enum ComparisonBranch {
    Positive,
    Negative,
}

/// The role of a literal operand in the comparison being evaluated.
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
enum LiteralOperand {
    Target,
    Other,
}

impl From<bool> for ComparisonBranch {
    fn from(is_positive: bool) -> Self {
        if is_positive {
            Self::Positive
        } else {
            Self::Negative
        }
    }
}

impl<'db> ComparisonResult<'db> {
    fn from_bool(value: bool) -> Self {
        if value {
            ComparisonResult::AlwaysTrue
        } else {
            ComparisonResult::AlwaysFalse
        }
    }

    /// Convert this result into a constraint for a branch with the given truthiness.
    fn constraint(self, branch: ComparisonBranch) -> Option<Type<'db>> {
        match self {
            ComparisonResult::AlwaysTrue => {
                (branch == ComparisonBranch::Negative).then_some(Type::Never)
            }
            ComparisonResult::AlwaysFalse => {
                (branch == ComparisonBranch::Positive).then_some(Type::Never)
            }
            ComparisonResult::CanNarrow(narrowed) => Some(narrowed),
            ComparisonResult::Ambiguous => None,
        }
    }

    /// Preserve definite truthiness while discarding a conditional narrowing result.
    ///
    /// This is necessary when a comparison is evaluated through a runtime-equivalent type whose
    /// static identity must be preserved. For example, a `NewType` instance has the comparison
    /// behavior of its concrete base type, but a constraint derived for that base type cannot be
    /// applied to the distinct `NewType`.
    fn discard_narrowing(self) -> Self {
        match self {
            ComparisonResult::CanNarrow(_) => ComparisonResult::Ambiguous,
            result => result,
        }
    }
}

/// Return a constraint for `left` in a branch where `left == right` has the given truthiness.
///
/// Returns `None` when the comparison behavior of either operand is not precise enough to safely
/// constrain `left`.
pub(super) fn evaluate_type_equality<'db>(
    db: &'db dyn Db,
    left: Type<'db>,
    right: Type<'db>,
    is_positive: bool,
    soundness_policy: ComparisonSoundnessPolicy,
) -> Option<Type<'db>> {
    let branch = ComparisonBranch::from(is_positive);
    let condition_expects_equality =
        ComparisonOperator::Equality.condition_expects_equality(branch);
    enum_literal_constraint(
        db,
        left,
        right,
        ComparisonOperator::Equality,
        condition_expects_equality,
    )
    .or_else(|| {
        builtin_literal_constraint(
            db,
            left,
            right,
            ComparisonOperator::Equality,
            condition_expects_equality,
        )
    })
    .or_else(|| {
        evaluate_enum_domains(db, left, right, branch, ComparisonOperator::Equality)
            .and_then(|result| result.constraint(branch))
    })
    .or_else(|| {
        if comparison_domain(db, left, right, ComparisonOperator::Equality)
            == ComparisonDomain::Known
        {
            ComparisonEvaluator::new(db, soundness_policy)
                .evaluate(left, right, branch, ComparisonOperator::Equality)
                .constraint(branch)
        } else {
            None
        }
    })
}

/// Return a constraint excluding every value known to compare equal to `ty`.
pub(super) fn equality_exclusion_constraint<'db>(
    db: &'db dyn Db,
    ty: Type<'db>,
) -> Option<Type<'db>> {
    let ty = ty.resolve_type_alias(db);
    builtin_literal_constraint(db, ty, ty, ComparisonOperator::Equality, false)
        .or_else(|| ty.is_single_valued(db).then(|| ty.negate(db)))
        .or_else(|| {
            (ComparisonEvaluator::conservative(db).evaluate(
                ty,
                ty,
                ComparisonBranch::Positive,
                ComparisonOperator::Equality,
            ) == ComparisonResult::AlwaysTrue)
                .then(|| ty.negate(db))
        })
}

/// Return a constraint for `left` in a branch where `left != right` has the given truthiness.
///
/// Returns `None` when the comparison behavior of either operand is not precise enough to safely
/// constrain `left`.
///
/// For example, comparing a literal union against one of its members constrains both branches:
///
/// ```python
/// from typing import Literal
///
/// def f(x: Literal[1, 2]):
///     if x != 1:
///         reveal_type(x)  # Literal[2]
///     else:
///         reveal_type(x)  # Literal[1]
///
/// def g(x: Literal[1]):
///     if x != 1:
///         reveal_type(x)  # Never
/// ```
pub(super) fn evaluate_type_inequality<'db>(
    db: &'db dyn Db,
    left: Type<'db>,
    right: Type<'db>,
    is_positive: bool,
    soundness_policy: ComparisonSoundnessPolicy,
) -> Option<Type<'db>> {
    let branch = ComparisonBranch::from(is_positive);
    let condition_expects_equality =
        ComparisonOperator::Inequality.condition_expects_equality(branch);
    enum_literal_constraint(
        db,
        left,
        right,
        ComparisonOperator::Inequality,
        condition_expects_equality,
    )
    .or_else(|| {
        builtin_literal_constraint(
            db,
            left,
            right,
            ComparisonOperator::Inequality,
            condition_expects_equality,
        )
    })
    .or_else(|| {
        ComparisonEvaluator::new(db, soundness_policy)
            .evaluate(left, right, branch, ComparisonOperator::Inequality)
            .constraint(branch)
    })
}

/// Return the truthiness of `left == right` when it is known for every represented runtime value.
///
/// A result that only permits narrowing remains ambiguous because it can still evaluate either way.
pub(crate) fn equality_truthiness<'db>(
    db: &'db dyn Db,
    left: Type<'db>,
    right: Type<'db>,
) -> Truthiness {
    comparison_truthiness(db, left, right, ComparisonOperator::Equality)
}

/// Return the truthiness of `left != right` when it is known for every represented runtime value.
///
/// A result that only permits narrowing remains ambiguous because it can still evaluate either way.
pub(super) fn inequality_truthiness<'db>(
    db: &'db dyn Db,
    left: Type<'db>,
    right: Type<'db>,
) -> Truthiness {
    comparison_truthiness(db, left, right, ComparisonOperator::Inequality)
}

fn comparison_truthiness<'db>(
    db: &'db dyn Db,
    left: Type<'db>,
    right: Type<'db>,
    operator: ComparisonOperator,
) -> Truthiness {
    match ComparisonEvaluator::for_truthiness(db).evaluate(
        left,
        right,
        ComparisonBranch::Positive,
        operator,
    ) {
        ComparisonResult::AlwaysTrue => Truthiness::AlwaysTrue,
        ComparisonResult::AlwaysFalse => Truthiness::AlwaysFalse,
        ComparisonResult::CanNarrow(_) | ComparisonResult::Ambiguous => Truthiness::Ambiguous,
    }
}

/// Selects how recursive comparison results are combined.
///
/// The goal is only an optimization; both modes use the same comparison semantics and agree on
/// which results are definite. [`Constraint`](Self::Constraint) preserves branch-specific narrowing
/// for the left operand. [`Truthiness`](Self::Truthiness) can discard those constraints because its
/// caller only needs to know whether every expanded alternative agrees, and can stop as soon as the
/// comparison cannot be definite.
///
/// For example, truthiness evaluation proves that this comparison is always false by checking the
/// finite alternatives on both sides, without constructing a narrowing constraint:
///
/// ```python
/// from enum import Enum
/// from typing import Literal
///
/// class Choice(Enum):
///     A = 1
///     B = 2
///     C = 3
///     D = 4
///
/// def compare(left: Literal[Choice.A, Choice.B], right: Literal[Choice.C, Choice.D]):
///     reveal_type(left == right)  # Literal[False]
/// ```
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
enum ComparisonGoal {
    Constraint,
    Truthiness,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub(super) enum ComparisonSoundnessPolicy {
    Conservative,
    UnsafeLiteralNarrowing,
}

impl ComparisonSoundnessPolicy {
    pub(super) fn from_strict_literal_narrowing(enabled: bool) -> Self {
        if enabled {
            Self::Conservative
        } else {
            Self::UnsafeLiteralNarrowing
        }
    }
}

/// Identifies an active comparison evaluation.
///
/// Operand order and branch are significant because the left operand is the narrowing target.
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
struct ComparisonKey<'db> {
    left: Type<'db>,
    right: Type<'db>,
    branch: ComparisonBranch,
    operator: ComparisonOperator,
}

/// Tracks comparisons that are already in progress so recursive evaluation terminates.
struct ComparisonEvaluator<'db> {
    db: &'db dyn Db,
    active: FxHashSet<ComparisonKey<'db>>,
    goal: ComparisonGoal,
    soundness_policy: ComparisonSoundnessPolicy,
}

impl<'db> ComparisonEvaluator<'db> {
    fn new(db: &'db dyn Db, soundness_policy: ComparisonSoundnessPolicy) -> Self {
        Self {
            db,
            active: FxHashSet::default(),
            goal: ComparisonGoal::Constraint,
            soundness_policy,
        }
    }

    fn conservative(db: &'db dyn Db) -> Self {
        Self::new(db, ComparisonSoundnessPolicy::Conservative)
    }

    fn for_truthiness(db: &'db dyn Db) -> Self {
        Self {
            db,
            active: FxHashSet::default(),
            goal: ComparisonGoal::Truthiness,
            soundness_policy: ComparisonSoundnessPolicy::Conservative,
        }
    }

    /// Evaluate a comparison recursively, treating `left` as the operand being constrained.
    ///
    /// For example, proving that every constraint of `EQUAL_VALUES` compares equal recursively
    /// evaluates the constrained type variable against itself:
    ///
    /// ```python
    /// from typing import Any, Literal, TypeVar
    ///
    /// EQUAL_VALUES = TypeVar("EQUAL_VALUES", Literal[0], Literal[False])
    ///
    /// def f(x: Any, y: EQUAL_VALUES):
    ///     if x != y:
    ///         reveal_type(x)  # Any & ~EQUAL_VALUES
    /// ```
    ///
    /// In [`ComparisonGoal::Constraint`] mode, `branch` selects the branch whose constraint is
    /// accumulated when either operand expands into multiple alternatives. In
    /// [`ComparisonGoal::Truthiness`] mode, expansion instead requires every alternative to agree
    /// on the comparison result. Re-entering an active comparison conservatively returns an
    /// ambiguous result instead of recursing indefinitely.
    fn evaluate(
        &mut self,
        left: Type<'db>,
        right: Type<'db>,
        branch: ComparisonBranch,
        operator: ComparisonOperator,
    ) -> ComparisonResult<'db> {
        let left = left.resolve_type_alias(self.db);
        let right = right.resolve_type_alias(self.db);
        let key = ComparisonKey {
            left,
            right,
            branch,
            operator,
        };

        // A repeated key means that the result depends on itself. Treating it as ambiguous is
        // conservative: callers only narrow from definite truthiness or an explicit constraint.
        if !self.active.insert(key) {
            return ComparisonResult::Ambiguous;
        }

        let result = evaluate_comparison_once(self, left, right, branch, operator);
        self.active.remove(&key);
        result
    }
}

/// Evaluate a comparison whose aliases are resolved and whose key is registered as active.
///
/// Recursive comparisons must use [`ComparisonEvaluator::evaluate`] so cycles are detected.
fn evaluate_comparison_once<'db>(
    evaluator: &mut ComparisonEvaluator<'db>,
    left: Type<'db>,
    right: Type<'db>,
    branch: ComparisonBranch,
    operator: ComparisonOperator,
) -> ComparisonResult<'db> {
    let db = evaluator.db;

    if let Some(result) = evaluate_enum_domains(db, left, right, branch, operator) {
        return result;
    }

    if let Some(alternatives) = finite_alternatives(db, left, operator) {
        return evaluate_union_left(evaluator, &alternatives, right, branch, operator);
    }
    if let Some(alternatives) = finite_alternatives(db, right, operator) {
        return evaluate_union_right(evaluator, left, &alternatives, branch, operator);
    }

    match (left, right) {
        (
            Type::Never
            | Type::Divergent(_)
            | Type::AlwaysFalsy
            | Type::AlwaysTruthy
            | Type::ProtocolInstance(_)
            | Type::DataclassTransformer(_)
            | Type::TypeGuard(_)
            | Type::TypeIs(_),
            _,
        )
        | (
            _,
            Type::Never
            | Type::Divergent(_)
            | Type::AlwaysFalsy
            | Type::AlwaysTruthy
            | Type::ProtocolInstance(_)
            | Type::DataclassTransformer(_)
            | Type::TypeGuard(_)
            | Type::TypeIs(_),
        ) => ComparisonResult::Ambiguous,

        (Type::Dynamic(_), other) => {
            if !operator.condition_expects_equality(branch)
                && all_values_compare_equal(evaluator, other, operator)
            {
                ComparisonResult::CanNarrow(
                    IntersectionBuilder::new(db)
                        .add_positive(left)
                        .add_negative(other)
                        .build(),
                )
            } else {
                ComparisonResult::Ambiguous
            }
        }
        (_, Type::Dynamic(_)) => ComparisonResult::Ambiguous,

        (Type::TypeVar(var), other) => match var.typevar(db).bound_or_constraints(db) {
            None => ComparisonResult::Ambiguous,
            Some(TypeVarBoundOrConstraints::UpperBound(_)) => {
                if !operator.condition_expects_equality(branch)
                    && all_values_compare_equal(evaluator, other, operator)
                {
                    ComparisonResult::CanNarrow(other.negate(db))
                } else {
                    ComparisonResult::Ambiguous
                }
            }
            Some(TypeVarBoundOrConstraints::Constraints(constraints)) => {
                evaluator.evaluate(constraints.as_type(db), other, branch, operator)
            }
        },
        (other, Type::TypeVar(var)) => match var.typevar(db).bound_or_constraints(db) {
            Some(TypeVarBoundOrConstraints::Constraints(constraints)) => {
                evaluator.evaluate(other, constraints.as_type(db), branch, operator)
            }
            None | Some(TypeVarBoundOrConstraints::UpperBound(_)) => ComparisonResult::Ambiguous,
        },

        (Type::NewTypeInstance(newtype), other) => evaluator
            .evaluate(newtype.concrete_base_type(db), other, branch, operator)
            .discard_narrowing(),
        (other, Type::NewTypeInstance(newtype)) => evaluator
            .evaluate(other, newtype.concrete_base_type(db), branch, operator)
            .discard_narrowing(),

        (Type::Union(union), other) => {
            evaluate_union_left(evaluator, union.elements(db), other, branch, operator)
        }
        (other, Type::Union(union)) => {
            evaluate_union_right(evaluator, other, union.elements(db), branch, operator)
        }
        (Type::Intersection(intersection), other) => evaluate_intersection_left(
            evaluator,
            Type::Intersection(intersection),
            intersection.positive(db),
            other,
            branch,
            operator,
        ),

        (Type::LiteralValue(left_literal), Type::LiteralValue(right_literal)) => {
            match known_literal_equality(db, left_literal.kind(), right_literal.kind(), operator) {
                Some(equal) => operator.result_from_equality(equal),
                None => narrow_literal_comparison(
                    db,
                    left,
                    right,
                    left_literal.kind(),
                    right_literal.kind(),
                    operator.condition_expects_equality(branch),
                ),
            }
        }

        (Type::LiteralValue(literal), other) => compare_literal_to_other(
            evaluator,
            Type::LiteralValue(literal),
            literal.kind(),
            other,
            branch,
            operator,
            LiteralOperand::Target,
        ),
        (other, Type::LiteralValue(literal)) => compare_literal_to_other(
            evaluator,
            Type::LiteralValue(literal),
            literal.kind(),
            other,
            branch,
            operator,
            LiteralOperand::Other,
        ),

        (Type::TypedDict(_), Type::TypedDict(_)) => ComparisonResult::Ambiguous,
        (Type::TypedDict(_), other) | (other, Type::TypedDict(_)) => {
            match KnownComparisonSemantics::of_type(db, other, operator) {
                Some(KnownComparisonSemantics::Dict) | None => ComparisonResult::Ambiguous,
                Some(_) => operator.result_from_equality(false),
            }
        }

        (Type::ModuleLiteral(left_module), Type::ModuleLiteral(right_module)) => {
            operator.result_from_equality(left_module.module(db) == right_module.module(db))
        }
        (Type::GenericAlias(left_alias), Type::GenericAlias(right_alias))
            if left_alias == right_alias =>
        {
            operator.result_from_equality(true)
        }
        (Type::WrapperDescriptor(left_descriptor), Type::WrapperDescriptor(right_descriptor))
            if left_descriptor == right_descriptor =>
        {
            operator.result_from_equality(true)
        }
        (
            Type::KnownBoundMethod(KnownBoundMethodType::FunctionTypeDunderGet(left_function)),
            Type::KnownBoundMethod(KnownBoundMethodType::FunctionTypeDunderGet(right_function)),
        )
        | (
            Type::KnownBoundMethod(KnownBoundMethodType::FunctionTypeDunderCall(left_function)),
            Type::KnownBoundMethod(KnownBoundMethodType::FunctionTypeDunderCall(right_function)),
        ) if left_function == right_function => operator.result_from_equality(true),
        (Type::KnownInstance(left_instance), Type::KnownInstance(right_instance))
            if left_instance == right_instance
                && left.is_single_valued(db)
                && operator == ComparisonOperator::Equality =>
        {
            ComparisonResult::AlwaysTrue
        }
        (left, right)
            if has_known_identity_comparison_semantics(db, left, operator)
                && has_known_identity_comparison_semantics(db, right, operator) =>
        {
            operator.result_from_equality(left == right)
        }

        (Type::NominalInstance(left_instance), Type::NominalInstance(right_instance)) => {
            compare_nominal_instances(db, left_instance, right_instance, operator)
        }

        _ => ComparisonResult::Ambiguous,
    }
}

/// Return whether every value represented by `ty` is known to compare equal to every other value.
///
/// Comparison evaluation is reused so this stays aligned with all modeled equality semantics.
/// Cyclic self-comparisons recover as ambiguous, so only a definite acyclic proof returns true.
fn all_values_compare_equal<'db>(
    evaluator: &mut ComparisonEvaluator<'db>,
    ty: Type<'db>,
    operator: ComparisonOperator,
) -> bool {
    evaluator.evaluate(ty, ty, ComparisonBranch::Positive, operator)
        == operator.result_from_equality(true)
}

/// Return whether `ty` is handled by [`builtin_literal_constraint`].
///
/// This includes `int`, `bool`, `str`, and `bytes` literals, along with `bool` itself because its
/// only possible values are `Literal[True]` and `Literal[False]`.
fn is_builtin_literal_type(db: &dyn Db, ty: Type) -> bool {
    match ty.resolve_type_alias(db) {
        Type::LiteralValue(literal) => matches!(
            literal.kind(),
            LiteralValueTypeKind::Int(_)
                | LiteralValueTypeKind::Bool(_)
                | LiteralValueTypeKind::String(_)
                | LiteralValueTypeKind::Bytes(_)
        ),
        Type::NominalInstance(instance) => instance.has_known_class(db, KnownClass::Bool),
        _ => false,
    }
}

/// Return a constraint for comparison with an `int`, `bool`, `str`, or `bytes` literal.
///
/// For example:
///
/// ```py
/// x = "B"
/// if random():
///     x = "C"
/// if x != "C":
///     while random():
///         reveal_type(x)  # Literal["B", "D"]
///         x = "D"
/// ```
///
/// At first, `x != "C"` narrows `x` from `"B" | "C"` to `"B"`. The loop later adds `"D"`. If we
/// record the result as just `"B"`, the type of `x` can never grow to include `"D"`. Recording it as
/// "anything except `"C"`" (`~Literal["C"]`) rules out `"C"` but still allows the loop to add `"D"`.
///
/// The constraint also follows Python's equality between booleans and integers: `x != 0` excludes
/// both `Literal[0]` and `Literal[False]`, while `x != 1` excludes `Literal[1]` and `Literal[True]`.
fn builtin_literal_constraint<'db>(
    db: &'db dyn Db,
    left: Type<'db>,
    right: Type<'db>,
    operator: ComparisonOperator,
    condition_expects_equality: bool,
) -> Option<Type<'db>> {
    let Type::LiteralValue(right) = right.resolve_type_alias(db) else {
        return None;
    };

    let equal_to_right = builtin_literals_equal_to(db, Type::LiteralValue(right), right.kind())?;

    if !condition_expects_equality {
        let equal_to_right = add_equal_enum_literals(
            db,
            left,
            right.kind(),
            operator,
            UnionBuilder::new(db).add(equal_to_right),
        );
        return Some(equal_to_right.build().negate(db));
    }

    match left.resolve_type_alias(db) {
        Type::Union(union) => union
            .elements(db)
            .iter()
            .copied()
            .all(|element| is_builtin_literal_type(db, element)),
        left => is_builtin_literal_type(db, left),
    }
    .then_some(equal_to_right)
}

/// Return the builtin literal values that compare equal to `literal_type`.
fn builtin_literals_equal_to<'db>(
    db: &'db dyn Db,
    literal_type: Type<'db>,
    literal: LiteralValueTypeKind<'db>,
) -> Option<Type<'db>> {
    let builder = match literal {
        LiteralValueTypeKind::Int(value) => {
            let mut builder = UnionBuilder::new(db).add(literal_type);
            if matches!(value.as_i64(), 0 | 1) {
                builder = builder.add(Type::bool_literal(value.as_i64() == 1));
            }
            builder
        }
        LiteralValueTypeKind::Bool(value) => UnionBuilder::new(db)
            .add(literal_type)
            .add(Type::int_literal(i64::from(value))),
        LiteralValueTypeKind::String(_) | LiteralValueTypeKind::Bytes(_) => {
            UnionBuilder::new(db).add(literal_type)
        }
        LiteralValueTypeKind::LiteralString | LiteralValueTypeKind::Enum(_) => return None,
    };
    Some(builder.build())
}

/// Add finite enum members in `ty` that are known to compare equal to `right`.
fn add_equal_enum_literals<'db>(
    db: &'db dyn Db,
    ty: Type<'db>,
    right: LiteralValueTypeKind<'db>,
    operator: ComparisonOperator,
    mut builder: UnionBuilder<'db>,
) -> UnionBuilder<'db> {
    match ty.resolve_type_alias(db) {
        Type::Union(union) => {
            for element in union.elements(db) {
                builder = add_equal_enum_literals(db, *element, right, operator, builder);
            }
        }
        Type::LiteralValue(literal) => {
            if matches!(literal.kind(), LiteralValueTypeKind::Enum(_))
                && known_literal_equality(db, literal.kind(), right, operator) == Some(true)
            {
                builder = builder.add(Type::LiteralValue(literal));
            }
        }
        ty => {
            if let Some(alternatives) = finite_alternatives(db, ty, operator) {
                for alternative in alternatives {
                    builder = add_equal_enum_literals(db, alternative, right, operator, builder);
                }
            }
        }
    }
    builder
}

/// Return a constraint when every possible value of `left` is a member of the same enum as `right`.
///
/// For example:
///
/// ```python
/// from enum import Enum
///
/// class Answer(Enum):
///     NO = 0
///     YES = 1
///
/// def f(answer: Answer):
///     if answer != Answer.NO:
///         reveal_type(answer)  # Literal[Answer.YES]
///     else:
///         reveal_type(answer)  # Literal[Answer.NO]
/// ```
///
/// This shortcut is disabled if the enum defines or inherits custom `__eq__` or `__ne__` methods,
/// because those methods can change whether two members compare equal.
fn enum_literal_constraint<'db>(
    db: &'db dyn Db,
    left: Type<'db>,
    right: Type<'db>,
    operator: ComparisonOperator,
    condition_expects_equality: bool,
) -> Option<Type<'db>> {
    let Type::LiteralValue(right_literal) = right.resolve_type_alias(db) else {
        return None;
    };
    let LiteralValueTypeKind::Enum(right) = right_literal.kind() else {
        return None;
    };
    if !is_same_enum_domain(db, left, right)
        || KnownComparisonSemantics::of_instance(db, right.enum_class_instance(db), operator)
            .is_none()
    {
        return None;
    }

    let enum_class_literal = right.enum_class_literal(db);
    let name = enum_class_literal
        .resolve_member(db, right.name(db))?
        .clone();
    let equal_to_right = Type::from(LiteralValueType::new(
        EnumLiteralType::new(db, enum_class_literal, name),
        right_literal.is_promotable(),
    ));
    Some(equal_to_right.negate_if(db, !condition_expects_equality))
}

/// Return whether every possible value of `ty` belongs to the same enum as `right`.
pub(super) fn is_same_enum_domain<'db>(
    db: &'db dyn Db,
    ty: Type<'db>,
    right: EnumLiteralType<'db>,
) -> bool {
    match ty.resolve_type_alias(db) {
        Type::LiteralValue(literal) => matches!(
            literal.kind(),
            LiteralValueTypeKind::Enum(left)
                if left.enum_class(db) == right.enum_class(db)
        ),
        Type::Union(union) => union
            .elements(db)
            .iter()
            .all(|element| is_same_enum_domain(db, *element, right)),
        Type::NominalInstance(instance) => instance.class_literal(db) == right.enum_class(db),
        Type::EnumComplement(complement) => complement.enum_class(db) == right.enum_class(db),
        Type::Intersection(intersection) => intersection
            .enum_complement(db)
            .is_some_and(|complement| complement.enum_class(db) == right.enum_class(db)),
        _ => false,
    }
}

/// Evaluate each alternative of the union being constrained and combine their branch results.
fn evaluate_union_left<'db>(
    evaluator: &mut ComparisonEvaluator<'db>,
    elements: &[Type<'db>],
    other: Type<'db>,
    branch: ComparisonBranch,
    operator: ComparisonOperator,
) -> ComparisonResult<'db> {
    if evaluator.goal == ComparisonGoal::Truthiness {
        return combine_definite_truthiness(
            elements
                .iter()
                .map(|element| evaluator.evaluate(*element, other, branch, operator)),
        );
    }

    let db = evaluator.db;
    evaluate_target_union(db, elements, branch, |element| {
        evaluator.evaluate(element, other, branch, operator)
    })
}

/// Combine comparison results for the alternatives of the union being constrained.
///
/// Alternatives that cannot satisfy the selected branch are removed. Dynamic alternatives retain
/// negative constraints for removed arms so that the result still describes the branch predicate.
fn evaluate_target_union<'db>(
    db: &'db dyn Db,
    elements: &[Type<'db>],
    branch: ComparisonBranch,
    mut evaluate: impl FnMut(Type<'db>) -> ComparisonResult<'db>,
) -> ComparisonResult<'db> {
    if elements.is_empty() {
        return ComparisonResult::Ambiguous;
    }

    let mut all_true = true;
    let mut all_false = true;
    let mut narrowed = Vec::with_capacity(elements.len());
    let mut removed = UnionBuilder::new(db);
    let mut removed_any = false;

    for element in elements {
        match evaluate(*element) {
            ComparisonResult::AlwaysTrue => {
                all_false = false;
                if branch == ComparisonBranch::Positive {
                    narrowed.push(Some(*element));
                } else {
                    narrowed.push(None);
                    removed = removed.add(*element);
                    removed_any = true;
                }
            }
            ComparisonResult::AlwaysFalse => {
                all_true = false;
                if branch == ComparisonBranch::Positive {
                    narrowed.push(None);
                    removed = removed.add(*element);
                    removed_any = true;
                } else {
                    narrowed.push(Some(*element));
                }
            }
            ComparisonResult::CanNarrow(narrowed_element) => {
                all_true = false;
                all_false = false;
                narrowed.push(Some(narrowed_element));
            }
            ComparisonResult::Ambiguous => {
                all_true = false;
                all_false = false;
                narrowed.push(Some(*element));
            }
        }
    }

    if all_true {
        return ComparisonResult::AlwaysTrue;
    }
    if all_false {
        return ComparisonResult::AlwaysFalse;
    }

    let removed = removed_any.then(|| removed.build());
    let mut builder = UnionBuilder::new(db);
    for narrowed in narrowed {
        let Some(mut narrowed) = narrowed else {
            continue;
        };
        if let Some(removed) = removed {
            narrowed = IntersectionBuilder::new(db)
                .add_positive(narrowed)
                .add_negative(removed)
                .build();
        }
        builder = builder.add(narrowed);
    }
    ComparisonResult::CanNarrow(builder.build())
}

/// Evaluate the target against each alternative of a union on the non-target side.
fn evaluate_union_right<'db>(
    evaluator: &mut ComparisonEvaluator<'db>,
    left: Type<'db>,
    elements: &[Type<'db>],
    branch: ComparisonBranch,
    operator: ComparisonOperator,
) -> ComparisonResult<'db> {
    if evaluator.goal == ComparisonGoal::Truthiness {
        return combine_definite_truthiness(
            elements
                .iter()
                .map(|element| evaluator.evaluate(left, *element, branch, operator)),
        );
    }

    let db = evaluator.db;
    evaluate_against_results(
        db,
        left,
        branch,
        elements
            .iter()
            .map(|element| evaluator.evaluate(left, *element, branch, operator)),
    )
}

/// Combine results when the caller only needs definite truthiness.
///
/// Any ambiguous or narrowing result, or any disagreement between definite results, makes the
/// aggregate ambiguous. In each case, later alternatives cannot make it definite again.
fn combine_definite_truthiness<'db>(
    results: impl IntoIterator<Item = ComparisonResult<'db>>,
) -> ComparisonResult<'db> {
    let mut definite = None;

    for result in results {
        let current = match result {
            ComparisonResult::AlwaysTrue => true,
            ComparisonResult::AlwaysFalse => false,
            ComparisonResult::CanNarrow(_) | ComparisonResult::Ambiguous => {
                return ComparisonResult::Ambiguous;
            }
        };

        match definite {
            Some(previous) if previous != current => return ComparisonResult::Ambiguous,
            Some(_) => {}
            None => definite = Some(current),
        }
    }

    definite.map_or(ComparisonResult::Ambiguous, ComparisonResult::from_bool)
}

/// Combine comparison results produced by alternatives of the non-target operand.
///
/// The target remains possible when any alternative can satisfy the selected branch; definite
/// truthiness is reported only when every alternative agrees.
fn evaluate_against_results<'db>(
    db: &'db dyn Db,
    target: Type<'db>,
    branch: ComparisonBranch,
    results: impl IntoIterator<Item = ComparisonResult<'db>>,
) -> ComparisonResult<'db> {
    let mut all_true = true;
    let mut all_false = true;
    let mut builder = UnionBuilder::new(db);
    let mut any = false;

    for result in results {
        any = true;
        match result {
            ComparisonResult::AlwaysTrue => {
                all_false = false;
                if branch == ComparisonBranch::Positive {
                    builder = builder.add(target);
                }
            }
            ComparisonResult::AlwaysFalse => {
                all_true = false;
                if branch == ComparisonBranch::Negative {
                    builder = builder.add(target);
                }
            }
            ComparisonResult::CanNarrow(narrowed) => {
                all_true = false;
                all_false = false;
                builder = builder.add(narrowed);
            }
            ComparisonResult::Ambiguous => {
                all_true = false;
                all_false = false;
                builder = builder.add(target);
            }
        }
    }

    if !any {
        ComparisonResult::Ambiguous
    } else if all_true {
        ComparisonResult::AlwaysTrue
    } else if all_false {
        ComparisonResult::AlwaysFalse
    } else {
        ComparisonResult::CanNarrow(builder.build())
    }
}

/// Combine compatible comparison results from the positive elements of an intersection target.
fn evaluate_intersection_left<'db>(
    evaluator: &mut ComparisonEvaluator<'db>,
    original: Type<'db>,
    positive: &crate::FxOrderSet<Type<'db>>,
    other: Type<'db>,
    branch: ComparisonBranch,
    operator: ComparisonOperator,
) -> ComparisonResult<'db> {
    if evaluator.goal == ComparisonGoal::Truthiness {
        return combine_definite_truthiness(
            positive
                .iter()
                .map(|element| evaluator.evaluate(*element, other, branch, operator)),
        );
    }

    let db = evaluator.db;
    let mut any_true = false;
    let mut any_false = false;
    let mut any_ambiguous = false;
    let mut any_narrowing = false;
    let mut builder = IntersectionBuilder::new(db).add_positive(original);

    for element in positive {
        match evaluator.evaluate(*element, other, branch, operator) {
            ComparisonResult::AlwaysTrue => any_true = true,
            ComparisonResult::AlwaysFalse => any_false = true,
            ComparisonResult::CanNarrow(narrowed) => {
                any_narrowing = true;
                builder = builder.add_positive(narrowed);
            }
            ComparisonResult::Ambiguous => any_ambiguous = true,
        }
    }

    if any_ambiguous || (any_narrowing && (any_true || any_false)) {
        return ComparisonResult::Ambiguous;
    }

    match (any_true, any_false) {
        (true, false) => ComparisonResult::AlwaysTrue,
        (false, true) => ComparisonResult::AlwaysFalse,
        (true, true) => ComparisonResult::Ambiguous,
        (false, false) => ComparisonResult::CanNarrow(builder.build()),
    }
}

/// Expand a type into its finite runtime alternatives when its comparison semantics are known.
///
/// Enum classes with custom comparison methods are deliberately not expanded because their members
/// may compare equal to values outside the enum domain.
fn finite_alternatives<'db>(
    db: &'db dyn Db,
    ty: Type<'db>,
    operator: ComparisonOperator,
) -> Option<Vec<Type<'db>>> {
    match ty {
        Type::EnumComplement(complement) => KnownComparisonSemantics::of_type(db, ty, operator)
            .is_some()
            .then(|| complement.remaining_literal_types(db)),
        Type::Intersection(intersection) => {
            let complement = intersection.enum_complement(db)?;
            KnownComparisonSemantics::of_type(db, ty, operator)
                .is_some()
                .then(|| complement.remaining_literal_types(db))
        }
        Type::NominalInstance(instance) if instance.has_known_class(db, KnownClass::Bool) => {
            Some(vec![Type::bool_literal(true), Type::bool_literal(false)])
        }
        Type::NominalInstance(instance)
            if KnownComparisonSemantics::of_type(db, ty, operator).is_some() =>
        {
            enum_member_literals(db, instance.class_literal(db), None).map(Iterator::collect)
        }
        _ => None,
    }
}

/// Return a constraint for literal pairs whose equality cannot be decided statically.
///
/// This primarily handles `LiteralString`, which can be constrained by a concrete string literal
/// or a string-valued enum member without having a single statically known runtime value.
fn narrow_literal_comparison<'db>(
    db: &'db dyn Db,
    left: Type<'db>,
    right: Type<'db>,
    left_literal: LiteralValueTypeKind<'db>,
    right_literal: LiteralValueTypeKind<'db>,
    equality_is_positive: bool,
) -> ComparisonResult<'db> {
    match (left_literal, right_literal) {
        (LiteralValueTypeKind::LiteralString, LiteralValueTypeKind::String(_)) => {
            ComparisonResult::CanNarrow(right.negate_if(db, !equality_is_positive))
        }
        (LiteralValueTypeKind::String(_), LiteralValueTypeKind::LiteralString) => {
            ComparisonResult::CanNarrow(left.negate_if(db, !equality_is_positive))
        }
        (LiteralValueTypeKind::LiteralString, LiteralValueTypeKind::Enum(enum_literal)) => {
            narrow_literal_string_against_enum(db, enum_literal, equality_is_positive)
        }
        (LiteralValueTypeKind::Enum(enum_literal), LiteralValueTypeKind::LiteralString) => {
            narrow_literal_string_against_enum(db, enum_literal, equality_is_positive)
        }
        _ => ComparisonResult::Ambiguous,
    }
}

/// Narrow `LiteralString` against a string-valued enum member with inherited `str` semantics.
fn narrow_literal_string_against_enum<'db>(
    db: &'db dyn Db,
    enum_literal: EnumLiteralType<'db>,
    equality_is_positive: bool,
) -> ComparisonResult<'db> {
    if KnownComparisonSemantics::of_type(
        db,
        Type::enum_literal(enum_literal),
        ComparisonOperator::Equality,
    ) != Some(KnownComparisonSemantics::Str)
    {
        return ComparisonResult::Ambiguous;
    }
    let Some(value @ Type::LiteralValue(_)) = enum_literal_value(db, enum_literal) else {
        return ComparisonResult::Ambiguous;
    };
    let Some(LiteralValueTypeKind::String(_)) = value.as_literal_value_kind() else {
        return ComparisonResult::Ambiguous;
    };
    let narrowed = UnionBuilder::new(db)
        .add(value)
        .add(Type::enum_literal(enum_literal))
        .build()
        .negate_if(db, !equality_is_positive);
    ComparisonResult::CanNarrow(narrowed)
}

/// Return the builtin comparison semantics assumed by unsafe literal narrowing.
fn unsafe_narrowable_builtin_semantics(db: &dyn Db, ty: Type) -> Option<KnownComparisonSemantics> {
    let Type::NominalInstance(instance) = ty.resolve_type_alias(db) else {
        return None;
    };

    if instance.has_known_class(db, KnownClass::Int) {
        Some(KnownComparisonSemantics::Int)
    } else if instance.has_known_class(db, KnownClass::Str) {
        Some(KnownComparisonSemantics::Str)
    } else if instance.has_known_class(db, KnownClass::Bytes) {
        Some(KnownComparisonSemantics::Bytes)
    } else {
        None
    }
}

/// Compare a literal with a non-literal type using their known runtime comparison semantics.
///
/// A literal on the non-target side can constrain the target only when the types overlap; matching
/// comparison implementations alone do not establish that the literal inhabits the target type.
fn compare_literal_to_other<'db>(
    evaluator: &ComparisonEvaluator<'db>,
    literal_type: Type<'db>,
    literal: LiteralValueTypeKind<'db>,
    other: Type<'db>,
    branch: ComparisonBranch,
    operator: ComparisonOperator,
    literal_operand: LiteralOperand,
) -> ComparisonResult<'db> {
    let db = evaluator.db;

    if matches!(literal, LiteralValueTypeKind::LiteralString) {
        return match KnownComparisonSemantics::of_type(db, other, operator) {
            Some(KnownComparisonSemantics::Str) => ComparisonResult::Ambiguous,
            Some(_) => ComparisonResult::from_bool(operator == ComparisonOperator::Inequality),
            None => ComparisonResult::Ambiguous,
        };
    }

    let Some(literal_semantics) = KnownComparisonSemantics::of_literal(db, literal, operator)
    else {
        return ComparisonResult::Ambiguous;
    };
    let condition_expects_equality = operator.condition_expects_equality(branch);

    // Treat broad builtin types as if they exclude subclasses with custom equality. This is
    // intentionally unsafe: an instance of such a subclass can compare equal to the literal
    // without inhabiting its literal type. Explicitly typed subclasses do not take this path.
    if evaluator.soundness_policy == ComparisonSoundnessPolicy::UnsafeLiteralNarrowing
        && condition_expects_equality
        && literal_operand == LiteralOperand::Other
        && let Some(equal_to_literal) = builtin_literals_equal_to(db, literal_type, literal)
        && let Some(other_semantics) = unsafe_narrowable_builtin_semantics(db, other)
    {
        return if literal_semantics == other_semantics {
            ComparisonResult::CanNarrow(equal_to_literal)
        } else {
            operator.result_from_equality(false)
        };
    }

    match KnownComparisonSemantics::of_type(db, other, operator) {
        Some(other_semantics) if literal_semantics != other_semantics => {
            ComparisonResult::from_bool(operator == ComparisonOperator::Inequality)
        }
        Some(KnownComparisonSemantics::Object)
            if literal_semantics == KnownComparisonSemantics::Object
                && other.is_disjoint_from(db, literal_type) =>
        {
            ComparisonResult::from_bool(operator == ComparisonOperator::Inequality)
        }
        // Inherited builtin comparison semantics do not imply type overlap. For example, a final
        // `int` subclass can compare equal to `1` despite being disjoint from `Literal[1]`.
        Some(_)
            if literal_operand == LiteralOperand::Other
                && literal_type.is_single_valued(db)
                && !other.is_disjoint_from(db, literal_type) =>
        {
            ComparisonResult::CanNarrow(literal_type.negate_if(db, !condition_expects_equality))
        }
        Some(_) => ComparisonResult::Ambiguous,
        None if literal_operand == LiteralOperand::Other
            && !condition_expects_equality
            && literal_type.is_single_valued(db) =>
        {
            ComparisonResult::CanNarrow(literal_type.negate(db))
        }
        None => ComparisonResult::Ambiguous,
    }
}

/// Compare nominal instances when their inherited comparison implementations are known.
///
/// The result is definite only when the implementations cannot compare equal, or when both types
/// denote the same singleton.
fn compare_nominal_instances<'db>(
    db: &'db dyn Db,
    left_instance: super::NominalInstanceType<'db>,
    right_instance: super::NominalInstanceType<'db>,
    operator: ComparisonOperator,
) -> ComparisonResult<'db> {
    let left = Type::NominalInstance(left_instance);
    let right = Type::NominalInstance(right_instance);
    let Some(left_semantics) = KnownComparisonSemantics::of_type(db, left, operator) else {
        return ComparisonResult::Ambiguous;
    };
    let Some(right_semantics) = KnownComparisonSemantics::of_type(db, right, operator) else {
        return ComparisonResult::Ambiguous;
    };

    let classes_differ = left_instance.class_literal(db) != right_instance.class_literal(db);

    if left_semantics != right_semantics
        || (left_semantics == KnownComparisonSemantics::Object && classes_differ)
    {
        return ComparisonResult::from_bool(operator == ComparisonOperator::Inequality);
    }

    if left == right && left.is_singleton(db) {
        ComparisonResult::from_bool(operator == ComparisonOperator::Equality)
    } else {
        ComparisonResult::Ambiguous
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
enum ComparisonOperator {
    Equality,
    Inequality,
}

impl ComparisonOperator {
    const fn dunder(self) -> &'static str {
        match self {
            ComparisonOperator::Equality => "__eq__",
            ComparisonOperator::Inequality => "__ne__",
        }
    }

    /// Return whether the selected branch requires the operands to compare equal.
    const fn condition_expects_equality(self, branch: ComparisonBranch) -> bool {
        matches!(
            (self, branch),
            (ComparisonOperator::Equality, ComparisonBranch::Positive)
                | (ComparisonOperator::Inequality, ComparisonBranch::Negative)
        )
    }

    fn result_from_equality<'db>(self, equal: bool) -> ComparisonResult<'db> {
        ComparisonResult::from_bool(match self {
            ComparisonOperator::Equality => equal,
            ComparisonOperator::Inequality => !equal,
        })
    }
}

/// A known builtin implementation that determines the runtime behavior of a comparison.
///
/// Two types with different known semantics cannot compare equal. Types with custom or otherwise
/// unknown comparison methods are not assigned a value of this enum.
#[derive(Debug, Copy, Clone, PartialEq, Eq, salsa::Update, get_size2::GetSize)]
enum KnownComparisonSemantics {
    Object,
    Int,
    Str,
    Bytes,
    Tuple,
    Dict,
}

impl KnownComparisonSemantics {
    /// Determine the builtin comparison implementation inherited by `ty`.
    ///
    /// Returns `None` when dunder lookup finds custom or conflicting comparison behavior.
    fn of_type<'db>(db: &'db dyn Db, ty: Type<'db>, operator: ComparisonOperator) -> Option<Self> {
        match ty {
            Type::LiteralValue(literal) => Self::of_literal(db, literal.kind(), operator),
            Type::TypedDict(_) => Some(Self::Dict),
            Type::EnumComplement(complement) => Self::of_instance(
                db,
                complement.enum_class(db).to_non_generic_instance(db),
                operator,
            ),
            Type::Intersection(intersection) => {
                if let Some(complement) = intersection.enum_complement(db) {
                    return Self::of_instance(
                        db,
                        complement.enum_class(db).to_non_generic_instance(db),
                        operator,
                    );
                }
                let mut semantics = intersection
                    .positive(db)
                    .iter()
                    .filter_map(|element| Self::of_type(db, *element, operator));
                let first = semantics.next()?;
                semantics
                    .all(|semantics| semantics == first)
                    .then_some(first)
            }
            Type::NominalInstance(instance)
                if instance.class(db).is_final(db) || instance.tuple_spec(db).is_some() =>
            {
                Self::of_instance(db, ty, operator)
            }
            _ => None,
        }
    }

    /// Return the builtin comparison implementation used by a literal value.
    fn of_literal<'db>(
        db: &'db dyn Db,
        literal: LiteralValueTypeKind<'db>,
        operator: ComparisonOperator,
    ) -> Option<Self> {
        match literal {
            LiteralValueTypeKind::Int(_) | LiteralValueTypeKind::Bool(_) => Some(Self::Int),
            LiteralValueTypeKind::String(_) | LiteralValueTypeKind::LiteralString => {
                Some(Self::Str)
            }
            LiteralValueTypeKind::Bytes(_) => Some(Self::Bytes),
            LiteralValueTypeKind::Enum(enum_literal) => {
                Self::of_instance(db, enum_literal.enum_class_instance(db), operator)
            }
        }
    }

    /// Return the builtin comparison implementation inherited by an instance.
    ///
    /// Returns `None` when lookup finds custom comparison behavior.
    fn of_instance<'db>(
        db: &'db dyn Db,
        instance: Type<'db>,
        operator: ComparisonOperator,
    ) -> Option<Self> {
        let class = instance.to_meta_type(db);
        let dunder = lookup_dunder(db, class, operator.dunder());

        if dunder.place.is_undefined() {
            if operator == ComparisonOperator::Inequality
                && !lookup_dunder(db, class, "__eq__").place.is_undefined()
            {
                return None;
            }
            return Some(Self::Object);
        }

        for (known_class, semantics) in [
            (KnownClass::Int, Self::Int),
            (KnownClass::Str, Self::Str),
            (KnownClass::Bytes, Self::Bytes),
            (KnownClass::Tuple, Self::Tuple),
            (KnownClass::Dict, Self::Dict),
        ] {
            if dunder == lookup_dunder(db, known_class.to_class_literal(db), operator.dunder()) {
                return Some(semantics);
            }
        }
        None
    }
}

/// Whether the non-target operand has a comparison domain that can safely constrain the target.
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
enum ComparisonDomain {
    /// The operand may use comparison behavior that `ty` does not model.
    Unknown,
    /// The operand can be handled by `ty`'s equality-narrowing evaluator.
    Known,
}

/// Classify whether `ty` has comparison behavior that can constrain `target`.
///
/// Unions only have a known domain if every arm does. Broad nominal types require full dunder
/// analysis, which is only useful here when it can eliminate an arm from a union target.
fn comparison_domain<'db>(
    db: &'db dyn Db,
    target: Type<'db>,
    ty: Type<'db>,
    operator: ComparisonOperator,
) -> ComparisonDomain {
    let target = target.resolve_type_alias(db);
    let ty = ty.resolve_type_alias(db);

    match ty {
        Type::Union(union) => {
            if union.elements(db).iter().all(|element| {
                comparison_domain(db, target, *element, operator) == ComparisonDomain::Known
            }) {
                ComparisonDomain::Known
            } else {
                ComparisonDomain::Unknown
            }
        }
        Type::LiteralValue(_) | Type::EnumComplement(_) | Type::TypedDict(_) => {
            ComparisonDomain::Known
        }
        Type::Intersection(intersection) if intersection.enum_complement(db).is_some() => {
            ComparisonDomain::Known
        }
        Type::NominalInstance(instance) => {
            if instance.tuple_spec(db).is_some()
                || ty.is_singleton(db)
                || instance.has_known_class(db, KnownClass::Bool)
                || target.is_union()
                    && KnownComparisonSemantics::of_type(db, ty, operator).is_some()
            {
                ComparisonDomain::Known
            } else {
                ComparisonDomain::Unknown
            }
        }
        _ if ty.is_single_valued(db) => ComparisonDomain::Known,
        _ => ComparisonDomain::Unknown,
    }
}

/// Return whether `ty` is a singleton whose comparison uses object identity semantics.
fn has_known_identity_comparison_semantics<'db>(
    db: &'db dyn Db,
    ty: Type<'db>,
    operator: ComparisonOperator,
) -> bool {
    match ty {
        Type::FunctionLiteral(_) | Type::ModuleLiteral(_) | Type::SpecialForm(_) => true,
        Type::ClassLiteral(class) => {
            KnownComparisonSemantics::of_instance(db, class.metaclass_instance_type(db), operator)
                == Some(KnownComparisonSemantics::Object)
        }
        _ => {
            ty.is_singleton(db)
                && KnownComparisonSemantics::of_type(db, ty, operator)
                    == Some(KnownComparisonSemantics::Object)
        }
    }
}

/// Look up a comparison method without falling back to `object`.
fn lookup_dunder<'db>(
    db: &'db dyn Db,
    ty: Type<'db>,
    name: &'static str,
) -> PlaceAndQualifiers<'db> {
    ty.member_lookup_with_policy(
        db,
        Name::new_static(name),
        MemberLookupPolicy::MRO_NO_OBJECT_FALLBACK,
    )
}

/// Return the comparison result for two literals when their runtime values determine it.
///
/// This accounts for integer/boolean equality and enum aliases or enum values. `None` means custom
/// or insufficiently known comparison behavior prevents a definitive result.
fn known_literal_equality<'db>(
    db: &'db dyn Db,
    left: LiteralValueTypeKind<'db>,
    right: LiteralValueTypeKind<'db>,
    operator: ComparisonOperator,
) -> Option<bool> {
    match (left, right) {
        (LiteralValueTypeKind::Int(left), LiteralValueTypeKind::Int(right)) => {
            Some(left.as_i64() == right.as_i64())
        }
        (LiteralValueTypeKind::Bool(left), LiteralValueTypeKind::Bool(right)) => {
            Some(left == right)
        }
        (LiteralValueTypeKind::Int(left), LiteralValueTypeKind::Bool(right))
        | (LiteralValueTypeKind::Bool(right), LiteralValueTypeKind::Int(left)) => {
            Some(left.as_i64() == i64::from(right))
        }
        (LiteralValueTypeKind::String(left), LiteralValueTypeKind::String(right)) => {
            Some(left.value(db) == right.value(db))
        }
        (LiteralValueTypeKind::Bytes(left), LiteralValueTypeKind::Bytes(right)) => {
            Some(left.value(db) == right.value(db))
        }
        (LiteralValueTypeKind::Enum(left), LiteralValueTypeKind::Enum(right)) => {
            let left_semantics =
                KnownComparisonSemantics::of_instance(db, left.enum_class_instance(db), operator)?;
            let right_semantics =
                KnownComparisonSemantics::of_instance(db, right.enum_class_instance(db), operator)?;
            if left_semantics != right_semantics {
                return Some(false);
            }
            if same_enum_member(db, left, right) {
                return Some(true);
            }
            let enum_class = left.enum_class_literal(db);
            if enum_class == right.enum_class_literal(db) && !enum_class.aliases_are_known(db) {
                return None;
            }
            if left_semantics == KnownComparisonSemantics::Object {
                return Some(false);
            }
            known_literal_equality(
                db,
                enum_literal_value(db, left)?.as_literal_value_kind()?,
                enum_literal_value(db, right)?.as_literal_value_kind()?,
                ComparisonOperator::Equality,
            )
        }
        (LiteralValueTypeKind::Enum(enum_literal), other)
        | (other, LiteralValueTypeKind::Enum(enum_literal)) => {
            let enum_semantics = KnownComparisonSemantics::of_instance(
                db,
                enum_literal.enum_class_instance(db),
                operator,
            )?;
            if enum_semantics != KnownComparisonSemantics::of_literal(db, other, operator)? {
                return Some(false);
            }
            known_literal_equality(
                db,
                enum_literal_value(db, enum_literal)?.as_literal_value_kind()?,
                other,
                ComparisonOperator::Equality,
            )
        }
        (
            LiteralValueTypeKind::LiteralString,
            LiteralValueTypeKind::LiteralString | LiteralValueTypeKind::String(_),
        )
        | (LiteralValueTypeKind::String(_), LiteralValueTypeKind::LiteralString) => None,
        (left, right) => {
            let left_semantics = KnownComparisonSemantics::of_literal(db, left, operator)?;
            let right_semantics = KnownComparisonSemantics::of_literal(db, right, operator)?;
            (left_semantics != right_semantics).then_some(false)
        }
    }
}

/// Return the statically known runtime value of an enum member.
///
/// Custom enum construction can replace the declared value, so members of such enums return `None`.
fn enum_literal_value<'db>(db: &'db dyn Db, literal: EnumLiteralType<'db>) -> Option<Type<'db>> {
    let enum_class_literal = literal.enum_class_literal(db);
    let metadata = enum_metadata(db, enum_class_literal.class_literal(db))?;
    let name = enum_class_literal.resolve_member(db, literal.name(db))?;
    metadata.concrete_value_type(db, name)
}

/// Return whether two enum literals resolve to the same member, including aliases.
fn same_enum_member<'db>(
    db: &'db dyn Db,
    left: EnumLiteralType<'db>,
    right: EnumLiteralType<'db>,
) -> bool {
    let enum_class_literal = left.enum_class_literal(db);
    if enum_class_literal != right.enum_class_literal(db) {
        return false;
    }
    enum_class_literal.resolve_member(db, left.name(db))
        == enum_class_literal.resolve_member(db, right.name(db))
}
