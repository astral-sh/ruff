use std::borrow::Cow;
use std::convert::Infallible;

use ruff_python_ast as ast;
use ruff_text_size::TextRange;
use ty_python_core::Truthiness;

use super::source::{
    ComparisonBranch, ComparisonFacts, ComparisonResult, SynchronousComparisonEffects,
    compare_inner_sync,
};
use super::{
    BinaryComparisonVisitor, IntersectionOn, MembershipOperator, NonIdentityOperator,
    RichCompareOperator, UnsupportedComparisonError, infer_binary_intersection_type_comparison,
    infer_binary_type_comparison_inner, infer_membership_test_comparison, infer_rich_comparison,
    infer_tuple_rich_comparison, source,
};
use crate::types::bool::BoolError;
use crate::types::constraints::ConstraintSetBuilder;
use crate::types::context::InferContext;
use crate::types::equality::{
    ComparisonSoundnessPolicy, TupleEqualityEvaluator, equality_truthiness, inequality_truthiness,
};
use crate::types::literal::{BytesLiteralType, IntLiteralType, StringLiteralType};
use crate::types::tuple::FixedLengthTuple;
use crate::types::tuple::TupleSpec;
use crate::types::typevar::BoundTypeVarInstance;
use crate::types::{
    IntersectionType, KnownClass, MemberLookupPolicy, Type, TypeVarBoundOrConstraints, UnionBuilder,
};

pub(super) struct OrdinaryComparisonEffects<'context, 'db, 'ast, 'visitor> {
    pub(super) context: &'context InferContext<'db, 'ast>,
    pub(super) range: TextRange,
    pub(super) visitor: &'visitor BinaryComparisonVisitor<'db>,
}

