use ruff_python_ast::name::Name;

use crate::{Db, place::PlaceAndQualifiers};

use super::{
    CallArguments, EnumLiteralType, IntersectionBuilder, KnownClass, LiteralValueTypeKind,
    MemberLookupPolicy, Truthiness, Type, TypeContext, TypeVarBoundOrConstraints, UnionBuilder,
    enums::{enum_member_literals, enum_metadata},
};

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
    /// ```py
    /// from enum import IntEnum
    ///
    /// class Foo(IntEnum):
    ///     X = 1
    /// ```
    AlwaysTrue,

    /// The comparison always evaluates to false.
    ///
    /// Similar to [`AlwaysTrue`], this only describes the runtime comparison result; it does not
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
    fn constraint(self, is_positive: bool) -> Option<Type<'db>> {
        match self {
            ComparisonResult::AlwaysTrue => (!is_positive).then_some(Type::Never),
            ComparisonResult::AlwaysFalse => is_positive.then_some(Type::Never),
            ComparisonResult::CanNarrow(narrowed) => Some(narrowed),
            ComparisonResult::Ambiguous => None,
        }
    }
}

pub(super) fn evaluate_type_equality<'db>(
    db: &'db dyn Db,
    left: Type<'db>,
    right: Type<'db>,
    is_positive: bool,
) -> Option<Type<'db>> {
    primitive_literal_constraint(db, left, right, is_positive)
        .or_else(|| equality_result(db, left, right, is_positive).constraint(is_positive))
}

pub(super) fn evaluate_type_inequality<'db>(
    db: &'db dyn Db,
    left: Type<'db>,
    right: Type<'db>,
    is_positive: bool,
) -> Option<Type<'db>> {
    primitive_literal_constraint(db, left, right, !is_positive)
        .or_else(|| inequality_result(db, left, right, is_positive).constraint(is_positive))
}

pub(crate) fn equality_truthiness<'db>(
    db: &'db dyn Db,
    left: Type<'db>,
    right: Type<'db>,
) -> Truthiness {
    match equality_result(db, left, right, true) {
        ComparisonResult::AlwaysTrue => Truthiness::AlwaysTrue,
        ComparisonResult::AlwaysFalse => Truthiness::AlwaysFalse,
        ComparisonResult::CanNarrow(_) | ComparisonResult::Ambiguous => Truthiness::Ambiguous,
    }
}

