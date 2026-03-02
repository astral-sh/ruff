use ruff_python_ast as ast;
use ruff_text_size::TextRange;
use smallvec::SmallVec;

use crate::Db;
use crate::types::call::{CallArguments, CallDunderError};
use crate::types::constraints::ConstraintSetBuilder;
use crate::types::context::InferContext;
use crate::types::cyclic::CycleDetector;
use crate::types::tuple::TupleSpec;
use crate::types::{
    DynamicType, IntersectionBuilder, IntersectionType, KnownClass, KnownInstanceType,
    LiteralValueType, LiteralValueTypeKind, MemberLookupPolicy, Truthiness, Type, TypeContext,
    TypeVarBoundOrConstraints, UnionBuilder,
};

/// Whether the intersection type is on the left or right side of the comparison.
#[derive(Debug, Clone, Copy)]
enum IntersectionOn {
    Left,
    Right,
}

/// A [`CycleDetector`] that is used in [`infer_binary_type_comparison`].
pub(super) type BinaryComparisonVisitor<'db> = CycleDetector<
    ast::CmpOp,
    (Type<'db>, ast::CmpOp, Type<'db>),
    Result<Type<'db>, UnsupportedComparisonError<'db>>,
>;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RichCompareOperator {
    Eq,
    Ne,
    Gt,
    Ge,
    Lt,
    Le,
}

impl From<RichCompareOperator> for ast::CmpOp {
    fn from(value: RichCompareOperator) -> Self {
        match value {
            RichCompareOperator::Eq => ast::CmpOp::Eq,
            RichCompareOperator::Ne => ast::CmpOp::NotEq,
            RichCompareOperator::Lt => ast::CmpOp::Lt,
            RichCompareOperator::Le => ast::CmpOp::LtE,
            RichCompareOperator::Gt => ast::CmpOp::Gt,
            RichCompareOperator::Ge => ast::CmpOp::GtE,
        }
    }
}