fn deferred_comparison<'db>(
    context: &InferContext<'db, '_>,
    left: Type<'db>,
    op: NonIdentityOperator,
    right: Type<'db>,
    range: TextRange,
    visitor: &BinaryComparisonVisitor<'db>,
    branch: ComparisonBranch<'db>,
) -> Option<ComparisonResult<'db>> {
    let db = context.db();
    let env = &context.program_environment();
    let try_dunder = |policy| match op {
        NonIdentityOperator::Rich(op) => infer_rich_comparison(context, left, right, op, policy),
        NonIdentityOperator::Membership(op) => {
            infer_membership_test_comparison(context, left, right, op, range)
        }
    };
    let result: Result<Option<ComparisonResult<'db>>, UnsupportedComparisonError<'db>> = (|| {
        Ok(match branch {
            ComparisonBranch::EnumComplementLeft(complement) => {
                Some(infer_binary_type_comparison_inner(
                    context,
                    complement.remaining_literal_union(db, env),
                    op,
                    right,
                    range,
                    visitor,
                ))
            }
            ComparisonBranch::EnumComplementRight(complement) => {
                Some(infer_binary_type_comparison_inner(
                    context,
                    left,
                    op,
                    complement.remaining_literal_union(db, env),
                    range,
                    visitor,
                ))
            }

            ComparisonBranch::UnionLeft(union) => {
                let other = right;
                let mut builder = UnionBuilder::new(db, env);
                for element in union.elements(db) {
                    builder = builder.add(infer_binary_type_comparison_inner(
                        context, *element, op, other, range, visitor,
                    )?);
                }
                Some(Ok(builder.build()))
            }
            ComparisonBranch::UnionRight(union) => {
                let other = left;
                let mut builder = UnionBuilder::new(db, env);
                for element in union.elements(db) {
                    builder = builder.add(infer_binary_type_comparison_inner(
                        context, other, op, *element, range, visitor,
                    )?);
                }
                Some(Ok(builder.build()))
            }

            ComparisonBranch::IntersectionExpandLeft(intersection) => {
                Some(infer_binary_type_comparison_inner(
                    context,
                    intersection.with_expanded_typevars_and_newtypes(db, env),
                    op,
                    right,
                    range,
                    visitor,
                ))
            }
            ComparisonBranch::IntersectionExpandRight(intersection) => {
                Some(infer_binary_type_comparison_inner(
                    context,
                    left,
                    op,
                    intersection.with_expanded_typevars_and_newtypes(db, env),
                    range,
                    visitor,
                ))
            }

            ComparisonBranch::IntersectionLeft(intersection) => Some(
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
            ComparisonBranch::IntersectionRight(intersection) => Some(
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

            ComparisonBranch::AliasLeft => Some(visitor.visit(db, (left, op, right), || {
                infer_binary_type_comparison_inner(
                    context,
                    left.resolve_type_alias(db),
                    op,
                    right,
                    range,
                    visitor,
                )
            })),

            ComparisonBranch::AliasRight => Some(visitor.visit(db, (left, op, right), || {
                infer_binary_type_comparison_inner(
                    context,
                    left,
                    op,
                    right.resolve_type_alias(db),
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
            ComparisonBranch::NewTypeLeft(newtype) => {
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
            ComparisonBranch::NewTypeRight(newtype) => {
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
            ComparisonBranch::SameTypeVar(left_tvar) => {
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
            ComparisonBranch::TypeVar(typevar) => {
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

            ComparisonBranch::ConstraintSets(left, right) => {
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
        })
    })();
    match result {
        Ok(result) => result,
        Err(error) => Some(Err(error)),
    }
}

impl<'db> SynchronousComparisonEffects<'db> for OrdinaryComparisonEffects<'_, 'db, '_, '_> {
    type Error = Infallible;

    fn checkpoint(&self) -> Result<(), Infallible> {
        Ok(())
    }
    fn identity(
        &self,
        left: Type<'db>,
        op: ast::CmpOp,
        right: Type<'db>,
    ) -> Result<Type<'db>, Infallible> {
        let db = self.context.db();
        let env = &self.context.program_environment();
        let truthiness = left
            .identity_comparison_truthiness(db, env, right)
            .negate_if(op == ast::CmpOp::IsNot);
        Ok(Type::from_truthiness(db, env, truthiness))
    }
    fn recurse(
        &self,
        left: Type<'db>,
        op: NonIdentityOperator,
        right: Type<'db>,
    ) -> Result<ComparisonResult<'db>, Infallible> {
        compare_inner_sync(left, op, right, ComparisonFacts, self)
    }
    fn policy(&self) -> Result<ComparisonSoundnessPolicy, Infallible> {
        Ok(ComparisonSoundnessPolicy::from_analysis_settings(
            self.context.db().analysis_settings(self.context.file()),
        ))
    }
    fn tuple_spec(&self, ty: Type<'db>) -> Result<Option<Cow<'db, TupleSpec<'db>>>, Infallible> {
        Ok(ty.tuple_instance_spec(self.context.db(), &self.context.program_environment()))
    }
    fn tuple_comparison(
        &self,
        left: Type<'db>,
        op: RichCompareOperator,
        right: Type<'db>,
        left_spec: &TupleSpec<'db>,
        right_spec: &TupleSpec<'db>,
    ) -> Result<ComparisonResult<'db>, Infallible> {
        Ok(self.visitor.visit(
            self.context.db(),
            (left, NonIdentityOperator::Rich(op), right),
            || {
                infer_tuple_rich_comparison(
                    self.context,
                    left_spec,
                    op,
                    right_spec,
                    self.range,
                    self.visitor,
                )
            },
        ))
    }
    fn membership(
        &self,
        left: Type<'db>,
        op: MembershipOperator,
        right: &FixedLengthTuple<Type<'db>>,
        policy: ComparisonSoundnessPolicy,
    ) -> Result<Type<'db>, Infallible> {
        let db = self.context.db();
        let env = &self.context.program_environment();
        let mut any_eq = false;
        let mut any_ambiguous = false;
        let mut equality = TupleEqualityEvaluator::new(db, env, policy);
        for &element in right.elements_slice() {
            // Membership combines alternatives without invoking their runtime `__bool__`.
            match equality
                .element_truthiness(element, left)
                .unwrap_or_else(|error| error.fallback_truthiness())
            {
                Truthiness::AlwaysTrue => any_eq = true,
                Truthiness::AlwaysFalse => (),
                Truthiness::Ambiguous => any_ambiguous = true,
            }
        }
        Ok(if any_eq {
            Type::bool_literal(op.is_in())
        } else if !any_ambiguous {
            Type::bool_literal(op.is_not_in())
        } else {
            KnownClass::Bool.to_instance(db, env)
        })
    }
    fn equality(
        &self,
        left: Type<'db>,
        op: RichCompareOperator,
        right: Type<'db>,
        policy: ComparisonSoundnessPolicy,
    ) -> Result<Truthiness, Infallible> {
        let db = self.context.db();
        let env = &self.context.program_environment();
        Ok(match op {
            RichCompareOperator::Eq => equality_truthiness(db, env, left, right, policy),
            RichCompareOperator::Ne => inequality_truthiness(db, env, left, right, policy),
            _ => Truthiness::Ambiguous,
        })
    }
    fn from_truthiness(&self, truthiness: Truthiness) -> Result<Type<'db>, Infallible> {
        Ok(Type::from_truthiness(
            self.context.db(),
            &self.context.program_environment(),
            truthiness,
        ))
    }
    fn intersection_has_typevar(
        &self,
        intersection: IntersectionType<'db>,
    ) -> Result<bool, Infallible> {
        Ok(intersection
            .positive(self.context.db())
            .iter()
            .copied()
            .any(Type::is_type_var))
    }
    fn same_typevar(
        &self,
        left: BoundTypeVarInstance<'db>,
        right: BoundTypeVarInstance<'db>,
    ) -> Result<bool, Infallible> {
        Ok(left.identity(self.context.db()) == right.identity(self.context.db()))
    }
    fn deferred(
        &self,
        branch: ComparisonBranch<'db>,
        left: Type<'db>,
        op: NonIdentityOperator,
        right: Type<'db>,
    ) -> Result<Option<ComparisonResult<'db>>, Infallible> {
        Ok(deferred_comparison(
            self.context,
            left,
            op,
            right,
            self.range,
            self.visitor,
            branch,
        ))
    }
    fn integer(
        &self,
        left: IntLiteralType,
        op: NonIdentityOperator,
        right: IntLiteralType,
        left_type: Type<'db>,
        right_type: Type<'db>,
    ) -> Result<ComparisonResult<'db>, Infallible> {
        Ok(source::integer_comparison(
            left, op, right, left_type, right_type,
        ))
    }
    fn string(
        &self,
        left: StringLiteralType<'db>,
        op: NonIdentityOperator,
        right: StringLiteralType<'db>,
    ) -> Result<Type<'db>, Infallible> {
        let left = left.value(self.context.db());
        let right = right.value(self.context.db());
        Ok(Type::bool_literal(match op {
            NonIdentityOperator::Rich(RichCompareOperator::Eq) => left == right,
            NonIdentityOperator::Rich(RichCompareOperator::Ne) => left != right,
            NonIdentityOperator::Rich(RichCompareOperator::Lt) => left < right,
            NonIdentityOperator::Rich(RichCompareOperator::Le) => left <= right,
            NonIdentityOperator::Rich(RichCompareOperator::Gt) => left > right,
            NonIdentityOperator::Rich(RichCompareOperator::Ge) => left >= right,
            NonIdentityOperator::Membership(MembershipOperator::In) => right.contains(left),
            NonIdentityOperator::Membership(MembershipOperator::NotIn) => !right.contains(left),
        }))
    }
    fn bytes(
        &self,
        left: BytesLiteralType<'db>,
        op: NonIdentityOperator,
        right: BytesLiteralType<'db>,
    ) -> Result<Type<'db>, Infallible> {
        let left = left.value(self.context.db());
        let right = right.value(self.context.db());
        Ok(Type::bool_literal(match op {
            NonIdentityOperator::Rich(RichCompareOperator::Eq) => left == right,
            NonIdentityOperator::Rich(RichCompareOperator::Ne) => left != right,
            NonIdentityOperator::Rich(RichCompareOperator::Lt) => left < right,
            NonIdentityOperator::Rich(RichCompareOperator::Le) => left <= right,
            NonIdentityOperator::Rich(RichCompareOperator::Gt) => left > right,
            NonIdentityOperator::Rich(RichCompareOperator::Ge) => left >= right,
            NonIdentityOperator::Membership(MembershipOperator::In) => {
                memchr::memmem::find(right, left).is_some()
            }
            NonIdentityOperator::Membership(MembershipOperator::NotIn) => {
                memchr::memmem::find(right, left).is_none()
            }
        }))
    }
    fn dunder(
        &self,
        left: Type<'db>,
        op: NonIdentityOperator,
        right: Type<'db>,
    ) -> Result<ComparisonResult<'db>, Infallible> {
        Ok(match op {
            NonIdentityOperator::Rich(op) => {
                infer_rich_comparison(self.context, left, right, op, MemberLookupPolicy::default())
            }
            NonIdentityOperator::Membership(op) => {
                infer_membership_test_comparison(self.context, left, right, op, self.range)
            }
        })
    }
    fn new_union(&self) -> Result<UnionBuilder<'db>, Infallible> {
        Ok(UnionBuilder::new(
            self.context.db(),
            &self.context.program_environment(),
        ))
    }
    fn union_add(&self, builder: &mut UnionBuilder<'db>, ty: Type<'db>) -> Result<(), Infallible> {
        builder.add_in_place(ty);
        Ok(())
    }
    fn union_build(&self, builder: UnionBuilder<'db>) -> Result<Type<'db>, Infallible> {
        Ok(builder.build())
    }
    fn new_equality(
        &self,
        policy: ComparisonSoundnessPolicy,
    ) -> Result<TupleEqualityEvaluator<'db>, Infallible> {
        Ok(TupleEqualityEvaluator::new(
            self.context.db(),
            &self.context.program_environment(),
            policy,
        ))
    }
    fn element_equality(
        &self,
        evaluator: &mut TupleEqualityEvaluator<'db>,
        left: Type<'db>,
        right: Type<'db>,
    ) -> Result<Result<Truthiness, BoolError<'db>>, Infallible> {
        Ok(evaluator.element_truthiness(left, right))
    }
    fn retire_equality(&self, evaluator: TupleEqualityEvaluator<'db>) -> Result<(), Infallible> {
        drop(evaluator);
        Ok(())
    }
    fn report_equality(&self, error: &BoolError<'db>) -> Result<(), Infallible> {
        // The whole comparison range is used until element ranges are available here.
        error.report_diagnostic(self.context, self.range);
        Ok(())
    }
    fn pairs<'tuple>(
        &self,
        left: &'tuple FixedLengthTuple<Type<'db>>,
        right: &'tuple FixedLengthTuple<Type<'db>>,
    ) -> Result<source::FixedTuplePairs<'tuple, 'db>, Infallible> {
        Ok(source::tuple_pairs(left, right))
    }
    fn next_pair(
        &self,
        pairs: &mut source::FixedTuplePairs<'_, 'db>,
    ) -> Result<Option<(Type<'db>, Type<'db>)>, Infallible> {
        Ok(pairs.next())
    }
    fn variable_tuple(
        &self,
        left: &TupleSpec<'db>,
        op: RichCompareOperator,
        right: &TupleSpec<'db>,
    ) -> Result<ComparisonResult<'db>, Infallible> {
        let db = self.context.db();
        let env = &self.context.program_environment();
        let mut results = smallvec::SmallVec::<[Type<'db>; 8]>::new();
        let pairs = left.try_for_each_element_pair(db, right, |left, right| {
            results.push(infer_binary_type_comparison_inner(
                self.context,
                left,
                NonIdentityOperator::Rich(op),
                right,
                self.range,
                self.visitor,
            )?);
            Ok::<_, UnsupportedComparisonError<'db>>(())
        });
        if let Err(error) = pairs {
            return Ok(Err(error));
        }
        let mut builder = UnionBuilder::new(db, env);
        for result in results {
            builder = builder.add(result);
        }
        builder = builder.add(KnownClass::Bool.to_instance(db, env));
        Ok(Ok(builder.build()))
    }
    fn boolean_type(&self) -> Result<Type<'db>, Infallible> {
        Ok(KnownClass::Bool.to_instance(self.context.db(), &self.context.program_environment()))
    }
}