fn equality_result<'db>(
    db: &'db dyn Db,
    left: Type<'db>,
    right: Type<'db>,
    is_positive: bool,
) -> ComparisonResult<'db> {
    let left = left.resolve_type_alias(db);
    let right = right.resolve_type_alias(db);

    if let Some(alternatives) = equality_alternatives(db, left) {
        return evaluate_union_left(db, &alternatives, right, is_positive, equality_result);
    }
    if let Some(alternatives) = equality_alternatives(db, right) {
        return evaluate_union_right(db, left, &alternatives, is_positive, equality_result);
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
            if !is_positive && other.is_single_valued(db) {
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
                if !is_positive && other.is_single_valued(db) {
                    ComparisonResult::CanNarrow(other.negate(db))
                } else {
                    ComparisonResult::Ambiguous
                }
            }
            Some(TypeVarBoundOrConstraints::Constraints(constraints)) => {
                equality_result(db, constraints.as_type(db), other, is_positive)
            }
        },
        (other, Type::TypeVar(var)) => match var.typevar(db).bound_or_constraints(db) {
            Some(TypeVarBoundOrConstraints::Constraints(constraints)) => {
                equality_result(db, other, constraints.as_type(db), is_positive)
            }
            None | Some(TypeVarBoundOrConstraints::UpperBound(_)) => ComparisonResult::Ambiguous,
        },

        (Type::NewTypeInstance(newtype), other) | (other, Type::NewTypeInstance(newtype)) => {
            match equality_result(db, newtype.concrete_base_type(db), other, is_positive) {
                ComparisonResult::AlwaysTrue => ComparisonResult::AlwaysTrue,
                ComparisonResult::AlwaysFalse => ComparisonResult::AlwaysFalse,
                ComparisonResult::CanNarrow(_) | ComparisonResult::Ambiguous => {
                    ComparisonResult::Ambiguous
                }
            }
        }

        (Type::Union(union), other) => {
            evaluate_union_left(db, union.elements(db), other, is_positive, equality_result)
        }
        (other, Type::Union(union)) => {
            evaluate_union_right(db, other, union.elements(db), is_positive, equality_result)
        }
        (Type::Intersection(intersection), other) => evaluate_intersection_left(
            db,
            Type::Intersection(intersection),
            intersection.positive(db),
            other,
            is_positive,
            equality_result,
        ),

        (Type::LiteralValue(left_literal), Type::LiteralValue(right_literal)) => {
            match known_literal_equality(
                db,
                left_literal.kind(),
                right_literal.kind(),
                ComparisonOperator::Equality,
            ) {
                Some(true) => ComparisonResult::AlwaysTrue,
                Some(false) => ComparisonResult::AlwaysFalse,
                None => narrow_literal_comparison(
                    db,
                    left,
                    right,
                    left_literal.kind(),
                    right_literal.kind(),
                    is_positive,
                ),
            }
        }

        (Type::LiteralValue(literal), other) => compare_literal_to_other(
            db,
            Type::LiteralValue(literal),
            literal.kind(),
            other,
            is_positive,
            ComparisonOperator::Equality,
            true,
        ),
        (other, Type::LiteralValue(literal)) => compare_literal_to_other(
            db,
            Type::LiteralValue(literal),
            literal.kind(),
            other,
            is_positive,
            ComparisonOperator::Equality,
            false,
        ),

        (Type::TypedDict(_), Type::TypedDict(_)) => ComparisonResult::Ambiguous,
        (Type::TypedDict(_), other) | (other, Type::TypedDict(_)) => {
            match known_equality_semantics(db, other) {
                Some(KnownComparisonSemantics::Dict) | None => ComparisonResult::Ambiguous,
                Some(_) => ComparisonResult::AlwaysFalse,
            }
        }

        (Type::ClassLiteral(left_class), Type::ClassLiteral(right_class)) => {
            if known_instance_semantics(
                db,
                left_class.metaclass_instance_type(db),
                ComparisonOperator::Equality,
            ) == Some(KnownComparisonSemantics::Object)
                && known_instance_semantics(
                    db,
                    right_class.metaclass_instance_type(db),
                    ComparisonOperator::Equality,
                ) == Some(KnownComparisonSemantics::Object)
            {
                ComparisonResult::from_bool(left_class == right_class)
            } else {
                call_comparison_dunder(db, left, right, "__eq__")
            }
        }

        (Type::ClassLiteral(class), other) | (other, Type::ClassLiteral(class)) => {
            if known_instance_semantics(
                db,
                class.metaclass_instance_type(db),
                ComparisonOperator::Equality,
            ) == Some(KnownComparisonSemantics::Object)
                && !matches!(other, Type::ClassLiteral(_))
            {
                ComparisonResult::AlwaysFalse
            } else {
                call_comparison_dunder(db, left, right, "__eq__")
            }
        }

        (Type::FunctionLiteral(left_function), Type::FunctionLiteral(right_function)) => {
            ComparisonResult::from_bool(left_function == right_function)
        }
        (Type::ModuleLiteral(left_module), Type::ModuleLiteral(right_module)) => {
            ComparisonResult::from_bool(left_module.module(db) == right_module.module(db))
        }
        (Type::SpecialForm(left_form), Type::SpecialForm(right_form)) => {
            ComparisonResult::from_bool(left_form == right_form)
        }

        (Type::NominalInstance(left_instance), Type::NominalInstance(right_instance)) => {
            compare_nominal_instances(
                db,
                left_instance,
                right_instance,
                ComparisonOperator::Equality,
            )
        }

        _ => call_comparison_dunder(db, left, right, "__eq__"),
    }
}

