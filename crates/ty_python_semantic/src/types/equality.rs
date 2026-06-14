use ruff_python_ast::name::Name;

use crate::{Db, place::PlaceAndQualifiers};

use super::{
    EnumLiteralType, IntersectionBuilder, KnownBoundMethodType, KnownClass, LiteralValueTypeKind,
    MemberLookupPolicy, Truthiness, Type, TypeVarBoundOrConstraints, UnionBuilder,
    enums::{enum_member_literals, enum_metadata},
};

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

impl<'db> ComparisonResult<'db> {
    fn from_bool(value: bool) -> Self {
        if value {
            ComparisonResult::AlwaysTrue
        } else {
            ComparisonResult::AlwaysFalse
        }
    }

    /// Convert this result into a constraint for a branch with the given truthiness.
    fn constraint(self, is_positive: bool) -> Option<Type<'db>> {
        match self {
            ComparisonResult::AlwaysTrue => (!is_positive).then_some(Type::Never),
            ComparisonResult::AlwaysFalse => is_positive.then_some(Type::Never),
            ComparisonResult::CanNarrow(narrowed) => Some(narrowed),
            ComparisonResult::Ambiguous => None,
        }
    }

    /// Preserve definite truthiness while discarding a conditional narrowing result.
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
) -> Option<Type<'db>> {
    enum_literal_constraint(db, left, right, ComparisonOperator::Equality, is_positive)
        .or_else(|| builtin_literal_constraint(db, left, right, is_positive))
        .or_else(|| {
            if comparison_domain(db, left, right, ComparisonOperator::Equality)
                == ComparisonDomain::Known
            {
                comparison_result(db, left, right, is_positive, ComparisonOperator::Equality)
                    .constraint(is_positive)
            } else {
                None
            }
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
) -> Option<Type<'db>> {
    enum_literal_constraint(
        db,
        left,
        right,
        ComparisonOperator::Inequality,
        !is_positive,
    )
    .or_else(|| builtin_literal_constraint(db, left, right, !is_positive))
    .or_else(|| {
        comparison_result(db, left, right, is_positive, ComparisonOperator::Inequality)
            .constraint(is_positive)
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
    match comparison_result(db, left, right, true, ComparisonOperator::Equality) {
        ComparisonResult::AlwaysTrue => Truthiness::AlwaysTrue,
        ComparisonResult::AlwaysFalse => Truthiness::AlwaysFalse,
        ComparisonResult::CanNarrow(_) | ComparisonResult::Ambiguous => Truthiness::Ambiguous,
    }
}

/// Evaluate a comparison recursively, treating `left` as the operand being constrained.
///
/// `is_positive` selects the branch whose constraint is accumulated when either operand expands
/// into multiple alternatives.
fn comparison_result<'db>(
    db: &'db dyn Db,
    left: Type<'db>,
    right: Type<'db>,
    is_positive: bool,
    operator: ComparisonOperator,
) -> ComparisonResult<'db> {
    let left = left.resolve_type_alias(db);
    let right = right.resolve_type_alias(db);

    if let Some(alternatives) = finite_alternatives(db, left, operator) {
        return evaluate_union_left(db, &alternatives, right, is_positive, operator);
    }
    if let Some(alternatives) = finite_alternatives(db, right, operator) {
        return evaluate_union_right(db, left, &alternatives, is_positive, operator);
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
            if !operator.condition_expects_equality(is_positive) && other.is_single_valued(db) {
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
                if !operator.condition_expects_equality(is_positive) && other.is_single_valued(db) {
                    ComparisonResult::CanNarrow(other.negate(db))
                } else {
                    ComparisonResult::Ambiguous
                }
            }
            Some(TypeVarBoundOrConstraints::Constraints(constraints)) => {
                comparison_result(db, constraints.as_type(db), other, is_positive, operator)
            }
        },
        (other, Type::TypeVar(var)) => match var.typevar(db).bound_or_constraints(db) {
            Some(TypeVarBoundOrConstraints::Constraints(constraints)) => {
                comparison_result(db, other, constraints.as_type(db), is_positive, operator)
            }
            None | Some(TypeVarBoundOrConstraints::UpperBound(_)) => ComparisonResult::Ambiguous,
        },

        (Type::NewTypeInstance(newtype), other) => comparison_result(
            db,
            newtype.concrete_base_type(db),
            other,
            is_positive,
            operator,
        )
        .discard_narrowing(),
        (other, Type::NewTypeInstance(newtype)) => comparison_result(
            db,
            other,
            newtype.concrete_base_type(db),
            is_positive,
            operator,
        )
        .discard_narrowing(),

        (Type::Union(union), other) => {
            evaluate_union_left(db, union.elements(db), other, is_positive, operator)
        }
        (other, Type::Union(union)) => {
            evaluate_union_right(db, other, union.elements(db), is_positive, operator)
        }
        (Type::Intersection(intersection), other) => evaluate_intersection_left(
            db,
            Type::Intersection(intersection),
            intersection.positive(db),
            other,
            is_positive,
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
                    operator.condition_expects_equality(is_positive),
                ),
            }
        }

        (Type::LiteralValue(literal), other) => compare_literal_to_other(
            db,
            Type::LiteralValue(literal),
            literal.kind(),
            other,
            is_positive,
            operator,
            true,
        ),
        (other, Type::LiteralValue(literal)) => compare_literal_to_other(
            db,
            Type::LiteralValue(literal),
            literal.kind(),
            other,
            is_positive,
            operator,
            false,
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

/// Return a predicate-shaped constraint for comparison with an `int`, `bool`, `str`, or `bytes`
/// literal.
///
/// Narrowing constraints participate in cyclic inference. Filtering `"B" | "C"` to `"B"` for the
/// false branch of `x == "C"` can freeze a loop before later iterations widen `x`. Constraining the
/// target with `~Literal["C"]` instead describes the predicate itself and remains valid as the cycle
/// reaches its fixed point.
///
/// The constraint also follows Python's equality between booleans and integers: `x != 0` excludes
/// both `Literal[0]` and `Literal[False]`, while `x != 1` excludes `Literal[1]` and `Literal[True]`.
fn builtin_literal_constraint<'db>(
    db: &'db dyn Db,
    left: Type<'db>,
    right: Type<'db>,
    condition_expects_equality: bool,
) -> Option<Type<'db>> {
    let Type::LiteralValue(right) = right.resolve_type_alias(db) else {
        return None;
    };

    let equal_to_right = match right.kind() {
        LiteralValueTypeKind::Int(value) => {
            let mut builder = UnionBuilder::new(db).add(Type::LiteralValue(right));
            if matches!(value.as_i64(), 0 | 1) {
                builder = builder.add(Type::bool_literal(value.as_i64() == 1));
            }
            builder.build()
        }
        LiteralValueTypeKind::Bool(value) => UnionBuilder::new(db)
            .add(Type::LiteralValue(right))
            .add(Type::int_literal(i64::from(value)))
            .build(),
        LiteralValueTypeKind::String(_) | LiteralValueTypeKind::Bytes(_) => {
            Type::LiteralValue(right)
        }
        LiteralValueTypeKind::LiteralString | LiteralValueTypeKind::Enum(_) => return None,
    };

    if !condition_expects_equality {
        return Some(equal_to_right.negate(db));
    }

    if matches!(
        right.kind(),
        LiteralValueTypeKind::String(_) | LiteralValueTypeKind::Bytes(_)
    ) && let Type::Union(union) = left.resolve_type_alias(db)
    {
        let mut excluded = UnionBuilder::new(db);
        let mut has_dynamic = false;

        for element in union.elements(db) {
            match element.resolve_type_alias(db) {
                Type::LiteralValue(left) => {
                    match known_literal_equality(
                        db,
                        left.kind(),
                        right.kind(),
                        ComparisonOperator::Equality,
                    ) {
                        Some(true) => {}
                        Some(false) => excluded = excluded.add(*element),
                        None => return None,
                    }
                }
                Type::Dynamic(_) => has_dynamic = true,
                Type::Intersection(intersection)
                    if !intersection.positive(db).is_empty()
                        && intersection.positive(db).iter().all(Type::is_dynamic) =>
                {
                    has_dynamic = true;
                }
                _ => return None,
            }
        }

        if has_dynamic {
            return excluded.try_build().map(|excluded| excluded.negate(db));
        }
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
    let LiteralValueTypeKind::Enum(right) = right.as_literal_value_kind()? else {
        return None;
    };
    let enum_class = right.enum_class(db);

    if !is_same_enum_domain(db, left, right)
        || KnownComparisonSemantics::of_instance(db, right.enum_class_instance(db), operator)
            .is_none()
    {
        return None;
    }

    let metadata = enum_metadata(db, enum_class)?;
    let name = metadata.resolve_member(right.name(db))?.clone();
    let equal_to_right = Type::enum_literal(EnumLiteralType::new(db, enum_class, name));
    Some(equal_to_right.negate_if(db, !condition_expects_equality))
}