impl RichCompareOperator {
    #[must_use]
    const fn dunder(self) -> &'static str {
        match self {
            RichCompareOperator::Eq => "__eq__",
            RichCompareOperator::Ne => "__ne__",
            RichCompareOperator::Lt => "__lt__",
            RichCompareOperator::Le => "__le__",
            RichCompareOperator::Gt => "__gt__",
            RichCompareOperator::Ge => "__ge__",
        }
    }

    #[must_use]
    const fn reflect(self) -> Self {
        match self {
            RichCompareOperator::Eq => RichCompareOperator::Eq,
            RichCompareOperator::Ne => RichCompareOperator::Ne,
            RichCompareOperator::Lt => RichCompareOperator::Gt,
            RichCompareOperator::Le => RichCompareOperator::Ge,
            RichCompareOperator::Gt => RichCompareOperator::Lt,
            RichCompareOperator::Ge => RichCompareOperator::Le,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum MembershipTestCompareOperator {
    In,
    NotIn,
}

impl From<MembershipTestCompareOperator> for ast::CmpOp {
    fn from(value: MembershipTestCompareOperator) -> Self {
        match value {
            MembershipTestCompareOperator::In => ast::CmpOp::In,
            MembershipTestCompareOperator::NotIn => ast::CmpOp::NotIn,
        }
    }
}

/// Context for a failed comparison operation.
///
/// `left_ty` and `right_ty` are the "low-level" types
/// that cannot be compared using `op`. For example,
/// when evaluating `(1, "foo") < (2, 3)`, the "high-level"
/// types of the operands are `tuple[Literal[1], Literal["foo"]]`
/// and `tuple[Literal[2], Literal[3]]`. Those aren't captured
/// in this struct, but the "low-level" types that mean that
/// the high-level types cannot be compared *are* captured in
/// this struct. In this case, those would be `Literal["foo"]`
/// and `Literal[3]`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct UnsupportedComparisonError<'db> {
    pub(crate) op: ast::CmpOp,
    pub(crate) left_ty: Type<'db>,
    pub(crate) right_ty: Type<'db>,
}

/// Infers the type of a binary comparison (e.g. 'left == right'). See
/// `TypeInferenceBuilder::infer_compare_expression` for the higher level logic dealing with
/// multi-comparison expressions.
///
/// If the operation is not supported, return an error (we need upstream context to emit a
/// diagnostic).
pub(super) fn infer_binary_type_comparison<'db>(
    context: &InferContext<'db, '_>,
    left: Type<'db>,
    op: ast::CmpOp,
    right: Type<'db>,
    range: TextRange,
    visitor: &BinaryComparisonVisitor<'db>,
) -> Result<Type<'db>, UnsupportedComparisonError<'db>> {
    let db = context.db();

    // Note: identity (is, is not) for equal builtin types is unreliable and not part of the
    // language spec.
    // - `[ast::CompOp::Is]`: return `false` if unequal, `bool` if equal
    // - `[ast::CompOp::IsNot]`: return `true` if unequal, `bool` if equal
    let try_dunder = |policy: MemberLookupPolicy| {
        let rich_comparison = |op| infer_rich_comparison(db, left, right, op, policy);
        let membership_test_comparison = |op, range: TextRange| {
            infer_membership_test_comparison(context, left, right, op, range)
        };

        match op {
            ast::CmpOp::Eq => rich_comparison(RichCompareOperator::Eq),
            ast::CmpOp::NotEq => rich_comparison(RichCompareOperator::Ne),
            ast::CmpOp::Lt => rich_comparison(RichCompareOperator::Lt),
            ast::CmpOp::LtE => rich_comparison(RichCompareOperator::Le),
            ast::CmpOp::Gt => rich_comparison(RichCompareOperator::Gt),
            ast::CmpOp::GtE => rich_comparison(RichCompareOperator::Ge),
            ast::CmpOp::In => membership_test_comparison(MembershipTestCompareOperator::In, range),
            ast::CmpOp::NotIn => {
                membership_test_comparison(MembershipTestCompareOperator::NotIn, range)
            }
            ast::CmpOp::Is => {
                if left.is_disjoint_from(db, right) {
                    Ok(Type::bool_literal(false))
                } else if left.is_singleton(db) && left.is_equivalent_to(db, right) {
                    Ok(Type::bool_literal(true))
                } else {
                    Ok(KnownClass::Bool.to_instance(db))
                }
            }
            ast::CmpOp::IsNot => {
                if left.is_disjoint_from(db, right) {
                    Ok(Type::bool_literal(true))
                } else if left.is_singleton(db) && left.is_equivalent_to(db, right) {
                    Ok(Type::bool_literal(false))
                } else {
                    Ok(KnownClass::Bool.to_instance(db))
                }
            }
        }
    };

    let comparison_result = match (left, right) {
        (Type::Union(union), other) => {
            let mut builder = UnionBuilder::new(db);
            for element in union.elements(db) {
                builder = builder.add(infer_binary_type_comparison(
                    context, *element, op, other, range, visitor,
                )?);
            }
            Some(Ok(builder.build()))
        }
        (other, Type::Union(union)) => {
            let mut builder = UnionBuilder::new(db);
            for element in union.elements(db) {
                builder = builder.add(infer_binary_type_comparison(
                    context, other, op, *element, range, visitor,
                )?);
            }
            Some(Ok(builder.build()))
        }

        (Type::Intersection(intersection), right) => {
            Some(
                infer_binary_intersection_type_comparison(
                    context,
                    intersection,
                    op,
                    right,
                    IntersectionOn::Left,
                    range,
                    visitor,
                )
                .map_err(|err| UnsupportedComparisonError {
                    op,
                    left_ty: left,
                    right_ty: err.right_ty,
                }),
            )
        }
        (left, Type::Intersection(intersection)) => {
            Some(
                infer_binary_intersection_type_comparison(
                    context,
                    intersection,
                    op,
                    left,
                    IntersectionOn::Right,
                    range,
                    visitor,
                )
                .map_err(|err| UnsupportedComparisonError {
                    op,
                    left_ty: err.left_ty,
                    right_ty: right,
                }),
            )
        }

        (Type::TypeAlias(alias), right) => Some(visitor.visit((left, op, right), || {
            infer_binary_type_comparison(context, alias.value_type(db), op, right, range, visitor)
        })),

        (left, Type::TypeAlias(alias)) => Some(visitor.visit((left, op, right), || {
            infer_binary_type_comparison(context, left, op, alias.value_type(db), range, visitor)
        })),

        // `try_dunder` works for almost all `NewType`s, but not for `NewType`s of `float` and
        // `complex`, where the concrete base type is a union. In that case it turns out the
        // `self` types of the dunder methods in typeshed don't match, because they don't get
        // the same `int | float` and `int | float | complex` special treatment that the
        // positional arguments get. In those cases we need to explicitly delegate to the base
        // type, so that it hits the `Type::Union` branches above.
        (Type::NewTypeInstance(newtype), right) => Some(
            try_dunder(MemberLookupPolicy::default()).or_else(|_| {
                visitor.visit((left, op, right), || {
                    infer_binary_type_comparison(
                        context,
                        newtype.concrete_base_type(db),
                        op,
                        right,
                        range,
                        visitor,
                    )
                })
            }),
        ),
        (left, Type::NewTypeInstance(newtype)) => Some(
            try_dunder(MemberLookupPolicy::default()).or_else(|_| {
                visitor.visit((left, op, right), || {
                    infer_binary_type_comparison(
                        context,
                        left,
                        op,
                        newtype.concrete_base_type(db),
                        range,
                        visitor,
                    )
                })
            }),
        ),

        // Similar to `NewType`s, `TypeVar`s with union bounds (like `bound=float` which becomes
        // `int | float`) need to delegate to the bound type.
        //
        // When both operands are the same bounded TypeVar, we check the comparison on the bound
        // type paired with itself.
        (Type::TypeVar(left_tvar), Type::TypeVar(right_tvar))
            if left_tvar.identity(db) == right_tvar.identity(db) =>
        {
            match left_tvar.typevar(db).bound_or_constraints(db) {
                Some(TypeVarBoundOrConstraints::UpperBound(bound)) => Some(
                    try_dunder(MemberLookupPolicy::default()).or_else(|_| {
                        visitor.visit((left, op, right), || {
                            infer_binary_type_comparison(
                                context, bound, op, bound, range, visitor,
                            )
                        })
                    }),
                ),
                Some(TypeVarBoundOrConstraints::Constraints(constraints)) => {
                    // For constrained TypeVars, check each constraint paired with itself.
                    let mut builder = UnionBuilder::new(db);
                    for &constraint in constraints.elements(db) {
                        builder = builder.add(infer_binary_type_comparison(
                            context, constraint, op, constraint, range, visitor,
                        )?);
                    }
                    Some(Ok(builder.build()))
                }
                None => None, // Fall through to default handling
            }
        }
        // When the left operand is a bounded TypeVar and the right is not a TypeVar,
        // delegate to the bound type.
        (Type::TypeVar(left_tvar), right) if !right.is_type_var() => {
            match left_tvar.typevar(db).bound_or_constraints(db) {
                Some(TypeVarBoundOrConstraints::UpperBound(bound)) => Some(
                    try_dunder(MemberLookupPolicy::default()).or_else(|_| {
                        visitor.visit((left, op, right), || {
                            infer_binary_type_comparison(
                                context, bound, op, right, range, visitor,
                            )
                        })
                    }),
                ),
                Some(TypeVarBoundOrConstraints::Constraints(constraints)) => {
                    let mut builder = UnionBuilder::new(db);
                    for &constraint in constraints.elements(db) {
                        builder = builder.add(infer_binary_type_comparison(
                            context, constraint, op, right, range, visitor,
                        )?);
                    }
                    Some(Ok(builder.build()))
                }
                None => None,
            }
        }
        // When the right operand is a bounded TypeVar and the left is not a TypeVar,
        // delegate to the bound type.
        (left, Type::TypeVar(right_tvar)) if !left.is_type_var() => {
            match right_tvar.typevar(db).bound_or_constraints(db) {
                Some(TypeVarBoundOrConstraints::UpperBound(bound)) => Some(
                    try_dunder(MemberLookupPolicy::default()).or_else(|_| {
                        visitor.visit((left, op, right), || {
                            infer_binary_type_comparison(
                                context, left, op, bound, range, visitor,
                            )
                        })
                    }),
                ),
                Some(TypeVarBoundOrConstraints::Constraints(constraints)) => {
                    let mut builder = UnionBuilder::new(db);
                    for &constraint in constraints.elements(db) {
                        builder = builder.add(infer_binary_type_comparison(
                            context, left, op, constraint, range, visitor,
                        )?);
                    }
                    Some(Ok(builder.build()))
                }
                None => None,
            }
        }

        (Type::LiteralValue(left_literal), Type::LiteralValue(right_literal)) => {
            match (left_literal.kind(), right_literal.kind()) {
                (LiteralValueTypeKind::Int(n), LiteralValueTypeKind::Int(m)) => {
                    Some(match op {
                        ast::CmpOp::Eq => Ok(Type::bool_literal(n == m)),
                        ast::CmpOp::NotEq => Ok(Type::bool_literal(n != m)),
                        ast::CmpOp::Lt => Ok(Type::bool_literal(n < m)),
                        ast::CmpOp::LtE => Ok(Type::bool_literal(n <= m)),
                        ast::CmpOp::Gt => Ok(Type::bool_literal(n > m)),
                        ast::CmpOp::GtE => Ok(Type::bool_literal(n >= m)),
                        // We cannot say that two equal int Literals will return True from an `is` or `is not` comparison.
                        // Even if they are the same value, they may not be the same object.
                        ast::CmpOp::Is => {
                            if n == m {
                                Ok(KnownClass::Bool.to_instance(db))
                            } else {
                                Ok(Type::bool_literal(false))
                            }
                        }
                        ast::CmpOp::IsNot => {
                            if n == m {
                                Ok(KnownClass::Bool.to_instance(db))
                            } else {
                                Ok(Type::bool_literal(true))
                            }
                        }
                        // Undefined for (int, int)
                        ast::CmpOp::In | ast::CmpOp::NotIn => Err(UnsupportedComparisonError {
                            op,
                            left_ty: left,
                            right_ty: right,
                        }),
                    })
                }
                // Booleans are coded as integers (False = 0, True = 1)
                (LiteralValueTypeKind::Int(n), LiteralValueTypeKind::Bool(b)) => Some(
                    infer_binary_type_comparison(
                        context,
                        Type::int_literal(n.as_i64()),
                        op,
                        Type::int_literal(i64::from(b)),
                        range,
                        visitor,
                    )
                    .map_err(|_| UnsupportedComparisonError {
                        op,
                        left_ty: left,
                        right_ty: right,
                    }),
                ),
                (LiteralValueTypeKind::Bool(b), LiteralValueTypeKind::Int(m)) => Some(
                    infer_binary_type_comparison(
                        context,
                        Type::int_literal(i64::from(b)),
                        op,
                        Type::int_literal(m.as_i64()),
                        range,
                        visitor,
                    )
                    .map_err(|_| UnsupportedComparisonError {
                        op,
                        left_ty: left,
                        right_ty: right,
                    }),
                ),
                (LiteralValueTypeKind::Bool(a), LiteralValueTypeKind::Bool(b)) => Some(
                    infer_binary_type_comparison(
                        context,
                        Type::int_literal(i64::from(a)),
                        op,
                        Type::int_literal(i64::from(b)),
                        range,
                        visitor,
                    )
                    .map_err(|_| UnsupportedComparisonError {
                        op,
                        left_ty: left,
                        right_ty: right,
                    }),
                ),

                (
                    LiteralValueTypeKind::String(salsa_s1),
                    LiteralValueTypeKind::String(salsa_s2),
                ) => {
                    let s1 = salsa_s1.value(db);
                    let s2 = salsa_s2.value(db);
                    let result = match op {
                        ast::CmpOp::Eq => Type::bool_literal(s1 == s2),
                        ast::CmpOp::NotEq => Type::bool_literal(s1 != s2),
                        ast::CmpOp::Lt => Type::bool_literal(s1 < s2),
                        ast::CmpOp::LtE => Type::bool_literal(s1 <= s2),
                        ast::CmpOp::Gt => Type::bool_literal(s1 > s2),
                        ast::CmpOp::GtE => Type::bool_literal(s1 >= s2),
                        ast::CmpOp::In => Type::bool_literal(s2.contains(s1)),
                        ast::CmpOp::NotIn => Type::bool_literal(!s2.contains(s1)),
                        ast::CmpOp::Is => {
                            if s1 == s2 {
                                KnownClass::Bool.to_instance(db)
                            } else {
                                Type::bool_literal(false)
                            }
                        }
                        ast::CmpOp::IsNot => {
                            if s1 == s2 {
                                KnownClass::Bool.to_instance(db)
                            } else {
                                Type::bool_literal(true)
                            }
                        }
                    };
                    Some(Ok(result))
                }

                (
                    LiteralValueTypeKind::Bytes(salsa_b1),
                    LiteralValueTypeKind::Bytes(salsa_b2),
                ) => {
                    let b1 = salsa_b1.value(db);
                    let b2 = salsa_b2.value(db);
                    let result = match op {
                        ast::CmpOp::Eq => Type::bool_literal(b1 == b2),
                        ast::CmpOp::NotEq => Type::bool_literal(b1 != b2),
                        ast::CmpOp::Lt => Type::bool_literal(b1 < b2),
                        ast::CmpOp::LtE => Type::bool_literal(b1 <= b2),
                        ast::CmpOp::Gt => Type::bool_literal(b1 > b2),
                        ast::CmpOp::GtE => Type::bool_literal(b1 >= b2),
                        ast::CmpOp::In => {
                            Type::bool_literal(memchr::memmem::find(b2, b1).is_some())
                        }
                        ast::CmpOp::NotIn => {
                            Type::bool_literal(memchr::memmem::find(b2, b1).is_none())
                        }
                        ast::CmpOp::Is => {
                            if b1 == b2 {
                                KnownClass::Bool.to_instance(db)
                            } else {
                                Type::bool_literal(false)
                            }
                        }
                        ast::CmpOp::IsNot => {
                            if b1 == b2 {
                                KnownClass::Bool.to_instance(db)
                            } else {
                                Type::bool_literal(true)
                            }
                        }
                    };
                    Some(Ok(result))
                }

                (LiteralValueTypeKind::Enum(literal_1), LiteralValueTypeKind::Enum(literal_2))
                    if op == ast::CmpOp::Eq =>
                {
                    Some(Ok(
                        match try_dunder(MemberLookupPolicy::MRO_NO_OBJECT_FALLBACK) {
                            Ok(ty) => ty,
                            Err(_) => Type::bool_literal(literal_1 == literal_2),
                        },
                    ))
                }
                (LiteralValueTypeKind::Enum(literal_1), LiteralValueTypeKind::Enum(literal_2))
                    if op == ast::CmpOp::NotEq =>
                {
                    Some(Ok(
                        match try_dunder(MemberLookupPolicy::MRO_NO_OBJECT_FALLBACK) {
                            Ok(ty) => ty,
                            Err(_) => Type::bool_literal(literal_1 != literal_2),
                        },
                    ))
                }

                _ => None,
            }
        }

        (
            Type::KnownInstance(KnownInstanceType::ConstraintSet(left)),
            Type::KnownInstance(KnownInstanceType::ConstraintSet(right)),
        ) => {
            let constraints = ConstraintSetBuilder::new();
            let left = constraints.load(left.constraints(db));
            let right = constraints.load(right.constraints(db));
            let result = left.iff(db, &constraints, right);
            let equivalent = result.is_always_satisfied(db);
            match op {
                ast::CmpOp::Eq => Some(Ok(Type::bool_literal(equivalent))),
                ast::CmpOp::NotEq => Some(Ok(Type::bool_literal(!equivalent))),
                _ => None,
            }
        }

        (Type::NominalInstance(nominal1), Type::NominalInstance(nominal2)) => nominal1
            .tuple_spec(db)
            .and_then(|lhs_tuple| Some((lhs_tuple, nominal2.tuple_spec(db)?)))
            .map(|(lhs_tuple, rhs_tuple)| {
                let tuple_rich_comparison = |rich_op| {
                    visitor.visit((left, op, right), || {
                        infer_tuple_rich_comparison(
                            context, &lhs_tuple, rich_op, &rhs_tuple, range, visitor,
                        )
                    })
                };

                match op {
                    ast::CmpOp::Eq => tuple_rich_comparison(RichCompareOperator::Eq),
                    ast::CmpOp::NotEq => tuple_rich_comparison(RichCompareOperator::Ne),
                    ast::CmpOp::Lt => tuple_rich_comparison(RichCompareOperator::Lt),
                    ast::CmpOp::LtE => tuple_rich_comparison(RichCompareOperator::Le),
                    ast::CmpOp::Gt => tuple_rich_comparison(RichCompareOperator::Gt),
                    ast::CmpOp::GtE => tuple_rich_comparison(RichCompareOperator::Ge),
                    ast::CmpOp::In | ast::CmpOp::NotIn => {
                        let mut any_eq = false;
                        let mut any_ambiguous = false;

                        for ty in rhs_tuple.iter_all_elements() {
                            let eq_result = infer_binary_type_comparison(
                                context,
                                left,
                                ast::CmpOp::Eq,
                                ty,
                                range,
                                visitor,
                            )
                            .expect("infer_binary_type_comparison should never return None for `CmpOp::Eq`");

                            match eq_result {
                                todo @ Type::Dynamic(DynamicType::Todo(_)) => return Ok(todo),
                                // It's okay to ignore errors here because Python doesn't call `__bool__`
                                // for different union variants. Instead, this is just for us to
                                // evaluate a possibly truthy value to `false` or `true`.
                                ty => match ty.bool(db) {
                                    Truthiness::AlwaysTrue => any_eq = true,
                                    Truthiness::AlwaysFalse => (),
                                    Truthiness::Ambiguous => any_ambiguous = true,
                                },
                            }
                        }

                        if any_eq {
                            Ok(Type::bool_literal(op.is_in()))
                        } else if !any_ambiguous {
                            Ok(Type::bool_literal(op.is_not_in()))
                        } else {
                            Ok(KnownClass::Bool.to_instance(db))
                        }
                    }
                    ast::CmpOp::Is | ast::CmpOp::IsNot => {
                        // - `[ast::CmpOp::Is]`: returns `false` if the elements are definitely unequal, otherwise `bool`
                        // - `[ast::CmpOp::IsNot]`: returns `true` if the elements are definitely unequal, otherwise `bool`
                        let eq_result =
                            tuple_rich_comparison(RichCompareOperator::Eq).expect(
                                "infer_binary_type_comparison should never return None for `CmpOp::Eq`",
                            );

                        Ok(match eq_result {
                            todo @ Type::Dynamic(DynamicType::Todo(_)) => todo,
                            // It's okay to ignore errors here because Python doesn't call `__bool__`
                            // for `is` and `is not` comparisons. This is an implementation detail
                            // for how we determine the truthiness of a type.
                            ty => match ty.bool(db) {
                                Truthiness::AlwaysFalse => Type::bool_literal(op.is_is_not()),
                                _ => KnownClass::Bool.to_instance(db),
                            },
                        })
                    }
                }
            }),

        _ => None,
    };

    if let Some(result) = comparison_result {
        return result;
    }

    // Final generalized fallback: lookup the rich comparison `__dunder__` methods
    try_dunder(MemberLookupPolicy::default())
}

