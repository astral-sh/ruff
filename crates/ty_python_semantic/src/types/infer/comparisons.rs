use crate::Db;
use ruff_python_ast as ast;
use ruff_text_size::TextRange;
use smallvec::SmallVec;

use crate::ProgramEnvironment;
use crate::types::call::{CallArguments, CallDunderError};
use crate::types::constraints::ConstraintSetBuilder;
use crate::types::context::InferContext;
use crate::types::cyclic::CycleDetector;
use crate::types::equality::{
    ComparisonSoundnessPolicy, TupleEqualityEvaluator, equality_truthiness, inequality_truthiness,
};
use crate::types::tuple::{Tuple, TupleSpec};
use crate::types::{
    DynamicType, IntersectionBuilder, IntersectionType, KnownClass, KnownInstanceType,
    LiteralValueType, LiteralValueTypeKind, MemberLookupPolicy, Type, TypeContext, TypeTransformer,
    TypeVarBoundOrConstraints, UnionBuilder,
};
use ty_python_core::Truthiness;

impl<'db> Type<'db> {
    /// Upcast `self` to a type that conservatively describes its possible runtime objects in an
    /// identity comparison.
    ///
    /// A `NewType` constructor returns its argument unchanged, so its tag can differ between two
    /// views of the same object: upcast a `NewType` to its concrete base. In contrast, preserve
    /// invariant generic arguments because the same mutable object cannot satisfy incompatible
    /// commitments such as `list[int]` and `list[str]` without some other code already being
    /// unsound.
    ///
    /// Preserve negations that constrain the object itself, such as `~None`, `~SomeClass`, and
    /// `~Literal[1]`. A `NewType` tag, type-variable selection, or type-guard proof can differ
    /// between views. Retain the existing conservative handling of negated string types.
    ///
    /// A type variable can also hide a `NewType` tag: even a variable bounded by `int` can be
    /// instantiated as an integer `NewType`. Expand variables to their upcast bounds or constraints
    /// instead of transferring that potentially tagged relationship; an unbounded variable becomes
    /// `object`.
    ///
    /// Use this upcast both to decide whether identity is possible and to narrow the other
    /// operand when it succeeds. Each operand retains its own existing tags and type-variable
    /// relationships when the resulting constraint is applied.
    pub(crate) fn identity_comparison_type(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
    ) -> Type<'db> {
        struct IdentityComparisonUpcasting;