/// Return a constraint that does not depend on the target's currently inferred literal union.
///
/// Narrowing constraints participate in cyclic inference. Filtering `"B" | "C"` to `"B"` for the
/// false branch of `x == "C"` can freeze a loop before later iterations widen `x`. Constraining the
/// target with `~Literal["C"]` instead describes the predicate itself and remains valid as the cycle
/// reaches its fixed point.
fn primitive_literal_constraint<'db>(
    db: &'db dyn Db,
    left: Type<'db>,
    right: Type<'db>,
    condition_expects_equality: bool,
) -> Option<Type<'db>> {
    let is_builtin_primitive = |ty: Type<'db>| match ty.resolve_type_alias(db) {
        Type::LiteralValue(literal) => matches!(
            literal.kind(),
            LiteralValueTypeKind::Int(_)
                | LiteralValueTypeKind::Bool(_)
                | LiteralValueTypeKind::String(_)
                | LiteralValueTypeKind::Bytes(_)
        ),
        Type::NominalInstance(instance) => instance.has_known_class(db, KnownClass::Bool),
        _ => false,
    };
    let left_is_builtin_primitive = match left.resolve_type_alias(db) {
        Type::Union(union) => union.elements(db).iter().copied().all(is_builtin_primitive),
        left => is_builtin_primitive(left),
    };
    if !left_is_builtin_primitive {
        return None;
    }

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

    Some(equal_to_right.negate_if(db, !condition_expects_equality))
}

fn inequality_result<'db>(
    db: &'db dyn Db,
    left: Type<'db>,
    right: Type<'db>,
    is_positive: bool,
) -> ComparisonResult<'db> {
    let left = left.resolve_type_alias(db);
    let right = right.resolve_type_alias(db);

    if let Some(alternatives) = inequality_alternatives(db, left) {
        return evaluate_union_left(db, &alternatives, right, is_positive, inequality_result);
    }
    if let Some(alternatives) = inequality_alternatives(db, right) {
        return evaluate_union_right(db, left, &alternatives, is_positive, inequality_result);
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
            if is_positive && other.is_single_valued(db) {
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
                if is_positive && other.is_single_valued(db) {
                    ComparisonResult::CanNarrow(other.negate(db))
                } else {
                    ComparisonResult::Ambiguous
                }
            }
            Some(TypeVarBoundOrConstraints::Constraints(constraints)) => {
                inequality_result(db, constraints.as_type(db), other, is_positive)
            }
        },
        (other, Type::TypeVar(var)) => match var.typevar(db).bound_or_constraints(db) {
            Some(TypeVarBoundOrConstraints::Constraints(constraints)) => {
                inequality_result(db, other, constraints.as_type(db), is_positive)
            }
            None | Some(TypeVarBoundOrConstraints::UpperBound(_)) => ComparisonResult::Ambiguous,
        },

        (Type::NewTypeInstance(newtype), other) | (other, Type::NewTypeInstance(newtype)) => {
            match inequality_result(db, newtype.concrete_base_type(db), other, is_positive) {
                ComparisonResult::AlwaysTrue => ComparisonResult::AlwaysTrue,
                ComparisonResult::AlwaysFalse => ComparisonResult::AlwaysFalse,
                ComparisonResult::CanNarrow(_) | ComparisonResult::Ambiguous => {
                    ComparisonResult::Ambiguous
                }
            }
        }

        (Type::Union(union), other) => evaluate_union_left(
            db,
            union.elements(db),
            other,
            is_positive,
            inequality_result,
        ),
        (other, Type::Union(union)) => evaluate_union_right(
            db,
            other,
            union.elements(db),
            is_positive,
            inequality_result,
        ),
        (Type::Intersection(intersection), other) => evaluate_intersection_left(
            db,
            Type::Intersection(intersection),
            intersection.positive(db),
            other,
            is_positive,
            inequality_result,
        ),

        (Type::LiteralValue(left_literal), Type::LiteralValue(right_literal)) => {
            match known_literal_equality(
                db,
                left_literal.kind(),
                right_literal.kind(),
                ComparisonOperator::Inequality,
            ) {
                Some(true) => ComparisonResult::AlwaysFalse,
                Some(false) => ComparisonResult::AlwaysTrue,
                None => narrow_literal_comparison(
                    db,
                    left,
                    right,
                    left_literal.kind(),
                    right_literal.kind(),
                    !is_positive,
                )
                .negate(),
            }
        }

        (Type::LiteralValue(literal), other) => compare_literal_to_other(
            db,
            Type::LiteralValue(literal),
            literal.kind(),
            other,
            is_positive,
            ComparisonOperator::Inequality,
            true,
        ),
        (other, Type::LiteralValue(literal)) => compare_literal_to_other(
            db,
            Type::LiteralValue(literal),
            literal.kind(),
            other,
            is_positive,
            ComparisonOperator::Inequality,
            false,
        ),

        (Type::TypedDict(_), Type::TypedDict(_)) => ComparisonResult::Ambiguous,
        (Type::TypedDict(_), other) | (other, Type::TypedDict(_)) => {
            match known_inequality_semantics(db, other) {
                Some(KnownComparisonSemantics::Dict) | None => ComparisonResult::Ambiguous,
                Some(_) => ComparisonResult::AlwaysTrue,
            }
        }

        (Type::ClassLiteral(left_class), Type::ClassLiteral(right_class)) => {
            if known_instance_semantics(
                db,
                left_class.metaclass_instance_type(db),
                ComparisonOperator::Inequality,
            ) == Some(KnownComparisonSemantics::Object)
                && known_instance_semantics(
                    db,
                    right_class.metaclass_instance_type(db),
                    ComparisonOperator::Inequality,
                ) == Some(KnownComparisonSemantics::Object)
            {
                ComparisonResult::from_bool(left_class != right_class)
            } else {
                call_comparison_dunder(db, left, right, "__ne__")
            }
        }

        (Type::ClassLiteral(class), other) | (other, Type::ClassLiteral(class)) => {
            if known_instance_semantics(
                db,
                class.metaclass_instance_type(db),
                ComparisonOperator::Inequality,
            ) == Some(KnownComparisonSemantics::Object)
                && !matches!(other, Type::ClassLiteral(_))
            {
                ComparisonResult::AlwaysTrue
            } else {
                call_comparison_dunder(db, left, right, "__ne__")
            }
        }

        (Type::FunctionLiteral(left_function), Type::FunctionLiteral(right_function)) => {
            ComparisonResult::from_bool(left_function != right_function)
        }
        (Type::ModuleLiteral(left_module), Type::ModuleLiteral(right_module)) => {
            ComparisonResult::from_bool(left_module.module(db) != right_module.module(db))
        }
        (Type::SpecialForm(left_form), Type::SpecialForm(right_form)) => {
            ComparisonResult::from_bool(left_form != right_form)
        }

        (Type::NominalInstance(left_instance), Type::NominalInstance(right_instance)) => {
            compare_nominal_instances(
                db,
                left_instance,
                right_instance,
                ComparisonOperator::Inequality,
            )
        }

        _ => call_comparison_dunder(db, left, right, "__ne__"),
    }
}