fn infer_binary_intersection_type_comparison<'db>(
    context: &InferContext<'db, '_>,
    intersection: IntersectionType<'db>,
    op: ast::CmpOp,
    other: Type<'db>,
    intersection_on: IntersectionOn,
    range: TextRange,
    visitor: &BinaryComparisonVisitor<'db>,
) -> Result<Type<'db>, UnsupportedComparisonError<'db>> {
    enum State<'db> {
        // We have not seen any positive elements (yet)
        NoPositiveElements,
        // The operator was unsupported on all elements that we have seen so far.
        // Contains the first error we encountered.
        UnsupportedOnAllElements(UnsupportedComparisonError<'db>),
        // The operator was supported on at least one positive element.
        Supported,
    }

    let db = context.db();

    // If a comparison yields a definitive true/false answer on a (positive) part
    // of an intersection type, it will also yield a definitive answer on the full
    // intersection type, which is even more specific.
    for pos in intersection.positive(db) {
        let result = match intersection_on {
            IntersectionOn::Left => {
                infer_binary_type_comparison(context, *pos, op, other, range, visitor)
            }
            IntersectionOn::Right => {
                infer_binary_type_comparison(context, other, op, *pos, range, visitor)
            }
        };

        if result
            .ok()
            .and_then(Type::as_literal_value)
            .is_some_and(LiteralValueType::is_bool)
        {
            return result;
        }
    }

    // For negative contributions to the intersection type, there are only a few
    // special cases that allow us to narrow down the result type of the comparison.
    for neg in intersection.negative(db) {
        let result = match intersection_on {
            IntersectionOn::Left => {
                infer_binary_type_comparison(context, *neg, op, other, range, visitor).ok()
            }
            IntersectionOn::Right => {
                infer_binary_type_comparison(context, other, op, *neg, range, visitor).ok()
            }
        }
        .and_then(Type::as_literal_value_kind);

        match (op, result) {
            (ast::CmpOp::Is, Some(LiteralValueTypeKind::Bool(true))) => {
                return Ok(Type::bool_literal(false));
            }
            (ast::CmpOp::IsNot, Some(LiteralValueTypeKind::Bool(false))) => {
                return Ok(Type::bool_literal(true));
            }
            _ => {}
        }
    }

    // If none of the simplifications above apply, we still need to return *some*
    // result type for the comparison 'T_inter `op` T_other' (or reversed), where
    //
    //    T_inter = P1 & P2 & ... & Pn & ~N1 & ~N2 & ... & ~Nm
    //
    // is the intersection type. If f(T) is the function that computes the result
    // type of a `op`-comparison with `T_other`, we are interested in f(T_inter).
    // Since we can't compute it exactly, we return the following approximation:
    //
    //   f(T_inter) = f(P1) & f(P2) & ... & f(Pn)
    //
    // The reason for this is the following: In general, for any function 'f', the
    // set f(A) & f(B) is *larger than or equal to* the set f(A & B). This means
    // that we will return a type that is possibly wider than it could be, but
    // never wrong.
    //
    // However, we do have to leave out the negative contributions. If we were to
    // add a contribution like ~f(N1), we would potentially infer result types
    // that are too narrow.
    //
    // As an example for this, consider the intersection type `int & ~Literal[1]`.
    // If 'f' would be the `==`-comparison with 2, we obviously can't tell if that
    // answer would be true or false, so we need to return `bool`. And indeed, we
    // we have (glossing over notational details):
    //
    //   f(int & ~1)
    //       = f({..., -1, 0, 2, 3, ...})
    //       = {..., False, False, True, False, ...}
    //       = bool
    //
    // On the other hand, if we were to compute
    //
    //   f(int) & ~f(1)
    //       = bool & ~False
    //       = True
    //
    // we would get a result type `Literal[True]` which is too narrow.
    //
    let mut builder = IntersectionBuilder::new(db);

    builder = builder.add_positive(KnownClass::Bool.to_instance(db));

    let mut state = State::NoPositiveElements;

    for pos in intersection.positive(db) {
        let result = match intersection_on {
            IntersectionOn::Left => {
                infer_binary_type_comparison(context, *pos, op, other, range, visitor)
            }
            IntersectionOn::Right => {
                infer_binary_type_comparison(context, other, op, *pos, range, visitor)
            }
        };

        match result {
            Ok(ty) => {
                state = State::Supported;
                builder = builder.add_positive(ty);
            }
            Err(error) => {
                match state {
                    State::NoPositiveElements => {
                        // This is the first positive element, but the operation is not supported.
                        // Store the error and continue.
                        state = State::UnsupportedOnAllElements(error);
                    }
                    State::UnsupportedOnAllElements(_) => {
                        // We already have an error stored, and continue to see elements on which
                        // the operator is not supported. Continue with the same state (only keep
                        // the first error).
                    }
                    State::Supported => {
                        // We previously saw a positive element that supported the operator,
                        // so the overall operation is still supported.
                    }
                }
            }
        }
    }

    match state {
        State::Supported => Ok(builder.build()),
        State::NoPositiveElements => {
            // We didn't see any positive elements, check if the operation is supported on `object`:
            match intersection_on {
                IntersectionOn::Left => {
                    infer_binary_type_comparison(context, Type::object(), op, other, range, visitor)
                }
                IntersectionOn::Right => {
                    infer_binary_type_comparison(context, other, op, Type::object(), range, visitor)
                }
            }
        }
        State::UnsupportedOnAllElements(error) => Err(error),
    }
}

