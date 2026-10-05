//! Equality and inequality reasoning for type narrowing and reachability.
//!
//! This module evaluates comparisons with statically known Python semantics, producing branch
//! constraints and definite truthiness while remaining conservative around custom comparison
//! methods.

use rustc_hash::FxHashSet;
use ty_python_core::definition::Definition;

use crate::{AnalysisSettings, Db, ProgramEnvironment, place::PlaceAndQualifiers};

use super::{
    CallArguments, EnumLiteralType, IntersectionBuilder, KnownClass, LiteralValueType,
    LiteralValueTypeKind, MemberLookupPolicy, Truthiness, Type, TypeContext,
    TypeVarBoundOrConstraints, UnionBuilder, bool::BoolError, cyclic::ActiveRecursionDetector,
    enums::enum_metadata,
};

pub(crate) mod enum_source;
mod enums;
pub(crate) mod nominal_source;
pub(crate) mod source;

/// The result of evaluating a runtime comparison between two types.
///
/// Definite truthiness is represented separately from a constraint for the operand currently being
/// narrowed. A comparison can therefore be ambiguous at runtime while still constraining that
/// operand in either branch.
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub(in crate::types) enum ComparisonResult<'db> {
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
pub(in crate::types) enum ComparisonBranch {
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
    env: &ProgramEnvironment<'db>,
    left: Type<'db>,
    right: Type<'db>,
    is_positive: bool,
    soundness_policy: ComparisonSoundnessPolicy,
) -> Option<Type<'db>> {
    evaluate_type_comparison(
        db,
        env,
        left,
        right,
        is_positive,
        ComparisonOperator::Equality,
        soundness_policy,
    )
}

/// Return a constraint excluding every value known to compare equal to `ty`.
pub(super) fn equality_exclusion_constraint<'db>(
    db: &'db dyn Db,
    env: &ProgramEnvironment<'db>,
    ty: Type<'db>,
    soundness_policy: ComparisonSoundnessPolicy,
) -> Option<Type<'db>> {
    let ty = ty.resolve_type_alias(db);
    builtin_literal_constraint(db, env, ty, ty, ComparisonOperator::Equality, false).or_else(|| {
        let mut evaluator = ComparisonEvaluator::new(db, env, soundness_policy);
        all_values_compare_equal(&mut evaluator, ty, ComparisonOperator::Equality)
            .then(|| ty.negate(db, env))
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
    env: &ProgramEnvironment<'db>,
    left: Type<'db>,
    right: Type<'db>,
    is_positive: bool,
    soundness_policy: ComparisonSoundnessPolicy,
) -> Option<Type<'db>> {
    evaluate_type_comparison(
        db,
        env,
        left,
        right,
        is_positive,
        ComparisonOperator::Inequality,
        soundness_policy,
    )
}

/// Return a constraint for `left` in the selected branch of an equality or inequality comparison.
fn evaluate_type_comparison<'db>(
    db: &'db dyn Db,
    env: &ProgramEnvironment<'db>,
    left: Type<'db>,
    right: Type<'db>,
    is_positive: bool,
    operator: ComparisonOperator,
    soundness_policy: ComparisonSoundnessPolicy,
) -> Option<Type<'db>> {
    let right = right.resolve_type_alias(db);
    let branch = ComparisonBranch::from(is_positive);
    let condition_expects_equality = operator.condition_expects_equality(branch);

    // Preserve the shared specialization of a constrained TypeVar. Expanding it before comparing
    // with `left` would lose the correlation with other occurrences in the function.
    if condition_expects_equality
        && let Type::TypeVar(typevar) = right
        && let Some(TypeVarBoundOrConstraints::Constraints(constraints)) =
            typevar.typevar(db).bound_or_constraints(db, env)
        && constraints.elements(db).iter().all(|constraint| {
            evaluate_type_comparison(
                db,
                env,
                left,
                *constraint,
                is_positive,
                operator,
                soundness_policy,
            )
            .is_some_and(|narrowed| {
                equality_truthiness(db, env, narrowed, *constraint, soundness_policy)
                    == Truthiness::AlwaysTrue
                    // Equal values need not have the same type: `False == 0` does not make
                    // `Literal[False]` a valid specialization of a `Literal[0]` constraint.
                    && IntersectionBuilder::new(db, env)
                        .add_positive(left)
                        .add_positive(narrowed)
                        .build()
                        .is_subtype_of(db, env, *constraint)
            })
        })
    {
        return Some(right);
    }

    enum_literal_constraint(db, env, left, right, operator, condition_expects_equality)
        .or_else(|| {
            builtin_literal_constraint(db, env, left, right, operator, condition_expects_equality)
        })
        .or_else(|| {
            ComparisonEvaluator::new(db, env, soundness_policy)
                .evaluate(left, right, branch, operator)
                .constraint(branch)
        })
}