impl ComparisonResult<'_> {
    fn from_bool(value: bool) -> Self {
        if value {
            ComparisonResult::AlwaysTrue
        } else {
            ComparisonResult::AlwaysFalse
        }
    }

    fn negate(self) -> Self {
        match self {
            ComparisonResult::AlwaysTrue => ComparisonResult::AlwaysFalse,
            ComparisonResult::AlwaysFalse => ComparisonResult::AlwaysTrue,
            ComparisonResult::CanNarrow(narrowed) => ComparisonResult::CanNarrow(narrowed),
            ComparisonResult::Ambiguous => ComparisonResult::Ambiguous,
        }
    }
}

fn evaluate_union_left<'db>(
    db: &'db dyn Db,
    elements: &[Type<'db>],
    other: Type<'db>,
    is_positive: bool,
    evaluate: fn(&'db dyn Db, Type<'db>, Type<'db>, bool) -> ComparisonResult<'db>,
) -> ComparisonResult<'db> {
    let mut all_true = true;
    let mut all_false = true;
    let mut narrowed = Vec::with_capacity(elements.len());
    let mut removed = UnionBuilder::new(db);
    let mut removed_any = false;

    for element in elements {
        let result = evaluate(db, *element, other, is_positive);
        match result {
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
    evaluate: fn(&'db dyn Db, Type<'db>, Type<'db>, bool) -> ComparisonResult<'db>,
) -> ComparisonResult<'db> {
    let mut all_true = true;
    let mut all_false = true;
    let mut builder = UnionBuilder::new(db);

    for element in elements {
        match evaluate(db, left, *element, is_positive) {
            ComparisonResult::AlwaysTrue => {
                all_false = false;
                if is_positive {
                    builder = builder.add(left);
                }
            }
            ComparisonResult::AlwaysFalse => {
                all_true = false;
                if !is_positive {
                    builder = builder.add(left);
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
                builder = builder.add(left);
            }
        }
    }

    if all_true {
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
    evaluate: fn(&'db dyn Db, Type<'db>, Type<'db>, bool) -> ComparisonResult<'db>,
) -> ComparisonResult<'db> {
    let mut any_true = false;
    let mut any_false = false;
    let mut builder = IntersectionBuilder::new(db).add_positive(original);

    for element in positive {
        match evaluate(db, *element, other, is_positive) {
            ComparisonResult::AlwaysTrue => any_true = true,
            ComparisonResult::AlwaysFalse => any_false = true,
            ComparisonResult::CanNarrow(narrowed) => {
                builder = builder.add_positive(narrowed);
            }
            ComparisonResult::Ambiguous => {}
        }
    }

    match (any_true, any_false) {
        (true, false) => ComparisonResult::AlwaysTrue,
        (false, true) => ComparisonResult::AlwaysFalse,
        (true, true) => ComparisonResult::Ambiguous,
        (false, false) => ComparisonResult::CanNarrow(builder.build()),
    }
}

fn equality_alternatives<'db>(db: &'db dyn Db, ty: Type<'db>) -> Option<Vec<Type<'db>>> {
    finite_alternatives(db, ty, ComparisonOperator::Equality)
}

fn inequality_alternatives<'db>(db: &'db dyn Db, ty: Type<'db>) -> Option<Vec<Type<'db>>> {
    finite_alternatives(db, ty, ComparisonOperator::Inequality)
}

fn finite_alternatives<'db>(
    db: &'db dyn Db,
    ty: Type<'db>,
    operator: ComparisonOperator,
) -> Option<Vec<Type<'db>>> {
    match ty {
        Type::EnumComplement(complement) => comparison_semantics(db, ty, operator)
            .is_some()
            .then(|| complement.remaining_literal_types(db)),
        Type::Intersection(intersection) => {
            let complement = intersection.enum_complement(db)?;
            comparison_semantics(db, ty, operator)
                .is_some()
                .then(|| complement.remaining_literal_types(db))
        }
        Type::NominalInstance(instance) if instance.has_known_class(db, KnownClass::Bool) => {
            Some(vec![Type::bool_literal(true), Type::bool_literal(false)])
        }
        Type::NominalInstance(instance)
            if enum_metadata(db, instance.class_literal(db)).is_some()
                && comparison_semantics(db, ty, operator).is_some() =>
        {
            Some(
                enum_member_literals(db, instance.class_literal(db), None)
                    .expect("enum metadata is available")
                    .collect(),
            )
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

fn narrow_literal_string_against_enum<'db>(
    db: &'db dyn Db,
    enum_literal: EnumLiteralType<'db>,
    equality_is_positive: bool,
) -> ComparisonResult<'db> {
    if comparison_semantics(
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
        return match comparison_semantics(db, other, operator) {
            Some(KnownComparisonSemantics::Str) => ComparisonResult::Ambiguous,
            Some(_) => ComparisonResult::from_bool(operator == ComparisonOperator::Inequality),
            None => call_comparison_dunder(db, literal_type, other, operator.dunder()),
        };
    }

    let Some(literal_semantics) = literal_semantics(db, literal, operator) else {
        return call_comparison_dunder(db, literal_type, other, operator.dunder());
    };
    match comparison_semantics(db, other, operator) {
        Some(other_semantics) if literal_semantics != other_semantics => {
            ComparisonResult::from_bool(operator == ComparisonOperator::Inequality)
        }
        Some(_) => ComparisonResult::Ambiguous,
        None => {
            let comparison_proves_target_is_not_literal = matches!(
                (operator, is_positive),
                (ComparisonOperator::Equality, false) | (ComparisonOperator::Inequality, true)
            );
            if !literal_is_target
                && comparison_proves_target_is_not_literal
                && literal_type.is_single_valued(db)
            {
                ComparisonResult::CanNarrow(literal_type.negate(db))
            } else {
                call_comparison_dunder(db, literal_type, other, operator.dunder())
            }
        }
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
    let Some(left_semantics) = comparison_semantics(db, left, operator) else {
        return call_comparison_dunder(db, left, right, operator.dunder());
    };
    let Some(right_semantics) = comparison_semantics(db, right, operator) else {
        return call_comparison_dunder(db, left, right, operator.dunder());
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
}

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
enum KnownComparisonSemantics {
    Object,
    Int,
    Str,
    Bytes,
    Tuple,
    Dict,
}

fn comparison_semantics<'db>(
    db: &'db dyn Db,
    ty: Type<'db>,
    operator: ComparisonOperator,
) -> Option<KnownComparisonSemantics> {
    match operator {
        ComparisonOperator::Equality => known_equality_semantics(db, ty),
        ComparisonOperator::Inequality => known_inequality_semantics(db, ty),
    }
}

fn known_equality_semantics<'db>(
    db: &'db dyn Db,
    ty: Type<'db>,
) -> Option<KnownComparisonSemantics> {
    known_semantics(db, ty, ComparisonOperator::Equality)
}

fn known_inequality_semantics<'db>(
    db: &'db dyn Db,
    ty: Type<'db>,
) -> Option<KnownComparisonSemantics> {
    known_semantics(db, ty, ComparisonOperator::Inequality)
}

fn known_semantics<'db>(
    db: &'db dyn Db,
    ty: Type<'db>,
    operator: ComparisonOperator,
) -> Option<KnownComparisonSemantics> {
    match ty {
        Type::LiteralValue(literal) => literal_semantics(db, literal.kind(), operator),
        Type::TypedDict(_) => Some(KnownComparisonSemantics::Dict),
        Type::EnumComplement(complement) => known_instance_semantics(
            db,
            complement.enum_class(db).to_non_generic_instance(db),
            operator,
        ),
        Type::Intersection(intersection) => {
            let complement = intersection.enum_complement(db)?;
            known_instance_semantics(
                db,
                complement.enum_class(db).to_non_generic_instance(db),
                operator,
            )
        }
        Type::NominalInstance(instance)
            if instance.class(db).is_final(db)
                || instance.tuple_spec(db).is_some()
                || enum_metadata(db, instance.class_literal(db)).is_some() =>
        {
            known_instance_semantics(db, ty, operator)
        }
        _ => None,
    }
}

fn literal_semantics<'db>(
    db: &'db dyn Db,
    literal: LiteralValueTypeKind<'db>,
    operator: ComparisonOperator,
) -> Option<KnownComparisonSemantics> {
    match literal {
        LiteralValueTypeKind::Int(_) | LiteralValueTypeKind::Bool(_) => {
            Some(KnownComparisonSemantics::Int)
        }
        LiteralValueTypeKind::String(_) | LiteralValueTypeKind::LiteralString => {
            Some(KnownComparisonSemantics::Str)
        }
        LiteralValueTypeKind::Bytes(_) => Some(KnownComparisonSemantics::Bytes),
        LiteralValueTypeKind::Enum(enum_literal) => {
            known_instance_semantics(db, enum_literal.enum_class_instance(db), operator)
        }
    }
}