/// Rich comparison in Python are the operators `==`, `!=`, `<`, `<=`, `>`, and `>=`. Their
/// behaviour can be edited for classes by implementing corresponding dunder methods.
/// This function performs rich comparison between two types and returns the resulting type.
/// see `<https://docs.python.org/3/reference/datamodel.html#object.__lt__>`
fn infer_rich_comparison<'db>(
    db: &'db dyn Db,
    left: Type<'db>,
    right: Type<'db>,
    op: RichCompareOperator,
    policy: MemberLookupPolicy,
) -> Result<Type<'db>, UnsupportedComparisonError<'db>> {
    // The following resource has details about the rich comparison algorithm:
    // https://snarky.ca/unravelling-rich-comparison-operators/
    let call_dunder = |op: RichCompareOperator, left: Type<'db>, right: Type<'db>| {
        left.try_call_dunder_with_policy(
            db,
            op.dunder(),
            &mut CallArguments::positional([right]),
            TypeContext::default(),
            policy,
        )
        .map(|outcome| outcome.return_type(db))
        .ok()
    };

    // The reflected dunder has priority if the right-hand side is a strict subclass of the left-hand side.
    if left != right && right.is_subtype_of(db, left) {
        call_dunder(op.reflect(), right, left).or_else(|| call_dunder(op, left, right))
    } else {
        call_dunder(op, left, right).or_else(|| call_dunder(op.reflect(), right, left))
    }
    .or_else(|| {
        // When no appropriate method returns any value other than NotImplemented,
        // the `==` and `!=` operators will fall back to `is` and `is not`, respectively.
        // refer to `<https://docs.python.org/3/reference/datamodel.html#object.__eq__>`
        if matches!(op, RichCompareOperator::Eq | RichCompareOperator::Ne)
            // This branch implements specific behavior of the `__eq__` and `__ne__` methods
            // on `object`, so it does not apply if we skip looking up attributes on `object`.
            && !policy.mro_no_object_fallback()
        {
            Some(KnownClass::Bool.to_instance(db))
        } else {
            None
        }
    })
    .ok_or_else(|| UnsupportedComparisonError {
        op: op.into(),
        left_ty: left,
        right_ty: right,
    })
}