/// Return the truthiness of `left == right` when it is known for every represented runtime value.
///
/// A result that only permits narrowing remains ambiguous because it can still evaluate either way.
pub(crate) fn equality_truthiness<'db>(
    db: &'db dyn Db,
    env: &ProgramEnvironment<'db>,
    left: Type<'db>,
    right: Type<'db>,
    soundness_policy: ComparisonSoundnessPolicy,
) -> Truthiness {
    comparison_truthiness(
        db,
        env,
        left,
        right,
        ComparisonOperator::Equality,
        soundness_policy,
    )
}

/// Return the truthiness of `left != right` when it is known for every represented runtime value.
///
/// A result that only permits narrowing remains ambiguous because it can still evaluate either way.
pub(super) fn inequality_truthiness<'db>(
    db: &'db dyn Db,
    env: &ProgramEnvironment<'db>,
    left: Type<'db>,
    right: Type<'db>,
    soundness_policy: ComparisonSoundnessPolicy,
) -> Truthiness {
    comparison_truthiness(
        db,
        env,
        left,
        right,
        ComparisonOperator::Inequality,
        soundness_policy,
    )
}

/// Evaluates tuple-element equality while reusing the active-comparison-set allocation across a
/// tuple walk. The set only detects recursive comparisons; results are not cached between
/// elements.
pub(super) struct TupleEqualityEvaluator<'db> {
    pub(in crate::types) evaluator: ComparisonEvaluator<'db>,
}

impl<'db> TupleEqualityEvaluator<'db> {
    pub(super) fn new(
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        soundness_policy: ComparisonSoundnessPolicy,
    ) -> Self {
        Self {
            evaluator: ComparisonEvaluator::for_truthiness(db, env, soundness_policy),
        }
    }

    pub(super) fn element_truthiness(
        &mut self,
        left: Type<'db>,
        right: Type<'db>,
    ) -> Result<Truthiness, BoolError<'db>> {
        let db = self.evaluator.db;
        source::infallible(source::tuple_element_sync(
            self,
            left,
            right,
            source::EqualityFacts,
            &source::OrdinaryEqualityEffects { db },
        ))
    }
}

fn comparison_truthiness<'db>(
    db: &'db dyn Db,
    env: &ProgramEnvironment<'db>,
    left: Type<'db>,
    right: Type<'db>,
    operator: ComparisonOperator,
    soundness_policy: ComparisonSoundnessPolicy,
) -> Truthiness {
    let mut evaluator = ComparisonEvaluator::for_truthiness(db, env, soundness_policy);
    source::infallible(source::comparison_truthiness_sync(
        &mut evaluator,
        left,
        right,
        operator,
        &source::OrdinaryEqualityEffects { db },
    ))
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
pub(in crate::types) enum ComparisonGoal {
    Constraint,
    Truthiness,
}

#[derive(Debug, Copy, Clone)]
pub(crate) struct ComparisonSoundnessPolicy {
    allow_unsafe_equality: bool,
}

impl ComparisonSoundnessPolicy {
    pub(super) const CONSERVATIVE: Self = Self {
        allow_unsafe_equality: false,
    };

    pub(crate) fn from_analysis_settings(settings: &AnalysisSettings) -> Self {
        Self {
            allow_unsafe_equality: !settings.strict_equality_semantics,
        }
    }
}

/// Identifies an active comparison evaluation.
///
/// Operand order and branch are significant because the left operand is the narrowing target.
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
pub(in crate::types) struct ComparisonKey<'db> {
    left: Type<'db>,
    right: Type<'db>,
    branch: ComparisonBranch,
    operator: ComparisonOperator,
}

#[cfg(feature = "experimental-analysis")]
impl<'db> ComparisonKey<'db> {
    /// Returns the stored operands hashed by equality recursion detection.
    pub(in crate::types) const fn operands(self) -> (Type<'db>, Type<'db>) {
        (self.left, self.right)
    }
}

/// Tracks comparisons that are already in progress so recursive evaluation terminates.
pub(in crate::types) struct ComparisonEvaluator<'db> {
    pub(in crate::types) db: &'db dyn Db,
    pub(in crate::types) env: ProgramEnvironment<'db>,
    pub(in crate::types) active: FxHashSet<ComparisonKey<'db>>,
    #[cfg(feature = "experimental-analysis")]
    pub(in crate::types) source_active_backing: usize,
    #[cfg(feature = "experimental-analysis")]
    /// Largest admitted hash/equality work bound for keys retained by this active set.
    pub(in crate::types) source_active_key_work: usize,
    pub(in crate::types) goal: ComparisonGoal,
    pub(in crate::types) soundness_policy: ComparisonSoundnessPolicy,
}