fn is_same_enum_domain<'db>(db: &'db dyn Db, ty: Type<'db>, right: EnumLiteralType<'db>) -> bool {
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

fn evaluate_union_left<'db>(
    db: &'db dyn Db,
    elements: &[Type<'db>],
    other: Type<'db>,
    is_positive: bool,
    operator: ComparisonOperator,
) -> ComparisonResult<'db> {
    evaluate_target_union(db, elements, is_positive, |element| {
        comparison_result(db, element, other, is_positive, operator)
    })
}

/// Combine comparison results for the alternatives of the union being constrained.
///
/// Alternatives that cannot satisfy the selected branch are removed. Dynamic alternatives retain
/// negative constraints for removed arms so that the result still describes the branch predicate.
fn evaluate_target_union<'db>(
    db: &'db dyn Db,
    elements: &[Type<'db>],
    is_positive: bool,
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
                if is_positive {
                    narrowed.push(Some(*element));
                } else {
                    narrowed.push(None);
                    removed = removed.add(*element);
                    removed_any = true;
                }
            }
            ComparisonResult::AlwaysFalse => {
                all_true = false;
                if is_positive {
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
    for (original, narrowed) in elements.iter().zip(narrowed) {
        let Some(mut narrowed) = narrowed else {
            continue;
        };
        if original.is_dynamic()
            && let Some(removed) = removed
        {
            narrowed = IntersectionBuilder::new(db)
                .add_positive(narrowed)
                .add_negative(removed)
                .build();
        }
        builder = builder.add(narrowed);
    }
    ComparisonResult::CanNarrow(builder.build())
}

fn evaluate_union_right<'db>(
    db: &'db dyn Db,
    left: Type<'db>,
    elements: &[Type<'db>],
    is_positive: bool,
    operator: ComparisonOperator,
) -> ComparisonResult<'db> {
    evaluate_against_results(
        db,
        left,
        is_positive,
        elements
            .iter()
            .map(|element| comparison_result(db, left, *element, is_positive, operator)),
    )
}