fn known_instance_semantics<'db>(
    db: &'db dyn Db,
    instance: Type<'db>,
    operator: ComparisonOperator,
) -> Option<KnownComparisonSemantics> {
    if let Some(nominal) = instance.as_nominal_instance()
        && enum_metadata(db, nominal.class_literal(db)).is_some()
    {
        return known_enum_semantics(db, instance, operator);
    }

    let class = instance.to_meta_type(db);
    let dunder = lookup_dunder(db, class, operator.dunder());

    if dunder.place.is_undefined() {
        if operator == ComparisonOperator::Inequality
            && !lookup_dunder(db, class, "__eq__").place.is_undefined()
        {
            return None;
        }
        return Some(KnownComparisonSemantics::Object);
    }

    for (known_class, semantics) in [
        (KnownClass::Int, KnownComparisonSemantics::Int),
        (KnownClass::Str, KnownComparisonSemantics::Str),
        (KnownClass::Bytes, KnownComparisonSemantics::Bytes),
        (KnownClass::Tuple, KnownComparisonSemantics::Tuple),
        (KnownClass::Dict, KnownComparisonSemantics::Dict),
    ] {
        if dunder == lookup_dunder(db, known_class.to_class_literal(db), operator.dunder()) {
            return Some(semantics);
        }
    }
    None
}