impl<'db> ComparisonEvaluator<'db> {
    fn new(
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        soundness_policy: ComparisonSoundnessPolicy,
    ) -> Self {
        Self {
            db,
            env: env.clone(),
            active: FxHashSet::default(),
            #[cfg(feature = "experimental-analysis")]
            source_active_backing: 0,
            #[cfg(feature = "experimental-analysis")]
            source_active_key_work: 0,
            goal: ComparisonGoal::Constraint,
            soundness_policy,
        }
    }

    fn for_truthiness(
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        soundness_policy: ComparisonSoundnessPolicy,
    ) -> Self {
        Self {
            db,
            env: env.clone(),
            active: FxHashSet::default(),
            #[cfg(feature = "experimental-analysis")]
            source_active_backing: 0,
            #[cfg(feature = "experimental-analysis")]
            source_active_key_work: 0,
            goal: ComparisonGoal::Truthiness,
            soundness_policy,
        }
    }

    fn comparison_semantics(
        &self,
        ty: Type<'db>,
        operator: ComparisonOperator,
    ) -> Option<KnownComparisonSemantics> {
        source::infallible(source::comparison_semantics_sync(
            self,
            ty,
            operator,
            source::EqualityFacts,
            &source::OrdinaryEqualityEffects { db: self.db },
        ))
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
        let db = self.db;
        source::infallible(source::evaluate_sync(
            self,
            left,
            right,
            branch,
            operator,
            source::EqualityFacts,
            &source::OrdinaryEqualityEffects { db },
        ))
    }
}

/// Handle dynamic values such as `Any` before checking individual enum members.
///
/// A one-member enum can exclude that member from `Any`. An enum with several members must not
/// exclude all of its members one at a time.
fn evaluate_dynamic_comparison_other<'db>(
    evaluator: &mut ComparisonEvaluator<'db>,
    env: &ProgramEnvironment<'db>,
    left: Type<'db>,
    right: Type<'db>,
    branch: ComparisonBranch,
    operator: ComparisonOperator,
) -> Option<ComparisonResult<'db>> {
    let db = evaluator.db;

    match (left, right) {
        (Type::Dynamic(_), other)
            if !operator.condition_expects_equality(branch)
                && all_values_compare_equal(evaluator, other, operator) =>
        {
            let excluded = if other.is_enum(db, env)
                && let Some(alternatives) = finite_alternatives(db, env, other, operator)
                && let [alternative] = alternatives.as_slice()
            {
                *alternative
            } else {
                other
            };
            Some(ComparisonResult::CanNarrow(
                IntersectionBuilder::new(db, env)
                    .add_positive(left)
                    .add_negative(excluded)
                    .build(),
            ))
        }
        (Type::Dynamic(_), _) | (_, Type::Dynamic(_)) => Some(ComparisonResult::Ambiguous),
        _ => None,
    }
}

/// Compare values not handled by the enum, dynamic, or finite-value stages.
fn evaluate_structural_comparison_other<'db>(
    evaluator: &mut ComparisonEvaluator<'db>,
    env: &ProgramEnvironment<'db>,
    left: Type<'db>,
    right: Type<'db>,
    branch: ComparisonBranch,
    operator: ComparisonOperator,
) -> ComparisonResult<'db> {
    let db = evaluator.db;
    source::infallible(source::structural_other_sync(
        evaluator,
        env,
        left,
        right,
        branch,
        operator,
        source::EqualityFacts,
        &source::OrdinaryEqualityEffects { db },
    ))
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
    env: &ProgramEnvironment<'db>,
    left: Type<'db>,
    right: Type<'db>,
    operator: ComparisonOperator,
    condition_expects_equality: bool,
) -> Option<Type<'db>> {
    let Type::LiteralValue(right) = right.resolve_type_alias(db) else {
        return None;
    };

    let equal_to_right =
        builtin_literals_equal_to(db, env, Type::LiteralValue(right), right.kind())?;

    if !condition_expects_equality {
        let equal_to_right = add_equal_enum_literals(
            db,
            env,
            left,
            right.kind(),
            operator,
            UnionBuilder::new(db, env).add(equal_to_right),
        );
        return Some(equal_to_right.build().negate(db, env));
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
    env: &ProgramEnvironment<'db>,
    literal_type: Type<'db>,
    literal: LiteralValueTypeKind<'db>,
) -> Option<Type<'db>> {
    let builder = match literal {
        LiteralValueTypeKind::Int(value) => {
            let mut builder = UnionBuilder::new(db, env).add(literal_type);
            if matches!(value.as_i64(), 0 | 1) {
                builder = builder.add(Type::bool_literal(value.as_i64() == 1));
            }
            builder
        }
        LiteralValueTypeKind::Bool(value) => UnionBuilder::new(db, env)
            .add(literal_type)
            .add(Type::int_literal(i64::from(value))),
        LiteralValueTypeKind::String(_) | LiteralValueTypeKind::Bytes(_) => {
            UnionBuilder::new(db, env).add(literal_type)
        }
        LiteralValueTypeKind::LiteralString | LiteralValueTypeKind::Enum(_) => return None,
    };
    Some(builder.build())
}