        fn upcast<'db>(
            db: &'db dyn Db,
            env: &ProgramEnvironment<'db>,
            ty: Type<'db>,
            visitor: &TypeTransformer<'db, IdentityComparisonUpcasting>,
        ) -> Type<'db> {
            match ty {
                Type::TypeAlias(alias) => {
                    visitor.visit_type(db, ty, || upcast(db, env, alias.value_type(db), visitor))
                }
                Type::NewTypeInstance(newtype) => {
                    upcast(db, env, newtype.concrete_base_type(db), visitor)
                }
                Type::TypeVar(typevar) => visitor.visit_type(db, ty, || {
                    match typevar.typevar(db).bound_or_constraints(db, env) {
                        Some(bound_or_constraints) => {
                            upcast(db, env, bound_or_constraints.as_type(db, env), visitor)
                        }
                        None => KnownClass::Object.to_instance(db, env),
                    }
                }),
                Type::Union(union) => {
                    union.map(db, env, |element| upcast(db, env, *element, visitor))
                }
                Type::Intersection(intersection) => {
                    let mut builder = IntersectionBuilder::new(db, env);
                    for element in intersection.positive(db) {
                        builder = builder.add_positive(upcast(db, env, *element, visitor));
                    }
                    for element in intersection.negative(db) {
                        // Static tags and predicate proofs can differ between views. Retain the
                        // existing conservative handling of negated string types.
                        match element.resolve_type_alias(db) {
                            Type::NewTypeInstance(_)
                            | Type::TypeVar(_)
                            | Type::TypeIs(_)
                            | Type::TypeGuard(_) => continue,
                            Type::LiteralValue(literal)
                                if literal.is_literal_string() || literal.is_string() =>
                            {
                                continue;
                            }
                            _ => builder.add_negative_in_place(*element),
                        }
                    }
                    builder.build()
                }
                _ => ty,
            }
        }

        upcast(
            db,
            env,
            self,
            &TypeTransformer::<IdentityComparisonUpcasting>::default(),
        )
    }

    /// Return whether values of these types always, never, or possibly identify the same object.
    pub(crate) fn identity_comparison_truthiness(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        other: Type<'db>,
    ) -> Truthiness {
        let is_singleton_or_intersection_with_singleton = |ty: Type<'db>| {
            ty.is_singleton(db, env)
                || ty
                    .resolve_type_alias(db)
                    .as_intersection()
                    .is_some_and(|intersection| {
                        intersection
                            .positive(db)
                            .iter()
                            .any(|ty| ty.is_singleton(db, env))
                    })
        };

        // Two occurrences of the same constrained `TypeVar` require separate handling. Although
        // different specializations can choose different singleton constraints, every occurrence in
        // one specialization shares the same selected constraint and therefore the same object.
        if let Type::TypeVar(left) = self.resolve_type_alias(db)
            && let Type::TypeVar(right) = other.resolve_type_alias(db)
            && left.is_same_typevar_as(db, right)
            && is_singleton_or_intersection_with_singleton(Type::TypeVar(left))
        {
            return Truthiness::AlwaysTrue;
        }

        // `NewType` instances are identity functions at runtime, so distinct static types can still
        // identify the same object. Compare the types of their possible runtime objects instead.
        let left_identity = self.identity_comparison_type(db, env);
        let right_identity = other.identity_comparison_type(db, env);

        // Non-disjoint singleton types do not necessarily identify the same object: disjointness can
        // be inconclusive, for example when aliases between enum members cannot be determined.
        // Require one singleton type to be a subtype of the other before concluding that they are
        // definitely identical.
        if left_identity.is_disjoint_from(db, env, right_identity) {
            Truthiness::AlwaysFalse
        } else if is_singleton_or_intersection_with_singleton(left_identity)
            && is_singleton_or_intersection_with_singleton(right_identity)
            && (left_identity.is_subtype_of(db, env, right_identity)
                || right_identity.is_subtype_of(db, env, left_identity))
        {
            Truthiness::AlwaysTrue
        } else {
            Truthiness::Ambiguous
        }
    }
}

/// Whether the intersection type is on the left or right side of the comparison.
#[derive(Debug, Clone, Copy)]
enum IntersectionOn {
    Left,
    Right,
}

/// A [`CycleDetector`] that is used in [`infer_binary_type_comparison`].
type BinaryComparisonVisitor<'db> = CycleDetector<
    'db,
    ast::CmpOp,
    (Type<'db>, NonIdentityOperator, Type<'db>),
    Result<Type<'db>, UnsupportedComparisonError<'db>>,
    1,
>;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum MembershipOperator {
    In,
    NotIn,
}

impl MembershipOperator {
    const fn is_in(self) -> bool {
        matches!(self, MembershipOperator::In)
    }

    const fn is_not_in(self) -> bool {
        matches!(self, MembershipOperator::NotIn)
    }
}