/// Performs a membership test (`in` and `not in`) between two instances and returns the resulting type, or `None` if the test is unsupported.
/// The behavior can be customized in Python by implementing `__contains__`, `__iter__`, or `__getitem__` methods.
/// See `<https://docs.python.org/3/reference/datamodel.html#object.__contains__>`
/// and `<https://docs.python.org/3/reference/expressions.html#membership-test-details>`
fn infer_membership_test_comparison<'db>(
    context: &InferContext<'db, '_>,
    left: Type<'db>,
    right: Type<'db>,
    op: MembershipTestCompareOperator,
    range: TextRange,
) -> Result<Type<'db>, UnsupportedComparisonError<'db>> {
    let db = context.db();
    let compare_result_opt = match right.try_call_dunder(
        db,
        "__contains__",
        CallArguments::positional([left]),
        TypeContext::default(),
    ) {
        // If `__contains__` is available, it is used directly for the membership test.
        Ok(bindings) => Some(bindings.return_type(db)),
        // If `__contains__` is not available or possibly unbound,
        // fall back to iteration-based membership test.
        Err(CallDunderError::MethodNotAvailable | CallDunderError::PossiblyUnbound(_)) => right
            .try_iterate(db)
            .map(|_| KnownClass::Bool.to_instance(db))
            .ok(),
        // `__contains__` exists but can't be called with the given arguments.
        Err(CallDunderError::CallError(..)) => None,
    };

    compare_result_opt
        .map(|ty| {
            if matches!(ty, Type::Dynamic(DynamicType::Todo(_))) {
                return ty;
            }

            let truthiness = ty.try_bool(db).unwrap_or_else(|err| {
                err.report_diagnostic(context, range);
                err.fallback_truthiness()
            });

            match op {
                MembershipTestCompareOperator::In => truthiness.into_type(db),
                MembershipTestCompareOperator::NotIn => truthiness.negate().into_type(db),
            }
        })
        .ok_or_else(|| UnsupportedComparisonError {
            op: op.into(),
            left_ty: left,
            right_ty: right,
        })
}