/// Add finite enum members in `ty` that are known to compare equal to `right`.
fn add_equal_enum_literals<'db>(
    db: &'db dyn Db,
    env: &ProgramEnvironment<'db>,
    ty: Type<'db>,
    right: LiteralValueTypeKind<'db>,
    operator: ComparisonOperator,
    mut builder: UnionBuilder<'db>,
) -> UnionBuilder<'db> {
    match ty.resolve_type_alias(db) {
        Type::Union(union) => {
            for element in union.elements(db) {
                builder = add_equal_enum_literals(db, env, *element, right, operator, builder);
            }
        }
        Type::LiteralValue(literal) => {
            if matches!(literal.kind(), LiteralValueTypeKind::Enum(_))
                && known_literal_equality(db, env, literal.kind(), right, operator) == Some(true)
            {
                builder = builder.add(Type::LiteralValue(literal));
            }
        }
        ty if let Some(alternatives) = finite_alternatives(db, env, ty, operator) => {
            for alternative in alternatives {
                builder = add_equal_enum_literals(db, env, alternative, right, operator, builder);
            }
        }
        _ => {}
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
    env: &ProgramEnvironment<'db>,
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
    if !is_same_enum_domain(db, env, left, right)
        || KnownComparisonSemantics::of_instance(
            db,
            env,
            right.enum_class_instance(db, env),
            operator,
        )
        .is_none()
    {
        return None;
    }

    let enum_class_literal = right.enum_class_literal(db);
    let name = enum_class_literal.resolve_member(db, right.name(db))?;
    let equal_to_right = Type::from(LiteralValueType::new(
        EnumLiteralType::new(db, enum_class_literal, name),
        right_literal.is_promotable(),
    ));
    Some(equal_to_right.negate_if(db, env, !condition_expects_equality))
}

/// Return whether every possible value of `ty` belongs to the same enum as `right`.
pub(super) fn is_same_enum_domain<'db>(
    db: &'db dyn Db,
    env: &ProgramEnvironment<'db>,
    ty: Type<'db>,
    right: EnumLiteralType<'db>,
) -> bool {
    // A proof made while another alias is active can still be disproved by a later union arm, so
    // completed visits must not be cached.
    #[derive(Default)]
    struct EnumDomainVisitor<'db> {
        active_specializations: ActiveRecursionDetector<Type<'db>>,
        active_definitions: ActiveRecursionDetector<Definition<'db>>,
    }

    fn visit<'db>(
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        ty: Type<'db>,
        right: EnumLiteralType<'db>,
        visitor: &EnumDomainVisitor<'db>,
    ) -> bool {
        match ty {
            // The same specialization preserves the domain; different arguments can introduce
            // values outside it even when the alias definition is the same.
            Type::TypeAlias(alias) => visitor.active_specializations.visit(
                &ty,
                || true,
                || {
                    visitor.active_definitions.visit(
                        &alias.definition(db),
                        || false,
                        || visit(db, env, alias.value_type(db), right, visitor),
                    )
                },
            ),
            Type::LiteralValue(literal) => matches!(
                literal.kind(),
                LiteralValueTypeKind::Enum(left)
                    if left.enum_class(db) == right.enum_class(db)
            ),
            Type::Union(union) => union
                .elements(db)
                .iter()
                .all(|&element| visit(db, env, element, right, visitor)),
            Type::NewTypeInstance(newtype) => {
                visit(db, env, newtype.concrete_base_type(db), right, visitor)
            }
            Type::NominalInstance(instance) => {
                instance.class_literal(db, env) == right.enum_class(db)
            }
            Type::EnumComplement(complement) => complement.enum_class(db) == right.enum_class(db),
            Type::Intersection(intersection) => intersection
                .positive(db)
                .iter()
                .any(|&element| visit(db, env, element, right, visitor)),
            _ => false,
        }
    }

    visit(db, env, ty, right, &EnumDomainVisitor::default())
}