/// Combine comparison results produced by alternatives of the non-target operand.
///
/// The target remains possible when any alternative can satisfy the selected branch; definite
/// truthiness is reported only when every alternative agrees.
fn evaluate_against_results<'db>(
    db: &'db dyn Db,
    target: Type<'db>,
    is_positive: bool,
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
                if is_positive {
                    builder = builder.add(target);
                }
            }
            ComparisonResult::AlwaysFalse => {
                all_true = false;
                if !is_positive {
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

fn evaluate_intersection_left<'db>(
    db: &'db dyn Db,
    original: Type<'db>,
    positive: &crate::FxOrderSet<Type<'db>>,
    other: Type<'db>,
    is_positive: bool,
    operator: ComparisonOperator,
) -> ComparisonResult<'db> {
    let mut any_true = false;
    let mut any_false = false;
    let mut any_ambiguous = false;
    let mut any_narrowing = false;
    let mut builder = IntersectionBuilder::new(db).add_positive(original);

    for element in positive {
        match comparison_result(db, *element, other, is_positive, operator) {
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

fn compare_literal_to_other<'db>(
    db: &'db dyn Db,
    literal_type: Type<'db>,
    literal: LiteralValueTypeKind<'db>,
    other: Type<'db>,
    is_positive: bool,
    operator: ComparisonOperator,
    literal_is_target: bool,
) -> ComparisonResult<'db> {
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
    let condition_expects_equality = operator.condition_expects_equality(is_positive);
    match KnownComparisonSemantics::of_type(db, other, operator) {
        Some(other_semantics) if literal_semantics != other_semantics => {
            ComparisonResult::from_bool(operator == ComparisonOperator::Inequality)
        }
        // Inherited builtin comparison semantics do not imply type overlap. For example, a final
        // `int` subclass can compare equal to `1` despite being disjoint from `Literal[1]`.
        Some(_)
            if !literal_is_target
                && literal_type.is_single_valued(db)
                && !other.is_disjoint_from(db, literal_type) =>
        {
            ComparisonResult::CanNarrow(literal_type.negate_if(db, !condition_expects_equality))
        }
        Some(_) => ComparisonResult::Ambiguous,
        None if !literal_is_target
            && !condition_expects_equality
            && literal_type.is_single_valued(db) =>
        {
            ComparisonResult::CanNarrow(literal_type.negate(db))
        }
        None => ComparisonResult::Ambiguous,
    }
}

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

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
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

    const fn condition_expects_equality(self, is_positive: bool) -> bool {
        matches!(
            (self, is_positive),
            (ComparisonOperator::Equality, true) | (ComparisonOperator::Inequality, false)
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
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
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
                if instance.class(db).is_final(db)
                    || instance.tuple_spec(db).is_some()
                    || enum_metadata(db, instance.class_literal(db)).is_some() =>
            {
                Self::of_instance(db, ty, operator)
            }
            _ => None,
        }
    }

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

    fn of_instance<'db>(
        db: &'db dyn Db,
        instance: Type<'db>,
        operator: ComparisonOperator,
    ) -> Option<Self> {
        if let Some(nominal) = instance.as_nominal_instance()
            && enum_metadata(db, nominal.class_literal(db)).is_some()
        {
            return Self::of_enum(db, instance, operator);
        }

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

    fn of_enum<'db>(
        db: &'db dyn Db,
        instance: Type<'db>,
        operator: ComparisonOperator,
    ) -> Option<Self> {
        let base_semantics = if instance.is_subtype_of(db, KnownClass::Str.to_instance(db)) {
            Self::Str
        } else if instance.is_subtype_of(db, KnownClass::Int.to_instance(db)) {
            Self::Int
        } else if instance.is_subtype_of(db, KnownClass::Bytes.to_instance(db)) {
            Self::Bytes
        } else {
            Self::Object
        };

        let class = instance.to_meta_type(db);
        let has_custom_dunder = |name| {
            let dunder = class.member_lookup_with_policy(
                db,
                Name::new_static(name),
                MemberLookupPolicy::MRO_NO_OBJECT_FALLBACK
                    | MemberLookupPolicy::MRO_NO_INT_OR_STR_LOOKUP,
            );
            if dunder.place.is_undefined() {
                return false;
            }
            if base_semantics == Self::Bytes
                && dunder == lookup_dunder(db, KnownClass::Bytes.to_class_literal(db), name)
            {
                return false;
            }
            true
        };

        match operator {
            ComparisonOperator::Equality => {
                (!has_custom_dunder("__eq__")).then_some(base_semantics)
            }
            ComparisonOperator::Inequality => {
                if has_custom_dunder("__ne__")
                    || (base_semantics == Self::Object && has_custom_dunder("__eq__"))
                {
                    None
                } else {
                    Some(base_semantics)
                }
            }
        }
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
                || instance
                    .class(db)
                    .known(db)
                    .is_some_and(|known| known == KnownClass::Bool || known.is_singleton())
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
            if left_semantics == KnownComparisonSemantics::Object {
                return Some(same_enum_member(db, left, right));
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
    let metadata = enum_metadata(db, literal.enum_class(db))?;
    let name = metadata.resolve_member(literal.name(db))?;
    if metadata.member_value_may_be_transformed(name) {
        return None;
    }
    if metadata.auto_members.contains(name) {
        metadata.value_type(db, name)
    } else {
        metadata.members.get(name).copied()
    }
}

/// Return whether two enum literals resolve to the same member, including aliases.
fn same_enum_member<'db>(
    db: &'db dyn Db,
    left: EnumLiteralType<'db>,
    right: EnumLiteralType<'db>,
) -> bool {
    if left.enum_class(db) != right.enum_class(db) {
        return false;
    }
    let Some(metadata) = enum_metadata(db, left.enum_class(db)) else {
        return left == right;
    };
    metadata.resolve_member(left.name(db)) == metadata.resolve_member(right.name(db))
}
