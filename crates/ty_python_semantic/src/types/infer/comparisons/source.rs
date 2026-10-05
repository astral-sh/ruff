//! Comparison dispatch and tuple traversal share the ordinary evaluation order.

use std::borrow::Cow;

use ruff_python_ast as ast;
use ty_mapping_probe_macros::shared_semantic_family;
use ty_python_core::Truthiness;

use super::{
    MembershipOperator, NonIdentityOperator, RichCompareOperator, UnsupportedComparisonError,
};
use crate::types::bool::BoolError;
use crate::types::enums::EnumComplementType;
use crate::types::equality::{ComparisonSoundnessPolicy, TupleEqualityEvaluator};
use crate::types::known_instance::InternedConstraintSet;
use crate::types::literal::{BytesLiteralType, IntLiteralType, StringLiteralType};
use crate::types::newtype::NewType;
use crate::types::tuple::{FixedLengthTuple, TupleSpec};
use crate::types::typevar::BoundTypeVarInstance;
use crate::types::{
    IntersectionType, KnownInstanceType, LiteralValueType, LiteralValueTypeKind, Type,
    UnionBuilder, UnionType,
};

#[cfg(feature = "experimental-analysis")]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TypeComparisonOperation {
    Identity,
    Membership,
    Equality,
    EnumComplement,
    Union,
    IntersectionTypeVars,
    Intersection,
    Alias,
    NewType,
    TypeVarIdentity,
    TypeVar,
    StringLiteral,
    BytesLiteral,
    ConstraintSets,
    Dunder,
    VariableTuple,
    EqualityDiagnostic,
    VisitorIdentity,
}

pub(in crate::types) type ComparisonResult<'db> =
    Result<Type<'db>, UnsupportedComparisonError<'db>>;