/// Evaluate each alternative of the union being constrained and combine their branch results.
fn evaluate_union_left<'db>(
    evaluator: &mut ComparisonEvaluator<'db>,
    elements: &[Type<'db>],
    other: Type<'db>,
    branch: ComparisonBranch,
    operator: ComparisonOperator,
) -> ComparisonResult<'db> {
    let db = evaluator.db;
    if evaluator.goal == ComparisonGoal::Truthiness {
        return combine_definite_truthiness(
            elements
                .iter()
                .map(|element| evaluator.evaluate(*element, other, branch, operator)),
        );
    }

    let env = evaluator.env.clone();
    evaluate_target_union(db, &env, elements, branch, |element| {
        evaluator.evaluate(element, other, branch, operator)
    })
}

/// Combine comparison results for the alternatives of the union being constrained.
///
/// Alternatives that cannot satisfy the selected branch are removed. Dynamic alternatives retain
/// negative constraints for removed arms so that the result still describes the branch predicate.
fn evaluate_target_union<'db>(
    db: &'db dyn Db,
    env: &ProgramEnvironment<'db>,
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
    let mut removed = UnionBuilder::new(db, env);
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
    let mut builder = UnionBuilder::new(db, env);
    for narrowed in narrowed {
        let Some(mut narrowed) = narrowed else {
            continue;
        };
        // A surviving alternative that is disjoint from every rejected alternative already
        // satisfies their exclusions. Constructing those redundant exclusions can exponentially
        // expand intersections such as `Any & Literal["a"]` when the rejected alternatives are
        // similarly shaped intersections with other string literals.
        if let Some(removed) = removed
            && !narrowed.is_disjoint_from(db, env, removed)
        {
            narrowed = IntersectionBuilder::new(db, env)
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
    let db = evaluator.db;
    if evaluator.goal == ComparisonGoal::Truthiness {
        return combine_definite_truthiness(
            elements
                .iter()
                .map(|element| evaluator.evaluate(left, *element, branch, operator)),
        );
    }

    let env = evaluator.env.clone();
    evaluate_against_results(
        db,
        &env,
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
    env: &ProgramEnvironment<'db>,
    target: Type<'db>,
    branch: ComparisonBranch,
    results: impl IntoIterator<Item = ComparisonResult<'db>>,
) -> ComparisonResult<'db> {
    let mut all_true = true;
    let mut all_false = true;
    let mut builder = UnionBuilder::new(db, env);
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
    let db = evaluator.db;
    if evaluator.goal == ComparisonGoal::Truthiness {
        return combine_definite_truthiness(
            positive
                .iter()
                .map(|element| evaluator.evaluate(*element, other, branch, operator)),
        );
    }

    let mut any_true = false;
    let mut any_false = false;
    let mut any_ambiguous = false;
    let mut any_narrowing = false;
    let mut builder = IntersectionBuilder::new(db, &evaluator.env).add_positive(original);

    for element in positive {
        match evaluator.evaluate(*element, other, branch, operator) {
            ComparisonResult::AlwaysTrue => any_true = true,
            ComparisonResult::AlwaysFalse => any_false = true,
            ComparisonResult::CanNarrow(narrowed) => {
                // Literal-string origin is a static proof, not a runtime object property. An
                // untrusted string can therefore equal a literal even when their static types
                // are disjoint. Keep its original proof instead of making that branch unreachable.
                if operator.condition_expects_equality(branch)
                    && original.is_disjoint_from(db, &evaluator.env, narrowed)
                    && original
                        .identity_comparison_truthiness(db, &evaluator.env, narrowed)
                        .may_be_true()
                {
                    return ComparisonResult::Ambiguous;
                }

                any_narrowing = true;
                builder.add_positive_in_place(narrowed);
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
    env: &ProgramEnvironment<'db>,
    ty: Type<'db>,
    operator: ComparisonOperator,
) -> Option<Vec<Type<'db>>> {
    source::infallible(source::finite_alternatives_sync(
        env,
        ty,
        operator,
        &source::OrdinaryEqualityEffects { db },
    ))
}

fn finite_alternatives_other<'db>(
    db: &'db dyn Db,
    env: &ProgramEnvironment<'db>,
    ty: Type<'db>,
    operator: ComparisonOperator,
) -> Option<Vec<Type<'db>>> {
    source::infallible(source::finite_alternatives_other_sync(
        env,
        ty,
        operator,
        source::EqualityFacts,
        &source::OrdinaryEqualityEffects { db },
    ))
}

/// Return a constraint for literal pairs whose equality cannot be decided statically.
///
/// This primarily handles `LiteralString`, which can be constrained by a concrete string literal
/// or a string-valued enum member without having a single statically known runtime value.
fn narrow_literal_comparison<'db>(
    db: &'db dyn Db,
    env: &ProgramEnvironment<'db>,
    left: Type<'db>,
    right: Type<'db>,
    left_literal: LiteralValueTypeKind<'db>,
    right_literal: LiteralValueTypeKind<'db>,
    equality_is_positive: bool,
) -> ComparisonResult<'db> {
    match (left_literal, right_literal) {
        (LiteralValueTypeKind::LiteralString, LiteralValueTypeKind::String(_)) => {
            ComparisonResult::CanNarrow(right.negate_if(db, env, !equality_is_positive))
        }
        (LiteralValueTypeKind::String(_), LiteralValueTypeKind::LiteralString) => {
            ComparisonResult::CanNarrow(left.negate_if(db, env, !equality_is_positive))
        }
        (LiteralValueTypeKind::LiteralString, LiteralValueTypeKind::Enum(enum_literal)) => {
            narrow_literal_string_against_enum(db, env, enum_literal, equality_is_positive)
        }
        (LiteralValueTypeKind::Enum(enum_literal), LiteralValueTypeKind::LiteralString) => {
            narrow_literal_string_against_enum(db, env, enum_literal, equality_is_positive)
        }
        _ => ComparisonResult::Ambiguous,
    }
}