/// Simulates rich comparison between tuples and returns the inferred result.
/// This performs a lexicographic comparison, returning a union of all possible return types that could result from the comparison.
///
/// basically it's based on cpython's `tuple_richcompare`
/// see `<https://github.com/python/cpython/blob/9d6366b60d01305fc5e45100e0cd13e358aa397d/Objects/tupleobject.c#L637>`
fn infer_tuple_rich_comparison<'db>(
    context: &InferContext<'db, '_>,
    left: &TupleSpec<'db>,
    op: RichCompareOperator,
    right: &TupleSpec<'db>,
    range: TextRange,
    visitor: &BinaryComparisonVisitor<'db>,
) -> Result<Type<'db>, UnsupportedComparisonError<'db>> {
    let db = context.db();
    match (left, right) {
        // Both fixed-length: perform full lexicographic comparison.
        (TupleSpec::Fixed(left), TupleSpec::Fixed(right)) => {
            let left_iter = left.iter_all_elements();
            let right_iter = right.iter_all_elements();

            let mut builder = UnionBuilder::new(db);

            for (l_ty, r_ty) in left_iter.zip(right_iter) {
                let pairwise_eq_result = infer_binary_type_comparison(
                    context,
                    l_ty,
                    ast::CmpOp::Eq,
                    r_ty,
                    range,
                    visitor,
                )
                .expect("infer_binary_type_comparison should never return None for `CmpOp::Eq`");

                match pairwise_eq_result.try_bool(db).unwrap_or_else(|err| {
                    // TODO: We should, whenever possible, pass the range of the left and right elements
                    //   instead of the range of the whole tuple.
                    err.report_diagnostic(context, range);
                    err.fallback_truthiness()
                }) {
                    // - AlwaysTrue : Continue to the next pair for lexicographic comparison
                    Truthiness::AlwaysTrue => continue,
                    // - AlwaysFalse:
                    // Lexicographic comparisons will always terminate with this pair.
                    // Complete the comparison and return the result.
                    // - Ambiguous:
                    // Lexicographic comparisons might continue to the next pair (if eq_result is true),
                    // or terminate here (if eq_result is false).
                    // To account for cases where the comparison terminates here, add the pairwise comparison result to the union builder.
                    eq_truthiness @ (Truthiness::AlwaysFalse | Truthiness::Ambiguous) => {
                        let pairwise_compare_result = match op {
                            RichCompareOperator::Lt
                            | RichCompareOperator::Le
                            | RichCompareOperator::Gt
                            | RichCompareOperator::Ge => infer_binary_type_comparison(
                                context,
                                l_ty,
                                op.into(),
                                r_ty,
                                range,
                                visitor,
                            )?,
                            // For `==` and `!=`, we already figure out the result from `pairwise_eq_result`
                            // NOTE: The CPython implementation does not account for non-boolean return types
                            // or cases where `!=` is not the negation of `==`, we also do not consider these cases.
                            RichCompareOperator::Eq => Type::bool_literal(false),
                            RichCompareOperator::Ne => Type::bool_literal(true),
                        };

                        builder = builder.add(pairwise_compare_result);

                        if eq_truthiness.is_ambiguous() {
                            continue;
                        }

                        return Ok(builder.build());
                    }
                }
            }

            // if no more items to compare, we just compare sizes
            let (left_len, right_len) = (left.len(), right.len());

            builder = builder.add(Type::bool_literal(match op {
                RichCompareOperator::Eq => left_len == right_len,
                RichCompareOperator::Ne => left_len != right_len,
                RichCompareOperator::Lt => left_len < right_len,
                RichCompareOperator::Le => left_len <= right_len,
                RichCompareOperator::Gt => left_len > right_len,
                RichCompareOperator::Ge => left_len >= right_len,
            }));

            Ok(builder.build())
        }

        // At least one tuple is variable-length. We can make no assumptions about
        // the relative lengths of the tuples, and therefore neither about how they
        // compare lexicographically. However, we still need to verify that the
        // element types are comparable for ordering comparisons.

        // For equality comparisons (==, !=), any two objects can be compared,
        // and tuple equality always returns bool regardless of element __eq__ return types.
        (TupleSpec::Variable(_), _) | (_, TupleSpec::Variable(_))
            if matches!(op, RichCompareOperator::Eq | RichCompareOperator::Ne) =>
        {
            Ok(KnownClass::Bool.to_instance(db))
        }

        // At least one variable-length: check all elements that could potentially be compared.
        // We use `try_for_each_element_pair` to iterate over all possible pairings.
        (left @ TupleSpec::Variable(_), right) | (left, right @ TupleSpec::Variable(_)) => {
            let mut results = SmallVec::<[Type<'db>; 8]>::new();
            left.try_for_each_element_pair(right, |l_ty, r_ty| {
                results.push(infer_binary_type_comparison(
                    context,
                    l_ty,
                    op.into(),
                    r_ty,
                    range,
                    visitor,
                )?);
                Ok::<_, UnsupportedComparisonError<'db>>(())
            })?;

            let mut builder = UnionBuilder::new(db);
            for result in results {
                builder = builder.add(result);
            }
            // Length comparison (when all elements are equal) returns bool.
            builder = builder.add(KnownClass::Bool.to_instance(db));

            Ok(builder.build())
        }
    }
}