pub(in crate::types) enum ComparisonBranch<'db> {
    EnumComplementLeft(EnumComplementType<'db>),
    EnumComplementRight(EnumComplementType<'db>),
    UnionLeft(UnionType<'db>),
    UnionRight(UnionType<'db>),
    IntersectionExpandLeft(IntersectionType<'db>),
    IntersectionExpandRight(IntersectionType<'db>),
    IntersectionLeft(IntersectionType<'db>),
    IntersectionRight(IntersectionType<'db>),
    AliasLeft,
    AliasRight,
    NewTypeLeft(NewType<'db>),
    NewTypeRight(NewType<'db>),
    SameTypeVar(BoundTypeVarInstance<'db>),
    TypeVar(BoundTypeVarInstance<'db>),
    ConstraintSets(InternedConstraintSet<'db>, InternedConstraintSet<'db>),
}

pub(in crate::types) type FixedTuplePairs<'tuple, 'db> = std::iter::Zip<
    std::iter::Copied<std::slice::Iter<'tuple, Type<'db>>>,
    std::iter::Copied<std::slice::Iter<'tuple, Type<'db>>>,
>;

pub(in crate::types) fn tuple_pairs<'tuple, 'db>(
    left: &'tuple FixedLengthTuple<Type<'db>>,
    right: &'tuple FixedLengthTuple<Type<'db>>,
) -> FixedTuplePairs<'tuple, 'db> {
    left.elements_slice()
        .iter()
        .copied()
        .zip(right.elements_slice().iter().copied())
}

pub(in crate::types) struct ComparisonFacts;

shared_semantic_family! {
    #[synchronous(SynchronousComparisonEffects)]
    pub(in crate::types) trait ComparisonEffects<'db> {
        type Error;
        #[operation(checkpoint)]
        async fn checkpoint(&self) -> Result<(), Self::Error>;
        #[operation(child)]
        async fn identity(&self, left: Type<'db>, op: ast::CmpOp, right: Type<'db>) -> Result<Type<'db>, Self::Error>;
        #[operation(child)]
        async fn recurse(&self, left: Type<'db>, op: NonIdentityOperator, right: Type<'db>) -> Result<ComparisonResult<'db>, Self::Error>;
        #[operation(local)]
        async fn policy(&self) -> Result<ComparisonSoundnessPolicy, Self::Error>;
        #[operation(child)]
        async fn tuple_spec(&self, ty: Type<'db>) -> Result<Option<Cow<'db, TupleSpec<'db>>>, Self::Error>;
        #[operation(child)]
        async fn tuple_comparison(&self, left: Type<'db>, op: RichCompareOperator, right: Type<'db>, left_spec: &TupleSpec<'db>, right_spec: &TupleSpec<'db>) -> Result<ComparisonResult<'db>, Self::Error>;
        #[operation(child)]
        async fn membership(&self, left: Type<'db>, op: MembershipOperator, right: &FixedLengthTuple<Type<'db>>, policy: ComparisonSoundnessPolicy) -> Result<Type<'db>, Self::Error>;
        #[operation(child)]
        async fn equality(&self, left: Type<'db>, op: RichCompareOperator, right: Type<'db>, policy: ComparisonSoundnessPolicy) -> Result<Truthiness, Self::Error>;
        #[operation(child)]
        async fn from_truthiness(&self, truthiness: Truthiness) -> Result<Type<'db>, Self::Error>;
        #[operation(local)]
        async fn intersection_has_typevar(&self, intersection: IntersectionType<'db>) -> Result<bool, Self::Error>;
        #[operation(local)]
        async fn same_typevar(&self, left: BoundTypeVarInstance<'db>, right: BoundTypeVarInstance<'db>) -> Result<bool, Self::Error>;
        #[operation(child)]
        async fn deferred(&self, branch: ComparisonBranch<'db>, left: Type<'db>, op: NonIdentityOperator, right: Type<'db>) -> Result<Option<ComparisonResult<'db>>, Self::Error>;
        #[operation(local)]
        async fn integer(&self, left: IntLiteralType, op: NonIdentityOperator, right: IntLiteralType, left_type: Type<'db>, right_type: Type<'db>) -> Result<ComparisonResult<'db>, Self::Error>;
        #[operation(child)]
        async fn string(&self, left: StringLiteralType<'db>, op: NonIdentityOperator, right: StringLiteralType<'db>) -> Result<Type<'db>, Self::Error>;
        #[operation(child)]
        async fn bytes(&self, left: BytesLiteralType<'db>, op: NonIdentityOperator, right: BytesLiteralType<'db>) -> Result<Type<'db>, Self::Error>;
        #[operation(child)]
        async fn dunder(&self, left: Type<'db>, op: NonIdentityOperator, right: Type<'db>) -> Result<ComparisonResult<'db>, Self::Error>;
        #[operation(local)]
        async fn new_union(&self) -> Result<UnionBuilder<'db>, Self::Error>;
        #[operation(child)]
        async fn union_add(&self, builder: &mut UnionBuilder<'db>, ty: Type<'db>) -> Result<(), Self::Error>;
        #[operation(child)]
        async fn union_build(&self, builder: UnionBuilder<'db>) -> Result<Type<'db>, Self::Error>;
        #[operation(local)]
        async fn new_equality(&self, policy: ComparisonSoundnessPolicy) -> Result<TupleEqualityEvaluator<'db>, Self::Error>;
        #[operation(child)]
        async fn element_equality(&self, evaluator: &mut TupleEqualityEvaluator<'db>, left: Type<'db>, right: Type<'db>) -> Result<Result<Truthiness, BoolError<'db>>, Self::Error>;
        #[operation(local)]
        async fn retire_equality(&self, evaluator: TupleEqualityEvaluator<'db>) -> Result<(), Self::Error>;
        #[operation(source)]
        async fn report_equality(&self, error: &BoolError<'db>) -> Result<(), Self::Error>;
        #[operation(local)]
        async fn pairs<'tuple>(&self, left: &'tuple FixedLengthTuple<Type<'db>>, right: &'tuple FixedLengthTuple<Type<'db>>) -> Result<FixedTuplePairs<'tuple, 'db>, Self::Error>;
        #[operation(local)]
        #[progress]
        async fn next_pair(&self, pairs: &mut FixedTuplePairs<'_, 'db>) -> Result<Option<(Type<'db>, Type<'db>)>, Self::Error>;
        #[operation(child)]
        async fn variable_tuple(&self, left: &TupleSpec<'db>, op: RichCompareOperator, right: &TupleSpec<'db>) -> Result<ComparisonResult<'db>, Self::Error>;
        #[operation(child)]
        async fn boolean_type(&self) -> Result<Type<'db>, Self::Error>;
    }

    #[finite_capability]
    impl ComparisonFacts {
        fn non_identity(&self, op: ast::CmpOp) -> Option<NonIdentityOperator> {
            Some(match op {
                ast::CmpOp::Is | ast::CmpOp::IsNot => return None,
                ast::CmpOp::Eq => NonIdentityOperator::Rich(RichCompareOperator::Eq),
                ast::CmpOp::NotEq => NonIdentityOperator::Rich(RichCompareOperator::Ne),
                ast::CmpOp::Lt => NonIdentityOperator::Rich(RichCompareOperator::Lt),
                ast::CmpOp::LtE => NonIdentityOperator::Rich(RichCompareOperator::Le),
                ast::CmpOp::Gt => NonIdentityOperator::Rich(RichCompareOperator::Gt),
                ast::CmpOp::GtE => NonIdentityOperator::Rich(RichCompareOperator::Ge),
                ast::CmpOp::In => NonIdentityOperator::Membership(MembershipOperator::In),
                ast::CmpOp::NotIn => NonIdentityOperator::Membership(MembershipOperator::NotIn),
            })
        }
        fn tuple<'a, 'db>(&self, tuple: &'a Cow<'db, TupleSpec<'db>>) -> &'a TupleSpec<'db> { tuple.as_ref() }
        fn kind<'db>(&self, literal: LiteralValueType<'db>) -> LiteralValueTypeKind<'db> { literal.kind() }
        fn ambiguous(&self, truthiness: Truthiness) -> bool { truthiness.is_ambiguous() }
        fn is_typevar(&self, ty: Type<'_>) -> bool { ty.is_type_var() }
        fn bool_type<'db>(&self, value: bool) -> Type<'db> { Type::bool_literal(value) }
        fn int_type<'db>(&self, value: IntLiteralType) -> Type<'db> { Type::int_literal(value.as_i64()) }
        fn bool_as_int<'db>(&self, value: bool) -> Type<'db> { Type::int_literal(i64::from(value)) }
        fn unequal_value(&self, op: RichCompareOperator) -> bool { op == RichCompareOperator::Ne }
        fn recover<'db>(&self, result: ComparisonResult<'db>, left: Type<'db>, op: NonIdentityOperator, right: Type<'db>) -> ComparisonResult<'db> {
            result.map_err(|_| UnsupportedComparisonError { op: op.into(), left_ty: left, right_ty: right })
        }
        fn lengths<'db>(&self, left: &FixedLengthTuple<Type<'db>>, op: RichCompareOperator, right: &FixedLengthTuple<Type<'db>>) -> Type<'db> {
            Type::bool_literal(match op {
                RichCompareOperator::Eq => left.len() == right.len(),
                RichCompareOperator::Ne => left.len() != right.len(),
                RichCompareOperator::Lt => left.len() < right.len(),
                RichCompareOperator::Le => left.len() <= right.len(),
                RichCompareOperator::Gt => left.len() > right.len(),
                RichCompareOperator::Ge => left.len() >= right.len(),
            })
        }
    }

    #[synchronous(compare_sync)]
    #[capabilities(effects = ComparisonEffects, facts = ComparisonFacts)]
    #[passive_values()]
    pub(in crate::types) async fn compare_with<'db, E: ComparisonEffects<'db>>(left: Type<'db>, op: ast::CmpOp, right: Type<'db>, facts: ComparisonFacts, effects: &E) -> Result<ComparisonResult<'db>, E::Error> {
        effects.checkpoint().await?;
        match facts.non_identity(op) {
            Some(op) => effects.recurse(left, op, right).await,
            None => Ok(Ok(effects.identity(left, op, right).await?)),
        }
    }

    #[synchronous(compare_inner_sync)]
    #[capabilities(effects = ComparisonEffects, facts = ComparisonFacts)]
    #[passive_values(Truthiness::Ambiguous, ComparisonBranch::EnumComplementLeft, ComparisonBranch::EnumComplementRight, ComparisonBranch::UnionLeft, ComparisonBranch::UnionRight, ComparisonBranch::IntersectionExpandLeft, ComparisonBranch::IntersectionExpandRight, ComparisonBranch::IntersectionLeft, ComparisonBranch::IntersectionRight, ComparisonBranch::AliasLeft, ComparisonBranch::AliasRight, ComparisonBranch::NewTypeLeft, ComparisonBranch::NewTypeRight, ComparisonBranch::SameTypeVar, ComparisonBranch::TypeVar, ComparisonBranch::ConstraintSets)]
    pub(in crate::types) async fn compare_inner_with<'db, E: ComparisonEffects<'db>>(left: Type<'db>, op: NonIdentityOperator, right: Type<'db>, facts: ComparisonFacts, effects: &E) -> Result<ComparisonResult<'db>, E::Error> {
        effects.checkpoint().await?;
        let policy = effects.policy().await?;
        if let NonIdentityOperator::Rich(rich) = op
            && let Some(left_spec) = effects.tuple_spec(left).await?
            && let Some(right_spec) = effects.tuple_spec(right).await?
        {
            return effects.tuple_comparison(left, rich, right, facts.tuple(&left_spec), facts.tuple(&right_spec)).await;
        }
        if let NonIdentityOperator::Membership(membership) = op
            && let Some(right_spec) = effects.tuple_spec(right).await?
            && let TupleSpec::Fixed(right_tuple) = facts.tuple(&right_spec)
        {
            return Ok(Ok(effects.membership(left, membership, right_tuple, policy).await?));
        }
        let truthiness = match op {
            NonIdentityOperator::Rich(rich @ (RichCompareOperator::Eq | RichCompareOperator::Ne)) => effects.equality(left, rich, right, policy).await?,
            _ => Truthiness::Ambiguous,
        };
        if !facts.ambiguous(truthiness) {
            return Ok(Ok(effects.from_truthiness(truthiness).await?));
        }
        let result = match (left, right) {
            (Type::EnumComplement(complement), _) => effects.deferred(ComparisonBranch::EnumComplementLeft(complement), left, op, right).await?,
            (_, Type::EnumComplement(complement)) => effects.deferred(ComparisonBranch::EnumComplementRight(complement), left, op, right).await?,
            (Type::Union(union), _) => effects.deferred(ComparisonBranch::UnionLeft(union), left, op, right).await?,
            (_, Type::Union(union)) => effects.deferred(ComparisonBranch::UnionRight(union), left, op, right).await?,
            (Type::Intersection(intersection), _) if effects.intersection_has_typevar(intersection).await? => effects.deferred(ComparisonBranch::IntersectionExpandLeft(intersection), left, op, right).await?,
            (_, Type::Intersection(intersection)) if effects.intersection_has_typevar(intersection).await? => effects.deferred(ComparisonBranch::IntersectionExpandRight(intersection), left, op, right).await?,
            (Type::Intersection(intersection), _) => effects.deferred(ComparisonBranch::IntersectionLeft(intersection), left, op, right).await?,
            (_, Type::Intersection(intersection)) => effects.deferred(ComparisonBranch::IntersectionRight(intersection), left, op, right).await?,
            (Type::TypeAlias(_) | Type::Recursive(_), _) => effects.deferred(ComparisonBranch::AliasLeft, left, op, right).await?,
            (_, Type::TypeAlias(_) | Type::Recursive(_)) => effects.deferred(ComparisonBranch::AliasRight, left, op, right).await?,
            (Type::NewTypeInstance(newtype), _) => effects.deferred(ComparisonBranch::NewTypeLeft(newtype), left, op, right).await?,
            (_, Type::NewTypeInstance(newtype)) => effects.deferred(ComparisonBranch::NewTypeRight(newtype), left, op, right).await?,
            (Type::TypeVar(left_tvar), Type::TypeVar(right_tvar)) if effects.same_typevar(left_tvar, right_tvar).await? => effects.deferred(ComparisonBranch::SameTypeVar(left_tvar), left, op, right).await?,
            (Type::TypeVar(typevar), other) | (other, Type::TypeVar(typevar)) if !facts.is_typevar(other) => effects.deferred(ComparisonBranch::TypeVar(typevar), left, op, right).await?,
            (Type::LiteralValue(left_literal), Type::LiteralValue(right_literal)) => {
                match (facts.kind(left_literal), facts.kind(right_literal)) {
                    (LiteralValueTypeKind::Int(n), LiteralValueTypeKind::Int(m)) => Some(effects.integer(n, op, m, left, right).await?),
                    (LiteralValueTypeKind::Int(n), LiteralValueTypeKind::Bool(b)) => Some(facts.recover(effects.recurse(facts.int_type(n), op, facts.bool_as_int(b)).await?, left, op, right)),
                    (LiteralValueTypeKind::Bool(b), LiteralValueTypeKind::Int(m)) => Some(facts.recover(effects.recurse(facts.bool_as_int(b), op, facts.int_type(m)).await?, left, op, right)),
                    (LiteralValueTypeKind::Bool(a), LiteralValueTypeKind::Bool(b)) => Some(facts.recover(effects.recurse(facts.bool_as_int(a), op, facts.bool_as_int(b)).await?, left, op, right)),
                    (LiteralValueTypeKind::String(a), LiteralValueTypeKind::String(b)) => Some(Ok(effects.string(a, op, b).await?)),
                    (LiteralValueTypeKind::Bytes(a), LiteralValueTypeKind::Bytes(b)) => Some(Ok(effects.bytes(a, op, b).await?)),
                    // Exact builtins of different kinds cannot compare equal. LiteralString only
                    // permits a definite answer when the other value cannot be a string.
                    (LiteralValueTypeKind::Int(_) | LiteralValueTypeKind::Bool(_) | LiteralValueTypeKind::String(_) | LiteralValueTypeKind::Bytes(_), LiteralValueTypeKind::Int(_) | LiteralValueTypeKind::Bool(_) | LiteralValueTypeKind::String(_) | LiteralValueTypeKind::Bytes(_))
                    | (LiteralValueTypeKind::LiteralString, LiteralValueTypeKind::Int(_) | LiteralValueTypeKind::Bool(_) | LiteralValueTypeKind::Bytes(_))
                    | (LiteralValueTypeKind::Int(_) | LiteralValueTypeKind::Bool(_) | LiteralValueTypeKind::Bytes(_), LiteralValueTypeKind::LiteralString)
                    if let NonIdentityOperator::Rich(rich @ (RichCompareOperator::Eq | RichCompareOperator::Ne)) = op => Some(Ok(facts.bool_type(facts.unequal_value(rich)))),
                    _ => None,
                }
            }
            (Type::KnownInstance(KnownInstanceType::ConstraintSet(left_set)), Type::KnownInstance(KnownInstanceType::ConstraintSet(right_set))) => effects.deferred(ComparisonBranch::ConstraintSets(left_set, right_set), left, op, right).await?,
            _ => None,
        };
        match result {
            Some(result) => Ok(result),
            None => effects.dunder(left, op, right).await,
        }
    }

    #[synchronous(compare_tuple_sync)]
    #[capabilities(effects = ComparisonEffects, facts = ComparisonFacts)]
    #[passive_values(Truthiness::Ambiguous, NonIdentityOperator::Rich, Err)]
    pub(in crate::types) async fn compare_tuple_with<'db, E: ComparisonEffects<'db>>(left: &TupleSpec<'db>, op: RichCompareOperator, right: &TupleSpec<'db>, facts: ComparisonFacts, effects: &E) -> Result<ComparisonResult<'db>, E::Error> {
        match (left, right) {
            (TupleSpec::Fixed(left), TupleSpec::Fixed(right)) => {
                let mut pairs = effects.pairs(left, right).await?;
                let mut builder = effects.new_union().await?;
                let policy = effects.policy().await?;
                let mut equality = effects.new_equality(policy).await?;
                #[cursor_loop]
                while let Some(pair) = effects.next_pair(&mut pairs).await? {
                    let (left, right) = pair;
                    let truthiness = match effects.element_equality(&mut equality, left, right).await? {
                        Ok(truthiness) => truthiness,
                        Err(error) => {
                            effects.report_equality(&error).await?;
                            Truthiness::Ambiguous
                        }
                    };
                    match truthiness {
                        Truthiness::AlwaysTrue => continue,
                        Truthiness::AlwaysFalse | Truthiness::Ambiguous => {
                            let result = match op {
                                RichCompareOperator::Lt | RichCompareOperator::Le | RichCompareOperator::Gt | RichCompareOperator::Ge => match effects.recurse(left, NonIdentityOperator::Rich(op), right).await? {
                                    Ok(ty) => ty,
                                    Err(error) => {
                                        effects.retire_equality(equality).await?;
                                        return Ok(Err(error));
                                    }
                                },
                                RichCompareOperator::Eq => facts.bool_type(false),
                                RichCompareOperator::Ne => facts.bool_type(true),
                            };
                            effects.union_add(&mut builder, result).await?;
                            if facts.ambiguous(truthiness) { continue; }
                            let result = effects.union_build(builder).await?;
                            effects.retire_equality(equality).await?;
                            return Ok(Ok(result));
                        }
                    }
                }
                effects.union_add(&mut builder, facts.lengths(left, op, right)).await?;
                let result = effects.union_build(builder).await?;
                effects.retire_equality(equality).await?;
                Ok(Ok(result))
            }
            (TupleSpec::Variable(_), _) | (_, TupleSpec::Variable(_)) if matches!(op, RichCompareOperator::Eq | RichCompareOperator::Ne) => Ok(Ok(effects.boolean_type().await?)),
            (TupleSpec::Variable(_), _) | (_, TupleSpec::Variable(_)) => effects.variable_tuple(left, op, right).await,
        }
    }
}

pub(super) fn infallible<T>(result: Result<T, std::convert::Infallible>) -> T {
    match result {
        Ok(value) => value,
        Err(never) => match never {},
    }
}

pub(in crate::types) fn integer_comparison<'db>(
    left: IntLiteralType,
    op: NonIdentityOperator,
    right: IntLiteralType,
    left_type: Type<'db>,
    right_type: Type<'db>,
) -> ComparisonResult<'db> {
    Ok(Type::bool_literal(match op {
        NonIdentityOperator::Rich(RichCompareOperator::Eq) => left == right,
        NonIdentityOperator::Rich(RichCompareOperator::Ne) => left != right,
        NonIdentityOperator::Rich(RichCompareOperator::Lt) => left < right,
        NonIdentityOperator::Rich(RichCompareOperator::Le) => left <= right,
        NonIdentityOperator::Rich(RichCompareOperator::Gt) => left > right,
        NonIdentityOperator::Rich(RichCompareOperator::Ge) => left >= right,
        NonIdentityOperator::Membership(_) => {
            return Err(UnsupportedComparisonError {
                op: op.into(),
                left_ty: left_type,
                right_ty: right_type,
            });
        }
    }))
}