/// Narrow `LiteralString` against a string-valued enum member with inherited `str` semantics.
fn narrow_literal_string_against_enum<'db>(
    db: &'db dyn Db,
    env: &ProgramEnvironment<'db>,
    enum_literal: EnumLiteralType<'db>,
    equality_is_positive: bool,
) -> ComparisonResult<'db> {
    if KnownComparisonSemantics::of_type(
        db,
        env,
        Type::enum_literal(enum_literal),
        ComparisonOperator::Equality,
    ) != Some(KnownComparisonSemantics::Str)
    {
        return ComparisonResult::Ambiguous;
    }
    let Some(value @ Type::LiteralValue(_)) = enum_literal_value(db, env, enum_literal) else {
        return ComparisonResult::Ambiguous;
    };
    let Some(LiteralValueTypeKind::String(_)) = value.as_literal_value_kind() else {
        return ComparisonResult::Ambiguous;
    };
    let narrowed = UnionBuilder::new(db, env)
        .add(value)
        .add(Type::enum_literal(enum_literal))
        .build()
        .negate_if(db, env, !equality_is_positive);
    ComparisonResult::CanNarrow(narrowed)
}

/// Return the builtin comparison semantics assumed by unsafe equality narrowing.
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
    let env = evaluator.env.clone();
    let env = &env;

    if matches!(literal, LiteralValueTypeKind::LiteralString) {
        return match evaluator.comparison_semantics(other, operator) {
            Some(KnownComparisonSemantics::Str) => ComparisonResult::Ambiguous,
            Some(_) => compare_different_semantics(db, env, literal_type, other, operator),
            None => ComparisonResult::Ambiguous,
        };
    }

    let Some(literal_semantics) = KnownComparisonSemantics::of_literal(db, env, literal, operator)
    else {
        return ComparisonResult::Ambiguous;
    };
    let condition_expects_equality = operator.condition_expects_equality(branch);

    // Treat broad builtin types as if only the literal itself can compare equal. This is
    // intentionally unsafe: subclasses, including `bool` for `int`, can compare equal without
    // inhabiting the literal type. Explicitly typed subclasses do not take this path.
    if evaluator.soundness_policy.allow_unsafe_equality
        && condition_expects_equality
        && literal_operand == LiteralOperand::Other
        && let Some(other_semantics) = unsafe_narrowable_builtin_semantics(db, other)
    {
        return if literal_semantics == other_semantics {
            ComparisonResult::CanNarrow(literal_type)
        } else {
            operator.result_from_equality(false)
        };
    }

    match evaluator.comparison_semantics(other, operator) {
        Some(other_semantics) if literal_semantics != other_semantics => {
            compare_different_semantics(db, env, literal_type, other, operator)
        }
        // Object equality compares identity. `NewType` operands are evaluated using their concrete
        // base before reaching this arm, so erased identities cannot make these types appear
        // disjoint here.
        Some(KnownComparisonSemantics::Object)
            if literal_semantics == KnownComparisonSemantics::Object
                && other.is_disjoint_from(db, env, literal_type) =>
        {
            ComparisonResult::from_bool(operator == ComparisonOperator::Inequality)
        }
        // Inherited builtin comparison semantics do not imply type overlap. For example, a final
        // `int` subclass can compare equal to `1` despite being disjoint from `Literal[1]`.
        Some(_)
            if literal_operand == LiteralOperand::Other
                && !other.is_disjoint_from(db, env, literal_type) =>
        {
            ComparisonResult::CanNarrow(literal_type.negate_if(
                db,
                env,
                !condition_expects_equality,
            ))
        }
        Some(_) => ComparisonResult::Ambiguous,
        None if literal_operand == LiteralOperand::Other && !condition_expects_equality => {
            ComparisonResult::CanNarrow(literal_type.negate(db, env))
        }
        None => ComparisonResult::Ambiguous,
    }
}