fn known_enum_semantics<'db>(
    db: &'db dyn Db,
    instance: Type<'db>,
    operator: ComparisonOperator,
) -> Option<KnownComparisonSemantics> {
    let base_semantics = if instance.is_subtype_of(db, KnownClass::Str.to_instance(db)) {
        KnownComparisonSemantics::Str
    } else if instance.is_subtype_of(db, KnownClass::Int.to_instance(db)) {
        KnownComparisonSemantics::Int
    } else if instance.is_subtype_of(db, KnownClass::Bytes.to_instance(db)) {
        KnownComparisonSemantics::Bytes
    } else {
        KnownComparisonSemantics::Object
    };

    let has_custom_dunder = |name| {
        let class = instance.to_meta_type(db);
        let dunder = class.member_lookup_with_policy(
            db,
            Name::new_static(name),
            MemberLookupPolicy::MRO_NO_OBJECT_FALLBACK
                | MemberLookupPolicy::MRO_NO_INT_OR_STR_LOOKUP,
        );
        if dunder.place.is_undefined() {
            return false;
        }
        if base_semantics == KnownComparisonSemantics::Bytes
            && dunder == lookup_dunder(db, KnownClass::Bytes.to_class_literal(db), name)
        {
            return false;
        }
        true
    };

    match operator {
        ComparisonOperator::Equality => (!has_custom_dunder("__eq__")).then_some(base_semantics),
        ComparisonOperator::Inequality => {
            if has_custom_dunder("__ne__")
                || (base_semantics == KnownComparisonSemantics::Object
                    && has_custom_dunder("__eq__"))
            {
                None
            } else {
                Some(base_semantics)
            }
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
                known_instance_semantics(db, left.enum_class_instance(db), operator)?;
            let right_semantics =
                known_instance_semantics(db, right.enum_class_instance(db), operator)?;
            if left_semantics != right_semantics {
                return Some(false);
            }
            if left_semantics == KnownComparisonSemantics::Object {
                return Some(same_enum_member(db, left, right));
            }
            known_type_value_equality(
                db,
                enum_literal_value(db, left)?,
                enum_literal_value(db, right)?,
            )
        }
        (LiteralValueTypeKind::Enum(enum_literal), other)
        | (other, LiteralValueTypeKind::Enum(enum_literal)) => {
            let enum_semantics =
                known_instance_semantics(db, enum_literal.enum_class_instance(db), operator)?;
            if enum_semantics != literal_semantics(db, other, operator)? {
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
            let left_semantics = literal_semantics(db, left, operator)?;
            let right_semantics = literal_semantics(db, right, operator)?;
            (left_semantics != right_semantics).then_some(false)
        }
    }
}

fn known_type_value_equality<'db>(
    db: &'db dyn Db,
    left: Type<'db>,
    right: Type<'db>,
) -> Option<bool> {
    known_literal_equality(
        db,
        left.as_literal_value_kind()?,
        right.as_literal_value_kind()?,
        ComparisonOperator::Equality,
    )
}

fn enum_literal_value<'db>(db: &'db dyn Db, literal: EnumLiteralType<'db>) -> Option<Type<'db>> {
    let metadata = enum_metadata(db, literal.enum_class(db))?;
    let name = metadata.resolve_member(literal.name(db))?;
    if metadata.init_function.is_some()
        || metadata.new_function.is_some()
        || metadata.custom_enum_metaclass_new
    {
        return None;
    }
    if metadata.auto_members.contains(name) {
        metadata.value_type(db, name)
    } else {
        metadata.members.get(name).copied()
    }
}

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

fn call_comparison_dunder<'db>(
    db: &'db dyn Db,
    left: Type<'db>,
    right: Type<'db>,
    dunder: &'static str,
) -> ComparisonResult<'db> {
    let Ok(bindings) = left.try_call_dunder(
        db,
        dunder,
        CallArguments::positional([right]),
        TypeContext::default(),
    ) else {
        return ComparisonResult::Ambiguous;
    };
    match bindings.return_type(db).bool(db) {
        Truthiness::AlwaysTrue => ComparisonResult::AlwaysTrue,
        Truthiness::AlwaysFalse => ComparisonResult::AlwaysFalse,
        Truthiness::Ambiguous => ComparisonResult::Ambiguous,
    }
}