impl From<MembershipOperator> for ast::CmpOp {
    fn from(value: MembershipOperator) -> Self {
        match value {
            MembershipOperator::In => ast::CmpOp::In,
            MembershipOperator::NotIn => ast::CmpOp::NotIn,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum NonIdentityOperator {
    Rich(RichCompareOperator),
    Membership(MembershipOperator),
}

impl From<NonIdentityOperator> for ast::CmpOp {
    fn from(value: NonIdentityOperator) -> Self {
        match value {
            NonIdentityOperator::Rich(rich_op) => rich_op.into(),
            NonIdentityOperator::Membership(membership_op) => membership_op.into(),
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
) -> Result<Type<'db>, UnsupportedComparisonError<'db>> {
    let db = context.db();
    let env = &context.program_environment();

    let op = match op {
        ast::CmpOp::Is | ast::CmpOp::IsNot => {
            let truthiness = left
                .identity_comparison_truthiness(db, env, right)
                .negate_if(op == ast::CmpOp::IsNot);
            return Ok(Type::from_truthiness(db, env, truthiness));
        }
        ast::CmpOp::Eq => NonIdentityOperator::Rich(RichCompareOperator::Eq),
        ast::CmpOp::NotEq => NonIdentityOperator::Rich(RichCompareOperator::Ne),
        ast::CmpOp::Lt => NonIdentityOperator::Rich(RichCompareOperator::Lt),
        ast::CmpOp::LtE => NonIdentityOperator::Rich(RichCompareOperator::Le),
        ast::CmpOp::Gt => NonIdentityOperator::Rich(RichCompareOperator::Gt),
        ast::CmpOp::GtE => NonIdentityOperator::Rich(RichCompareOperator::Ge),
        ast::CmpOp::In => NonIdentityOperator::Membership(MembershipOperator::In),
        ast::CmpOp::NotIn => NonIdentityOperator::Membership(MembershipOperator::NotIn),
    };

    infer_binary_type_comparison_inner(
        context,
        left,
        op,
        right,
        range,
        &BinaryComparisonVisitor::new(Ok(Type::bool_literal(true))),
    )
}

fn infer_binary_type_comparison_inner<'db>(
    context: &InferContext<'db, '_>,
    left: Type<'db>,
    op: NonIdentityOperator,
    right: Type<'db>,
    range: TextRange,
    visitor: &BinaryComparisonVisitor<'db>,
) -> Result<Type<'db>, UnsupportedComparisonError<'db>> {
    let db = context.db();
    let env = &context.program_environment();

    let try_dunder = |policy: MemberLookupPolicy| {
        let rich_comparison = |op| infer_rich_comparison(context, left, right, op, policy);
        let membership_test_comparison = |op, range: TextRange| {
            infer_membership_test_comparison(context, left, right, op, range)
        };

        match op {
            NonIdentityOperator::Rich(rich_op) => rich_comparison(rich_op),
            NonIdentityOperator::Membership(membership_op) => {
                membership_test_comparison(membership_op, range)
            }
        }
    };

    let soundness_policy =
        ComparisonSoundnessPolicy::from_analysis_settings(db.analysis_settings(context.file()));

    if let NonIdentityOperator::Rich(rich_op) = op
        && let Some(left_tuple) = left.tuple_instance_spec(db, env)
        && let Some(right_tuple) = right.tuple_instance_spec(db, env)
    {
        return visitor.visit(db, (left, op, right), || {
            infer_tuple_rich_comparison(context, &left_tuple, rich_op, &right_tuple, range, visitor)
        });
    }

    if let NonIdentityOperator::Membership(op) = op
        && let Some(right_tuple) = right.tuple_instance_spec(db, env)
        && let Tuple::Fixed(right_tuple) = &*right_tuple
    {
        let mut any_eq = false;
        let mut any_ambiguous = false;
        let mut equality = TupleEqualityEvaluator::new(db, env, soundness_policy);

        for &element_ty in right_tuple.elements_slice() {
            // It's okay to ignore errors here because Python doesn't call `__bool__`
            // for different union variants. Instead, this is just for us to
            // evaluate a possibly truthy value to `false` or `true`.
            match equality
                .element_truthiness(element_ty, left)
                .unwrap_or_else(|error| error.fallback_truthiness())
            {
                Truthiness::AlwaysTrue => any_eq = true,
                Truthiness::AlwaysFalse => (),
                Truthiness::Ambiguous => any_ambiguous = true,
            }
        }

        return Ok(if any_eq {
            Type::bool_literal(op.is_in())
        } else if !any_ambiguous {
            Type::bool_literal(op.is_not_in())
        } else {
            KnownClass::Bool.to_instance(db, env)
        });
    }

    let comparison_truthiness = match op {
        NonIdentityOperator::Rich(RichCompareOperator::Eq) => {
            equality_truthiness(db, env, left, right, soundness_policy)
        }
        NonIdentityOperator::Rich(RichCompareOperator::Ne) => {
            inequality_truthiness(db, env, left, right, soundness_policy)
        }
        _ => Truthiness::Ambiguous,
    };
    if comparison_truthiness != Truthiness::Ambiguous {
        return Ok(Type::from_truthiness(db, env, comparison_truthiness));
    }

    let comparison_result = match (left, right) {
        (Type::EnumComplement(complement), right) => Some(infer_binary_type_comparison_inner(
            context,
            complement.remaining_literal_union(db, env),
            op,
            right,
            range,
            visitor,
        )),
        (left, Type::EnumComplement(complement)) => Some(infer_binary_type_comparison_inner(
            context,
            left,
            op,
            complement.remaining_literal_union(db, env),
            range,
            visitor,
        )),

        (Type::Union(union), other) => {
            let mut builder = UnionBuilder::new(db, env);
            for element in union.elements(db) {
                builder = builder.add(infer_binary_type_comparison_inner(
                    context, *element, op, other, range, visitor,
                )?);
            }
            Some(Ok(builder.build()))
        }
        (other, Type::Union(union)) => {
            let mut builder = UnionBuilder::new(db, env);
            for element in union.elements(db) {
                builder = builder.add(infer_binary_type_comparison_inner(
                    context, other, op, *element, range, visitor,
                )?);
            }
            Some(Ok(builder.build()))
        }

        (Type::Intersection(intersection), right)
            if intersection
                .positive(db)
                .iter()
                .copied()
                .any(Type::is_type_var) =>
        {
            Some(infer_binary_type_comparison_inner(
                context,
                intersection.with_expanded_typevars_and_newtypes(db, env),
                op,
                right,
                range,
                visitor,
            ))
        }
        (left, Type::Intersection(intersection))
            if intersection
                .positive(db)
                .iter()
                .copied()
                .any(Type::is_type_var) =>
        {
            Some(infer_binary_type_comparison_inner(
                context,
                left,
                op,
                intersection.with_expanded_typevars_and_newtypes(db, env),
                range,
                visitor,
            ))
        }

        (Type::Intersection(intersection), right) => Some(
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
                op: op.into(),
                left_ty: left,
                right_ty: err.right_ty,
            }),
        ),
        (left, Type::Intersection(intersection)) => Some(
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
                op: op.into(),
                left_ty: err.left_ty,
                right_ty: right,
            }),
        ),

        (Type::TypeAlias(alias), right) => Some(visitor.visit(db, (left, op, right), || {
            infer_binary_type_comparison_inner(
                context,
                alias.value_type(db),
                op,
                right,
                range,
                visitor,
            )
        })),

        (left, Type::TypeAlias(alias)) => Some(visitor.visit(db, (left, op, right), || {
            infer_binary_type_comparison_inner(
                context,
                left,
                op,
                alias.value_type(db),
                range,
                visitor,
            )
        })),

        // `try_dunder` works for almost all `NewType`s, but not for `NewType`s of `float` and
        // `complex`, where the concrete base type is a union. In that case it turns out the
        // `self` types of the dunder methods in typeshed don't match, because they don't get
        // the same `int | float` and `int | float | complex` special treatment that the
        // positional arguments get. In those cases we need to explicitly delegate to the base
        // type, so that it hits the `Type::Union` branches above.
        (Type::NewTypeInstance(newtype), right) => {
            Some(try_dunder(MemberLookupPolicy::default()).or_else(|_| {
                visitor.visit(db, (left, op, right), || {
                    infer_binary_type_comparison_inner(
                        context,
                        newtype.concrete_base_type(db),
                        op,
                        right,
                        range,
                        visitor,
                    )
                })
            }))
        }
        (left, Type::NewTypeInstance(newtype)) => {
            Some(try_dunder(MemberLookupPolicy::default()).or_else(|_| {
                visitor.visit(db, (left, op, right), || {
                    infer_binary_type_comparison_inner(
                        context,
                        left,
                        op,
                        newtype.concrete_base_type(db),
                        range,
                        visitor,
                    )
                })
            }))
        }

        // Similar to `NewType`s, `TypeVar`s with union bounds (like `bound=float` which becomes
        // `int | float`) need to delegate to the bound type.
        //
        // When both operands are the same bounded TypeVar, we check the comparison on the bound
        // type paired with itself.
        (Type::TypeVar(left_tvar), Type::TypeVar(right_tvar))
            if left_tvar.identity(db) == right_tvar.identity(db) =>
        {
            match left_tvar.typevar(db).bound_or_constraints(db, env) {
                Some(TypeVarBoundOrConstraints::UpperBound(bound)) => {
                    Some(try_dunder(MemberLookupPolicy::default()).or_else(|_| {
                        visitor.visit(db, (left, op, right), || {
                            infer_binary_type_comparison_inner(
                                context, bound, op, bound, range, visitor,
                            )
                        })
                    }))
                }
                Some(TypeVarBoundOrConstraints::Constraints(constraints)) => {
                    // For constrained TypeVars, check each constraint paired with itself.
                    let mut builder = UnionBuilder::new(db, env);
                    for &constraint in constraints.elements(db) {
                        builder = builder.add(infer_binary_type_comparison_inner(
                            context, constraint, op, constraint, range, visitor,
                        )?);
                    }
                    Some(Ok(builder.build()))
                }
                None => None, // Fall through to default handling
            }
        }
        // A bounded or constrained TypeVar on either side delegates to its concrete alternatives.
        (Type::TypeVar(typevar), other) | (other, Type::TypeVar(typevar))
            if !other.is_type_var() =>
        {
            let compare_replacement = |replacement| {
                let (left, right) = if left.is_type_var() {
                    (replacement, right)
                } else {
                    (left, replacement)
                };
                infer_binary_type_comparison_inner(context, left, op, right, range, visitor)
            };

            match typevar.typevar(db).bound_or_constraints(db, env) {
                Some(TypeVarBoundOrConstraints::UpperBound(bound)) => {
                    Some(try_dunder(MemberLookupPolicy::default()).or_else(|_| {
                        visitor.visit(db, (left, op, right), || compare_replacement(bound))
                    }))
                }
                Some(TypeVarBoundOrConstraints::Constraints(constraints)) => {
                    let mut builder = UnionBuilder::new(db, env);
                    for &constraint in constraints.elements(db) {
                        builder = builder.add(compare_replacement(constraint)?);
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
                        NonIdentityOperator::Rich(RichCompareOperator::Eq) => {
                            Ok(Type::bool_literal(n == m))
                        }
                        NonIdentityOperator::Rich(RichCompareOperator::Ne) => {
                            Ok(Type::bool_literal(n != m))
                        }
                        NonIdentityOperator::Rich(RichCompareOperator::Lt) => {
                            Ok(Type::bool_literal(n < m))
                        }
                        NonIdentityOperator::Rich(RichCompareOperator::Le) => {
                            Ok(Type::bool_literal(n <= m))
                        }
                        NonIdentityOperator::Rich(RichCompareOperator::Gt) => {
                            Ok(Type::bool_literal(n > m))
                        }
                        NonIdentityOperator::Rich(RichCompareOperator::Ge) => {
                            Ok(Type::bool_literal(n >= m))
                        }
                        // Undefined for (int, int)
                        NonIdentityOperator::Membership(_) => Err(UnsupportedComparisonError {
                            op: op.into(),
                            left_ty: left,
                            right_ty: right,
                        }),
                    })
                }
                // Booleans are coded as integers (False = 0, True = 1)
                (LiteralValueTypeKind::Int(n), LiteralValueTypeKind::Bool(b)) => Some(
                    infer_binary_type_comparison_inner(
                        context,
                        Type::int_literal(n.as_i64()),
                        op,
                        Type::int_literal(i64::from(b)),
                        range,
                        visitor,
                    )
                    .map_err(|_| UnsupportedComparisonError {
                        op: op.into(),
                        left_ty: left,
                        right_ty: right,
                    }),
                ),
                (LiteralValueTypeKind::Bool(b), LiteralValueTypeKind::Int(m)) => Some(
                    infer_binary_type_comparison_inner(
                        context,
                        Type::int_literal(i64::from(b)),
                        op,
                        Type::int_literal(m.as_i64()),
                        range,
                        visitor,
                    )
                    .map_err(|_| UnsupportedComparisonError {
                        op: op.into(),
                        left_ty: left,
                        right_ty: right,
                    }),
                ),
                (LiteralValueTypeKind::Bool(a), LiteralValueTypeKind::Bool(b)) => Some(
                    infer_binary_type_comparison_inner(
                        context,
                        Type::int_literal(i64::from(a)),
                        op,
                        Type::int_literal(i64::from(b)),
                        range,
                        visitor,
                    )
                    .map_err(|_| UnsupportedComparisonError {
                        op: op.into(),
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
                        NonIdentityOperator::Rich(RichCompareOperator::Eq) => {
                            Type::bool_literal(s1 == s2)
                        }
                        NonIdentityOperator::Rich(RichCompareOperator::Ne) => {
                            Type::bool_literal(s1 != s2)
                        }
                        NonIdentityOperator::Rich(RichCompareOperator::Lt) => {
                            Type::bool_literal(s1 < s2)
                        }
                        NonIdentityOperator::Rich(RichCompareOperator::Le) => {
                            Type::bool_literal(s1 <= s2)
                        }
                        NonIdentityOperator::Rich(RichCompareOperator::Gt) => {
                            Type::bool_literal(s1 > s2)
                        }
                        NonIdentityOperator::Rich(RichCompareOperator::Ge) => {
                            Type::bool_literal(s1 >= s2)
                        }
                        NonIdentityOperator::Membership(MembershipOperator::In) => {
                            Type::bool_literal(s2.contains(s1))
                        }
                        NonIdentityOperator::Membership(MembershipOperator::NotIn) => {
                            Type::bool_literal(!s2.contains(s1))
                        }
                    };
                    Some(Ok(result))
                }

                (LiteralValueTypeKind::Bytes(salsa_b1), LiteralValueTypeKind::Bytes(salsa_b2)) => {
                    let b1 = salsa_b1.value(db);
                    let b2 = salsa_b2.value(db);
                    let result = match op {
                        NonIdentityOperator::Rich(RichCompareOperator::Eq) => {
                            Type::bool_literal(b1 == b2)
                        }
                        NonIdentityOperator::Rich(RichCompareOperator::Ne) => {
                            Type::bool_literal(b1 != b2)
                        }
                        NonIdentityOperator::Rich(RichCompareOperator::Lt) => {
                            Type::bool_literal(b1 < b2)
                        }
                        NonIdentityOperator::Rich(RichCompareOperator::Le) => {
                            Type::bool_literal(b1 <= b2)
                        }
                        NonIdentityOperator::Rich(RichCompareOperator::Gt) => {
                            Type::bool_literal(b1 > b2)
                        }
                        NonIdentityOperator::Rich(RichCompareOperator::Ge) => {
                            Type::bool_literal(b1 >= b2)
                        }
                        NonIdentityOperator::Membership(MembershipOperator::In) => {
                            Type::bool_literal(memchr::memmem::find(b2, b1).is_some())
                        }
                        NonIdentityOperator::Membership(MembershipOperator::NotIn) => {
                            Type::bool_literal(memchr::memmem::find(b2, b1).is_none())
                        }
                    };
                    Some(Ok(result))
                }

                // Same-kind exact literals and the special relationship between `int` and `bool`
                // are handled above. Any remaining pair of exact builtin literals compares
                // unequal. `LiteralString` also compares unequal to non-string literals, but its
                // comparison with an exact string literal remains ambiguous.
                (
                    LiteralValueTypeKind::Int(_)
                    | LiteralValueTypeKind::Bool(_)
                    | LiteralValueTypeKind::String(_)
                    | LiteralValueTypeKind::Bytes(_),
                    LiteralValueTypeKind::Int(_)
                    | LiteralValueTypeKind::Bool(_)
                    | LiteralValueTypeKind::String(_)
                    | LiteralValueTypeKind::Bytes(_),
                )
                | (
                    LiteralValueTypeKind::LiteralString,
                    LiteralValueTypeKind::Int(_)
                    | LiteralValueTypeKind::Bool(_)
                    | LiteralValueTypeKind::Bytes(_),
                )
                | (
                    LiteralValueTypeKind::Int(_)
                    | LiteralValueTypeKind::Bool(_)
                    | LiteralValueTypeKind::Bytes(_),
                    LiteralValueTypeKind::LiteralString,
                ) if let NonIdentityOperator::Rich(
                    rich @ (RichCompareOperator::Eq | RichCompareOperator::Ne),
                ) = op =>
                {
                    Some(Ok(Type::bool_literal(rich == RichCompareOperator::Ne)))
                }
                _ => None,
            }
        }

        (
            Type::KnownInstance(KnownInstanceType::ConstraintSet(left)),
            Type::KnownInstance(KnownInstanceType::ConstraintSet(right)),
        ) => {
            let constraints = ConstraintSetBuilder::new();
            let left = constraints.load(db, env, left.constraints(db));
            let right = constraints.load(db, env, right.constraints(db));
            let equivalent = left
                .iff(db, &constraints, right)
                .is_always_satisfied(db, env);
            match op {
                NonIdentityOperator::Rich(RichCompareOperator::Eq) => {
                    Some(Ok(Type::bool_literal(equivalent)))
                }
                NonIdentityOperator::Rich(RichCompareOperator::Ne) => {
                    Some(Ok(Type::bool_literal(!equivalent)))
                }
                _ => None,
            }
        }

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
    op: NonIdentityOperator,
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
    let env = &context.program_environment();

    if let Some(alternatives) = intersection.finite_alternative_union(db, env) {
        return match intersection_on {
            IntersectionOn::Left => {
                infer_binary_type_comparison_inner(context, alternatives, op, other, range, visitor)
            }
            IntersectionOn::Right => {
                infer_binary_type_comparison_inner(context, other, op, alternatives, range, visitor)
            }
        };
    }

    // If a comparison yields a definitive true/false answer on a (positive) part
    // of an intersection type, it will also yield a definitive answer on the full
    // intersection type, which is even more specific.
    for pos in intersection.positive(db) {
        let result = match intersection_on {
            IntersectionOn::Left => {
                infer_binary_type_comparison_inner(context, *pos, op, other, range, visitor)
            }
            IntersectionOn::Right => {
                infer_binary_type_comparison_inner(context, other, op, *pos, range, visitor)
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
    let mut builder = IntersectionBuilder::new(db, env);

    builder.add_positive_in_place(KnownClass::Bool.to_instance(db, env));

    let mut state = State::NoPositiveElements;

    for pos in intersection.positive(db) {
        let result = match intersection_on {
            IntersectionOn::Left => {
                infer_binary_type_comparison_inner(context, *pos, op, other, range, visitor)
            }
            IntersectionOn::Right => {
                infer_binary_type_comparison_inner(context, other, op, *pos, range, visitor)
            }
        };

        match result {
            Ok(ty) => {
                state = State::Supported;
                builder.add_positive_in_place(ty);
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
                IntersectionOn::Left => infer_binary_type_comparison_inner(
                    context,
                    Type::object(),
                    op,
                    other,
                    range,
                    visitor,
                ),
                IntersectionOn::Right => infer_binary_type_comparison_inner(
                    context,
                    other,
                    op,
                    Type::object(),
                    range,
                    visitor,
                ),
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
    context: &InferContext<'db, '_>,
    left: Type<'db>,
    right: Type<'db>,
    op: RichCompareOperator,
    policy: MemberLookupPolicy,
) -> Result<Type<'db>, UnsupportedComparisonError<'db>> {
    let db = context.db();
    let env = &context.program_environment();
    Type::try_call_rich_comparison_dunder(
        db,
        env,
        left,
        right,
        op.dunder(),
        op.reflect().dunder(),
        policy,
    )
    .or_else(|| {
        // When no appropriate method returns any value other than NotImplemented,
        // the `==` and `!=` operators will fall back to `is` and `is not`, respectively.
        // refer to `<https://docs.python.org/3/reference/datamodel.html#object.__eq__>`
        if matches!(op, RichCompareOperator::Eq | RichCompareOperator::Ne)
            // This branch implements specific behavior of the `__eq__` and `__ne__` methods
            // on `object`, so it does not apply if we skip looking up attributes on `object`.
            && !policy.mro_no_object_fallback()
        {
            Some(KnownClass::Bool.to_instance(db, env))
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
    op: MembershipOperator,
    range: TextRange,
) -> Result<Type<'db>, UnsupportedComparisonError<'db>> {
    let db = context.db();
    let env = &context.program_environment();

    if let Some(key) = left.as_string_literal()
        && let Some(typed_dict) = right.as_typed_dict()
    {
        let truthiness = typed_dict
            .key_membership_truthiness(db, key.value(db))
            .negate_if(op.is_not_in());
        return Ok(Type::from_truthiness(db, env, truthiness));
    }

    let compare_result_opt = match right.try_call_dunder(
        db,
        env,
        "__contains__",
        CallArguments::positional([left]),
        TypeContext::default(),
    ) {
        // If `__contains__` is available, it is used directly for the membership test.
        Ok(bindings) => Some(bindings.return_type(db, env)),
        // If `__contains__` is not available or possibly unbound,
        // fall back to iteration-based membership test.
        Err(CallDunderError::MethodNotAvailable | CallDunderError::PossiblyUnbound { .. }) => right
            .try_iterate(db, env)
            .map(|_| KnownClass::Bool.to_instance(db, env))
            .ok(),
        // `__contains__` exists but can't be called with the given arguments.
        Err(CallDunderError::CallError(..)) => None,
    };

    compare_result_opt
        .map(|ty| {
            if matches!(ty, Type::Dynamic(DynamicType::Todo(_))) {
                return ty;
            }

            let truthiness = ty.try_bool(db, env).unwrap_or_else(|err| {
                err.report_diagnostic(context, range);
                err.fallback_truthiness()
            });

            match op {
                MembershipOperator::In => Type::from_truthiness(db, env, truthiness),
                MembershipOperator::NotIn => Type::from_truthiness(db, env, truthiness.negate()),
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
    let env = &context.program_environment();
    match (left, right) {
        // Both fixed-length: perform full lexicographic comparison.
        (TupleSpec::Fixed(left), TupleSpec::Fixed(right)) => {
            let left_iter = left.iter_all_elements();
            let right_iter = right.iter_all_elements();

            let mut builder = UnionBuilder::new(db, env);
            let soundness_policy = ComparisonSoundnessPolicy::from_analysis_settings(
                db.analysis_settings(context.file()),
            );
            let mut equality = TupleEqualityEvaluator::new(db, env, soundness_policy);

            for (l_ty, r_ty) in left_iter.zip(right_iter) {
                let eq_truthiness = equality
                    .element_truthiness(l_ty, r_ty)
                    .unwrap_or_else(|err| {
                        // TODO: We should, whenever possible, pass the range of the left and right elements
                        //   instead of the range of the whole tuple.
                        err.report_diagnostic(context, range);
                        Truthiness::Ambiguous
                    });

                match eq_truthiness {
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
                            | RichCompareOperator::Ge => infer_binary_type_comparison_inner(
                                context,
                                l_ty,
                                NonIdentityOperator::Rich(op),
                                r_ty,
                                range,
                                visitor,
                            )?,
                            // For `==` and `!=`, the equality evaluator has already determined
                            // that these elements may differ.
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
            Ok(KnownClass::Bool.to_instance(db, env))
        }

        // At least one variable-length: check all elements that could potentially be compared.
        // We use `try_for_each_element_pair` to iterate over all possible pairings.
        (left @ TupleSpec::Variable(_), right) | (left, right @ TupleSpec::Variable(_)) => {
            let mut results = SmallVec::<[Type<'db>; 8]>::new();
            left.try_for_each_element_pair(db, right, |l_ty, r_ty| {
                results.push(infer_binary_type_comparison_inner(
                    context,
                    l_ty,
                    NonIdentityOperator::Rich(op),
                    r_ty,
                    range,
                    visitor,
                )?);
                Ok::<_, UnsupportedComparisonError<'db>>(())
            })?;

            let mut builder = UnionBuilder::new(db, env);
            for result in results {
                builder = builder.add(result);
            }
            // Length comparison (when all elements are equal) returns bool.
            builder = builder.add(KnownClass::Bool.to_instance(db, env));

            Ok(builder.build())
        }
    }
}