/// Compare types that inherit different builtin comparison implementations.
///
/// A base-class annotation can contain instances of a known subclass with a different
/// implementation. For example, `Sequence[object]` can contain tuples, so its inherited
/// `object.__eq__` cannot rule out equality with `tuple[()]`. Only consider known inheritance
/// here: hypothetical multiple-inheritance subclasses should not prevent the default equality
/// semantics from narrowing unrelated classes.
fn compare_different_semantics<'db>(
    db: &'db dyn Db,
    env: &ProgramEnvironment<'db>,
    left: Type<'db>,
    right: Type<'db>,
    operator: ComparisonOperator,
) -> ComparisonResult<'db> {
    // NoneType is final and inherits only from object. This common case does not need
    // ancestry checks, which would otherwise be repeated for each member of an optional enum.
    if left.is_none(db) || right.is_none(db) {
        return operator.result_from_equality(false);
    }

    match (left, right) {
        (Type::Intersection(intersection), other) | (other, Type::Intersection(intersection)) => {
            // An intersection can only compare equal if all of its positive elements can.
            intersection
                .positive(db)
                .iter()
                .map(|&element| compare_different_semantics(db, env, element, other, operator))
                .find(|result| *result != ComparisonResult::Ambiguous)
                .unwrap_or(ComparisonResult::Ambiguous)
        }
        (left, right)
            if let (Some(left_class), Some(right_class)) =
                (left.nominal_class(db, env), right.nominal_class(db, env))
                && (left_class.is_subtype_of_class_literal(db, right_class.class_literal(db))
                    || right_class
                        .is_subtype_of_class_literal(db, left_class.class_literal(db))) =>
        {
            ComparisonResult::Ambiguous
        }
        _ => operator.result_from_equality(false),
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
pub(in crate::types) enum ComparisonOperator {
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
/// Runtime values with different known semantics cannot compare equal. The implementation inferred
/// for a static type may differ from that of a known subclass; see
/// [`compare_different_semantics`]. Types with custom or otherwise unknown comparison methods are not
/// assigned a value of this enum.
#[derive(Debug, Copy, Clone, PartialEq, Eq, get_size2::GetSize)]
pub(in crate::types) enum KnownComparisonSemantics {
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
    fn of_type<'db>(
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        ty: Type<'db>,
        operator: ComparisonOperator,
    ) -> Option<Self> {
        Self::of_type_with_policy(
            db,
            env,
            ty,
            operator,
            ComparisonSoundnessPolicy::CONSERVATIVE,
        )
    }

    /// Determine comparison semantics, optionally assuming that subclasses do not override the
    /// inherited comparison method.
    fn of_type_with_policy<'db>(
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        ty: Type<'db>,
        operator: ComparisonOperator,
        soundness_policy: ComparisonSoundnessPolicy,
    ) -> Option<Self> {
        source::infallible(source::known_semantics_sync(
            env,
            ty,
            operator,
            soundness_policy,
            source::EqualityFacts,
            &source::OrdinaryEqualityEffects { db },
        ))
    }

    /// Return the builtin comparison implementation used by a literal value.
    fn of_literal<'db>(
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
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
                Self::of_instance(db, env, enum_literal.enum_class_instance(db, env), operator)
            }
        }
    }

    /// Return the builtin comparison implementation inherited by an instance.
    ///
    /// Returns `None` when lookup finds custom comparison behavior.
    fn of_instance<'db>(
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        instance: Type<'db>,
        operator: ComparisonOperator,
    ) -> Option<Self> {
        source::infallible(source::instance_semantics_sync(
            env,
            instance,
            operator,
            source::EqualityFacts,
            &source::OrdinaryEqualityEffects { db },
        ))
    }
}

/// Look up a comparison method without falling back to `object`.
fn lookup_dunder<'db>(
    db: &'db dyn Db,
    env: &ProgramEnvironment<'db>,
    ty: Type<'db>,
    name: &'static str,
) -> PlaceAndQualifiers<'db> {
    ty.member_lookup_with_policy(db, env, name, MemberLookupPolicy::MRO_NO_OBJECT_FALLBACK)
}

/// Return the comparison result for two literals when their runtime values determine it.
///
/// This accounts for integer/boolean equality, enum aliases or enum values, and reflexive custom
/// enum comparison methods with a definite return type. `None` means comparison behavior is
/// insufficiently known to produce a definitive result.
fn known_literal_equality<'db>(
    db: &'db dyn Db,
    env: &ProgramEnvironment<'db>,
    left: LiteralValueTypeKind<'db>,
    right: LiteralValueTypeKind<'db>,
    operator: ComparisonOperator,
) -> Option<bool> {
    source::infallible(source::literal_equality_sync(
        env,
        left,
        right,
        operator,
        source::EqualityFacts,
        &source::OrdinaryEqualityEffects { db },
    ))
}

fn known_literal_equality_other<'db>(
    db: &'db dyn Db,
    env: &ProgramEnvironment<'db>,
    left: LiteralValueTypeKind<'db>,
    right: LiteralValueTypeKind<'db>,
    operator: ComparisonOperator,
) -> Option<bool> {
    if let (LiteralValueTypeKind::Enum(left_enum), LiteralValueTypeKind::Enum(right_enum)) =
        (left, right)
        && same_enum_member(db, left_enum, right_enum)
        && KnownComparisonSemantics::of_instance(
            db,
            env,
            left_enum.enum_class_instance(db, env),
            operator,
        )
        .is_none()
        && let Ok(bindings) = Type::enum_literal(left_enum).try_call_dunder_with_policy(
            db,
            env,
            operator.dunder(),
            &mut CallArguments::positional([Type::unknown()]),
            TypeContext::default(),
            MemberLookupPolicy::MRO_NO_OBJECT_FALLBACK
                | MemberLookupPolicy::MRO_NO_INT_OR_STR_LOOKUP,
        )
        && let Some(result) = bindings
            .return_type(db, env)
            .as_literal_value()
            .and_then(LiteralValueType::as_bool)
    {
        return Some(result == (operator == ComparisonOperator::Equality));
    }

    match (left, right) {
        (LiteralValueTypeKind::Enum(left), LiteralValueTypeKind::Enum(right)) => {
            let left_semantics = KnownComparisonSemantics::of_instance(
                db,
                env,
                left.enum_class_instance(db, env),
                operator,
            )?;
            let right_semantics = KnownComparisonSemantics::of_instance(
                db,
                env,
                right.enum_class_instance(db, env),
                operator,
            )?;
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
                env,
                enum_literal_value(db, env, left)?.as_literal_value_kind()?,
                enum_literal_value(db, env, right)?.as_literal_value_kind()?,
                ComparisonOperator::Equality,
            )
        }
        (LiteralValueTypeKind::Enum(enum_literal), other)
        | (other, LiteralValueTypeKind::Enum(enum_literal)) => {
            let enum_semantics = KnownComparisonSemantics::of_instance(
                db,
                env,
                enum_literal.enum_class_instance(db, env),
                operator,
            )?;
            if enum_semantics != KnownComparisonSemantics::of_literal(db, env, other, operator)? {
                return Some(false);
            }
            known_literal_equality(
                db,
                env,
                enum_literal_value(db, env, enum_literal)?.as_literal_value_kind()?,
                other,
                ComparisonOperator::Equality,
            )
        }
        (left, right) => {
            let left_semantics = KnownComparisonSemantics::of_literal(db, env, left, operator)?;
            let right_semantics = KnownComparisonSemantics::of_literal(db, env, right, operator)?;
            (left_semantics != right_semantics).then_some(false)
        }
    }
}

/// Return the statically known runtime value of an enum member.
///
/// Custom enum construction can replace the declared value, so members of such enums return `None`.
fn enum_literal_value<'db>(
    db: &'db dyn Db,
    env: &ProgramEnvironment<'db>,
    literal: EnumLiteralType<'db>,
) -> Option<Type<'db>> {
    let enum_class_literal = literal.enum_class_literal(db);
    let metadata = enum_metadata(db, enum_class_literal.class_literal(db))?;
    let name = enum_class_literal.resolve_member(db, literal.name(db))?;
    metadata.concrete_value_type(db, env, name)
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
