//! Shared equality phases and tuple-element decisions.

use std::borrow::Cow;
use std::convert::Infallible;

use ty_mapping_probe_macros::shared_semantic_family;

use super::nominal_source::NominalTuplePairs;
use super::{
    ComparisonBranch, ComparisonEvaluator, ComparisonKey, ComparisonOperator, ComparisonResult,
    ComparisonSoundnessPolicy, KnownComparisonSemantics, TupleEqualityEvaluator,
};
use crate::place::PlaceAndQualifiers;
use crate::types::bool::BoolError;
use crate::types::function::FunctionLiteral;
use crate::types::literal::{BytesLiteralType, StringLiteralType};
use crate::types::tuple::{FixedLengthTuple, TupleSpec};
use crate::types::{
    ClassLiteral, FunctionType, KnownBoundMethodType, KnownClass, KnownInstanceType,
    NominalInstanceType, SpecialFormType, WrapperDescriptorKind,
};
use crate::types::{LiteralValueTypeKind, MemberLookupPolicy, Truthiness, Type};
use crate::{Db, ProgramEnvironment};

#[cfg(feature = "experimental-analysis")]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum EqualityOperation {
    AliasResolution,
    EnumComparison,
    Singleton,
    DunderCall,
    DunderTruthiness,
    DynamicComparison,
    FiniteAlternatives,
    FiniteComparison,
    StructuralComparison,
    LiteralEquality,
    LiteralNarrowing,
    ComparisonSemantics,
}

pub(in crate::types) fn infallible<T>(result: Result<T, Infallible>) -> T {
    match result {
        Ok(value) => value,
        Err(error) => match error {},
    }
}

pub(in crate::types) struct EqualityFacts;

shared_semantic_family! {
    #[synchronous(SynchronousEqualityEffects)]
    pub(in crate::types) trait EqualityEffects<'db> {
        type Error;
        #[operation(checkpoint)]
        async fn checkpoint(&self) -> Result<(), Self::Error>;
        #[operation(local)]
        async fn clone_environment(&self, evaluator: &ComparisonEvaluator<'db>) -> Result<ProgramEnvironment<'db>, Self::Error>;
        #[operation(source)]
        async fn alias(&self, ty: Type<'db>) -> Result<Type<'db>, Self::Error>;
        #[operation(local)]
        async fn insert_active(&self, evaluator: &mut ComparisonEvaluator<'db>, key: ComparisonKey<'db>) -> Result<bool, Self::Error>;
        #[operation(local)]
        async fn remove_active(&self, evaluator: &mut ComparisonEvaluator<'db>, key: ComparisonKey<'db>) -> Result<(), Self::Error>;
        #[operation(source)]
        async fn evaluate(&self, evaluator: &mut ComparisonEvaluator<'db>, left: Type<'db>, right: Type<'db>, branch: ComparisonBranch, operator: ComparisonOperator) -> Result<ComparisonResult<'db>, Self::Error>;
        #[operation(source)]
        async fn evaluate_once(&self, evaluator: &mut ComparisonEvaluator<'db>, left: Type<'db>, right: Type<'db>, branch: ComparisonBranch, operator: ComparisonOperator) -> Result<ComparisonResult<'db>, Self::Error>;
        #[operation(source)]
        async fn enum_comparison(&self, evaluator: &mut ComparisonEvaluator<'db>, left: Type<'db>, right: Type<'db>, branch: ComparisonBranch, operator: ComparisonOperator) -> Result<Option<ComparisonResult<'db>>, Self::Error>;
        #[operation(source)]
        async fn dynamic_comparison(&self, evaluator: &mut ComparisonEvaluator<'db>, left: Type<'db>, right: Type<'db>, branch: ComparisonBranch, operator: ComparisonOperator) -> Result<Option<ComparisonResult<'db>>, Self::Error>;
        #[operation(source)]
        async fn dynamic_constraint(&self, evaluator: &mut ComparisonEvaluator<'db>, env: &ProgramEnvironment<'db>, left: Type<'db>, right: Type<'db>, branch: ComparisonBranch, operator: ComparisonOperator) -> Result<Option<ComparisonResult<'db>>, Self::Error>;
        #[operation(source)]
        async fn finite_comparison(&self, evaluator: &mut ComparisonEvaluator<'db>, left: Type<'db>, right: Type<'db>, branch: ComparisonBranch, operator: ComparisonOperator) -> Result<Option<ComparisonResult<'db>>, Self::Error>;
        #[operation(source)]
        async fn finite_alternatives(&self, env: &ProgramEnvironment<'db>, ty: Type<'db>, operator: ComparisonOperator) -> Result<Option<Vec<Type<'db>>>, Self::Error>;
        #[operation(source)]
        async fn finite_alternatives_other(&self, env: &ProgramEnvironment<'db>, ty: Type<'db>, operator: ComparisonOperator) -> Result<Option<Vec<Type<'db>>>, Self::Error>;
        #[operation(source)]
        async fn union_left(&self, evaluator: &mut ComparisonEvaluator<'db>, alternatives: Vec<Type<'db>>, right: Type<'db>, branch: ComparisonBranch, operator: ComparisonOperator) -> Result<ComparisonResult<'db>, Self::Error>;
        #[operation(source)]
        async fn union_right(&self, evaluator: &mut ComparisonEvaluator<'db>, left: Type<'db>, alternatives: Vec<Type<'db>>, branch: ComparisonBranch, operator: ComparisonOperator) -> Result<ComparisonResult<'db>, Self::Error>;
        #[operation(source)]
        async fn structural_comparison(&self, evaluator: &mut ComparisonEvaluator<'db>, left: Type<'db>, right: Type<'db>, branch: ComparisonBranch, operator: ComparisonOperator) -> Result<ComparisonResult<'db>, Self::Error>;
        #[operation(source)]
        async fn structural_other(&self, evaluator: &mut ComparisonEvaluator<'db>, env: &ProgramEnvironment<'db>, left: Type<'db>, right: Type<'db>, branch: ComparisonBranch, operator: ComparisonOperator) -> Result<ComparisonResult<'db>, Self::Error>;
        #[operation(source)]
        async fn literal_equality(&self, env: &ProgramEnvironment<'db>, left: LiteralValueTypeKind<'db>, right: LiteralValueTypeKind<'db>, operator: ComparisonOperator) -> Result<Option<bool>, Self::Error>;
        #[operation(source)]
        async fn literal_equality_other(&self, env: &ProgramEnvironment<'db>, left: LiteralValueTypeKind<'db>, right: LiteralValueTypeKind<'db>, operator: ComparisonOperator) -> Result<Option<bool>, Self::Error>;
        #[operation(local)]
        async fn string_equality(&self, left: StringLiteralType<'db>, right: StringLiteralType<'db>) -> Result<bool, Self::Error>;
        #[operation(local)]
        async fn bytes_equality(&self, left: BytesLiteralType<'db>, right: BytesLiteralType<'db>) -> Result<bool, Self::Error>;
        #[operation(source)]
        async fn narrow_literals(&self, env: &ProgramEnvironment<'db>, left: Type<'db>, right: Type<'db>, left_literal: LiteralValueTypeKind<'db>, right_literal: LiteralValueTypeKind<'db>, positive: bool) -> Result<ComparisonResult<'db>, Self::Error>;
        #[operation(source)]
        async fn singleton(&self, evaluator: &ComparisonEvaluator<'db>, ty: Type<'db>) -> Result<bool, Self::Error>;
        #[operation(source)]
        async fn singleton_other(&self, evaluator: &ComparisonEvaluator<'db>, ty: Type<'db>) -> Result<bool, Self::Error>;
        #[operation(source)]
        async fn comparison_semantics(&self, evaluator: &ComparisonEvaluator<'db>, ty: Type<'db>, operator: ComparisonOperator) -> Result<Option<KnownComparisonSemantics>, Self::Error>;
        #[operation(source)]
        async fn comparison_semantics_other(&self, evaluator: &ComparisonEvaluator<'db>, ty: Type<'db>, operator: ComparisonOperator) -> Result<Option<KnownComparisonSemantics>, Self::Error>;
        #[operation(source)]
        async fn tuple_equality(&self, evaluator: &mut ComparisonEvaluator<'db>, left: Type<'db>, right: Type<'db>) -> Result<Truthiness, Self::Error>;
        #[operation(source)]
        async fn dunder_equality(&self, evaluator: &ComparisonEvaluator<'db>, left: Type<'db>, right: Type<'db>) -> Result<Option<Type<'db>>, Self::Error>;
        #[operation(source)]
        async fn try_bool(&self, evaluator: &ComparisonEvaluator<'db>, ty: Type<'db>) -> Result<Result<Truthiness, BoolError<'db>>, Self::Error>;
        #[operation(local)]
        async fn types_equal(&self, left: Type<'db>, right: Type<'db>) -> Result<bool, Self::Error>;
        #[operation(local)]
        async fn members_equal(&self, left: PlaceAndQualifiers<'db>, right: PlaceAndQualifiers<'db>) -> Result<bool, Self::Error>;
        #[operation(checkpoint)]
        async fn nominal_checkpoint(&self) -> Result<(), Self::Error>;
        #[operation(source)]
        async fn known_semantics(&self, env: &ProgramEnvironment<'db>, ty: Type<'db>, operator: ComparisonOperator, policy: ComparisonSoundnessPolicy) -> Result<Option<KnownComparisonSemantics>, Self::Error>;
        #[operation(source)]
        async fn instance_semantics(&self, env: &ProgramEnvironment<'db>, ty: Type<'db>, operator: ComparisonOperator) -> Result<Option<KnownComparisonSemantics>, Self::Error>;
        #[operation(source)]
        async fn known_semantics_specialized(&self, env: &ProgramEnvironment<'db>, ty: Type<'db>, operator: ComparisonOperator, policy: ComparisonSoundnessPolicy) -> Result<Option<KnownComparisonSemantics>, Self::Error>;
        #[operation(source)]
        async fn nominal_is_final(&self, env: &ProgramEnvironment<'db>, instance: NominalInstanceType<'db>) -> Result<bool, Self::Error>;
        #[operation(source)]
        async fn nominal_has_known_class(&self, instance: NominalInstanceType<'db>, class: KnownClass) -> Result<bool, Self::Error>;
        #[operation(source)]
        async fn nominal_class_available(&self, env: &ProgramEnvironment<'db>, ty: Type<'db>) -> Result<bool, Self::Error>;
        #[operation(source)]
        async fn equality_meta_type(&self, env: &ProgramEnvironment<'db>, ty: Type<'db>) -> Result<Type<'db>, Self::Error>;
        #[operation(source)]
        async fn equality_dunder(&self, env: &ProgramEnvironment<'db>, ty: Type<'db>, name: &'static str) -> Result<PlaceAndQualifiers<'db>, Self::Error>;
        #[operation(source)]
        async fn equality_known_class(&self, env: &ProgramEnvironment<'db>, class: KnownClass) -> Result<Type<'db>, Self::Error>;
        #[operation(source)]
        async fn same_member_implementation(&self, left: PlaceAndQualifiers<'db>, right: PlaceAndQualifiers<'db>) -> Result<bool, Self::Error>;
        #[operation(local)]
        async fn function_identity(&self, function: FunctionType<'db>) -> Result<FunctionLiteral<'db>, Self::Error>;
        #[operation(local)]
        #[progress]
        async fn next_builtin_semantics(&self, index: &mut usize) -> Result<Option<(KnownClass, KnownComparisonSemantics)>, Self::Error>;
        #[operation(source)]
        async fn identity_semantics(&self, env: &ProgramEnvironment<'db>, ty: Type<'db>, operator: ComparisonOperator) -> Result<bool, Self::Error>;
        #[operation(source)]
        async fn metaclass_instance(&self, env: &ProgramEnvironment<'db>, class: ClassLiteral<'db>) -> Result<Type<'db>, Self::Error>;
        #[operation(source)]
        async fn singleton_type(&self, env: &ProgramEnvironment<'db>, ty: Type<'db>) -> Result<bool, Self::Error>;
        #[operation(source)]
        async fn singleton_nominal(&self, instance: NominalInstanceType<'db>) -> Result<bool, Self::Error>;
        #[operation(source)]
        async fn singleton_specialized(&self, env: &ProgramEnvironment<'db>, ty: Type<'db>) -> Result<bool, Self::Error>;
        #[operation(source)]
        async fn finite_specialized(&self, env: &ProgramEnvironment<'db>, ty: Type<'db>, operator: ComparisonOperator) -> Result<Option<Vec<Type<'db>>>, Self::Error>;
        #[operation(local)]
        async fn boolean_alternatives(&self) -> Result<Vec<Type<'db>>, Self::Error>;
        #[operation(source)]
        async fn enum_alternatives(&self, env: &ProgramEnvironment<'db>, instance: NominalInstanceType<'db>) -> Result<Option<Vec<Type<'db>>>, Self::Error>;
        #[operation(source)]
        async fn structural_specialized(&self, evaluator: &mut ComparisonEvaluator<'db>, env: &ProgramEnvironment<'db>, left: Type<'db>, right: Type<'db>, branch: ComparisonBranch, operator: ComparisonOperator) -> Result<ComparisonResult<'db>, Self::Error>;
        #[operation(source)]
        async fn excluded_string_literal(&self, env: &ProgramEnvironment<'db>, left: Type<'db>, right: Type<'db>) -> Result<bool, Self::Error>;
        #[operation(local)]
        async fn same_module(&self, left: Type<'db>, right: Type<'db>) -> Result<bool, Self::Error>;
        #[operation(local)]
        async fn bound_method_identity(&self, left: Type<'db>, right: Type<'db>) -> Result<bool, Self::Error>;
        #[operation(source)]
        async fn equality_equivalent(&self, env: &ProgramEnvironment<'db>, left: Type<'db>, right: Type<'db>) -> Result<bool, Self::Error>;
        #[operation(source)]
        async fn equality_disjoint(&self, env: &ProgramEnvironment<'db>, left: Type<'db>, right: Type<'db>) -> Result<bool, Self::Error>;
        #[operation(source)]
        async fn different_semantics(&self, env: &ProgramEnvironment<'db>, left: Type<'db>, right: Type<'db>, operator: ComparisonOperator) -> Result<ComparisonResult<'db>, Self::Error>;
        #[operation(source)]
        async fn nominal_comparison(&self, evaluator: &mut ComparisonEvaluator<'db>, left: NominalInstanceType<'db>, right: NominalInstanceType<'db>, operator: ComparisonOperator) -> Result<ComparisonResult<'db>, Self::Error>;
        #[operation(source)]
        async fn equality_tuple_spec(&self, env: &ProgramEnvironment<'db>, instance: NominalInstanceType<'db>) -> Result<Option<Cow<'db, TupleSpec<'db>>>, Self::Error>;
        #[operation(local)]
        async fn equality_tuple_pairs<'tuple>(&self, left: &'tuple FixedLengthTuple<Type<'db>>, right: &'tuple FixedLengthTuple<Type<'db>>) -> Result<NominalTuplePairs<'tuple, 'db>, Self::Error>;
        #[operation(local)]
        #[progress]
        async fn next_equality_tuple_pair(&self, pairs: &mut NominalTuplePairs<'_, 'db>) -> Result<Option<(Type<'db>, Type<'db>)>, Self::Error>;
    }

    #[finite_capability]
    impl EqualityFacts {
        fn environment<'a, 'db>(&self, evaluator: &'a ComparisonEvaluator<'db>) -> &'a ProgramEnvironment<'db> { &evaluator.env }
fn same_descriptor(&self, left: WrapperDescriptorKind, right: WrapperDescriptorKind) -> bool { left == right }
fn semantics_is(&self, value: Option<KnownComparisonSemantics>, expected: KnownComparisonSemantics) -> bool { value == Some(expected) }
fn same_semantics(&self, left: KnownComparisonSemantics, right: KnownComparisonSemantics) -> bool { left == right }
fn allow_unsafe_equality(&self, policy: ComparisonSoundnessPolicy) -> bool { policy.allow_unsafe_equality }
fn dunder(&self, operator: ComparisonOperator) -> &'static str { operator.dunder() }
fn inequality(&self, operator: ComparisonOperator) -> bool { operator == ComparisonOperator::Inequality }
fn undefined_member(&self, member: PlaceAndQualifiers<'_>) -> bool { member.place.is_undefined() }
fn same_qualifiers(&self, left: PlaceAndQualifiers<'_>, right: PlaceAndQualifiers<'_>) -> bool { left.qualifiers == right.qualifiers }
fn function_member<'db>(&self, member: PlaceAndQualifiers<'db>) -> Option<FunctionType<'db>> { member.ignore_possibly_undefined().and_then(Type::as_function_literal) }
fn same_function_literal(&self, left: FunctionLiteral<'_>, right: FunctionLiteral<'_>) -> bool { left == right }
fn fixed_tuple<'tuple, 'db>(&self, tuple: &'tuple Cow<'db, TupleSpec<'db>>) -> Option<&'tuple FixedLengthTuple<Type<'db>>> { tuple.as_fixed_length() }
fn same_tuple_length<'db>(&self, left: &FixedLengthTuple<Type<'db>>, right: &FixedLengthTuple<Type<'db>>) -> bool { left.all_elements().len() == right.all_elements().len() }
fn special_form_singleton(&self, form: SpecialFormType) -> bool { form.is_guaranteed_singleton() }
        fn key<'db>(&self, left: Type<'db>, right: Type<'db>, branch: ComparisonBranch, operator: ComparisonOperator) -> ComparisonKey<'db> { ComparisonKey { left, right, branch, operator } }
        fn literal<'db>(&self, literal: crate::types::LiteralValueType<'db>) -> LiteralValueTypeKind<'db> { literal.kind() }
        fn condition(&self, operator: ComparisonOperator, branch: ComparisonBranch) -> bool { operator.condition_expects_equality(branch) }
        fn result<'db>(&self, operator: ComparisonOperator, equal: bool) -> ComparisonResult<'db> { operator.result_from_equality(equal) }
        fn integer_equal(&self, left: crate::types::literal::IntLiteralType, right: crate::types::literal::IntLiteralType) -> bool { left.as_i64() == right.as_i64() }
        fn boolean_equal(&self, left: bool, right: bool) -> bool { left == right }
        fn integer_boolean_equal(&self, left: crate::types::literal::IntLiteralType, right: bool) -> bool { left.as_i64() == i64::from(right) }
        fn known_semantics(&self, value: Option<KnownComparisonSemantics>) -> bool { value.is_some() }
        fn literal_singleton(&self, literal: LiteralValueTypeKind<'_>) -> bool {
            matches!(literal, LiteralValueTypeKind::Bool(_) | LiteralValueTypeKind::Enum(_))
        }
        fn evaluator<'a, 'db>(&self, evaluator: &'a mut TupleEqualityEvaluator<'db>) -> &'a mut ComparisonEvaluator<'db> { &mut evaluator.evaluator }
    }

    #[synchronous(evaluate_sync)]
    #[capabilities(effects = EqualityEffects, facts = EqualityFacts)]
    #[passive_values(ComparisonKey, ComparisonResult::Ambiguous)]
    pub(in crate::types) async fn evaluate_with<'db, E: EqualityEffects<'db>>(
        evaluator: &mut ComparisonEvaluator<'db>, left: Type<'db>, right: Type<'db>, branch: ComparisonBranch, operator: ComparisonOperator, facts: EqualityFacts, effects: &E,
    ) -> Result<ComparisonResult<'db>, E::Error> {
        effects.checkpoint().await?;
        let left = effects.alias(left).await?;
        let right = effects.alias(right).await?;
        let key = facts.key(left, right, branch, operator);
        if !effects.insert_active(evaluator, key).await? { return Ok(ComparisonResult::Ambiguous); }
        let result = effects.evaluate_once(evaluator, left, right, branch, operator).await?;
        effects.remove_active(evaluator, key).await?;
        Ok(result)
    }

    #[synchronous(evaluate_once_sync)]
    #[capabilities(effects = EqualityEffects)]
    #[passive_values()]
    pub(in crate::types) async fn evaluate_once_with<'db, E: EqualityEffects<'db>>(
        evaluator: &mut ComparisonEvaluator<'db>, left: Type<'db>, right: Type<'db>, branch: ComparisonBranch, operator: ComparisonOperator, effects: &E,
    ) -> Result<ComparisonResult<'db>, E::Error> {
        effects.checkpoint().await?;
        if let Some(result) = effects.enum_comparison(evaluator, left, right, branch, operator).await? { return Ok(result); }
        if let Some(result) = effects.dynamic_comparison(evaluator, left, right, branch, operator).await? { return Ok(result); }
        if let Some(result) = effects.finite_comparison(evaluator, left, right, branch, operator).await? { return Ok(result); }
        effects.structural_comparison(evaluator, left, right, branch, operator).await
    }

    #[synchronous(dynamic_sync)]
    #[capabilities(effects = EqualityEffects, facts = EqualityFacts)]
    #[passive_values(ComparisonResult::Ambiguous)]
    pub(in crate::types) async fn dynamic_with<'db, E: EqualityEffects<'db>>(
        evaluator: &mut ComparisonEvaluator<'db>, left: Type<'db>, right: Type<'db>, branch: ComparisonBranch, operator: ComparisonOperator, facts: EqualityFacts, effects: &E,
    ) -> Result<Option<ComparisonResult<'db>>, E::Error> {
        effects.checkpoint().await?;
        let env = effects.clone_environment(evaluator).await?;
        match (left, right) {
            (Type::Dynamic(_), _) if !facts.condition(operator, branch) => effects.dynamic_constraint(evaluator, &env, left, right, branch, operator).await,
            (Type::Dynamic(_), _) | (_, Type::Dynamic(_)) => Ok(Some(ComparisonResult::Ambiguous)),
            _ => Ok(None),
        }
    }

    #[synchronous(finite_sync)]
    #[capabilities(effects = EqualityEffects)]
    #[passive_values()]
    pub(in crate::types) async fn finite_with<'db, E: EqualityEffects<'db>>(
        evaluator: &mut ComparisonEvaluator<'db>, left: Type<'db>, right: Type<'db>, branch: ComparisonBranch, operator: ComparisonOperator, effects: &E,
    ) -> Result<Option<ComparisonResult<'db>>, E::Error> {
        effects.checkpoint().await?;
        let env = effects.clone_environment(evaluator).await?;
        if let Some(alternatives) = effects.finite_alternatives(&env, left, operator).await? {
            return Ok(Some(effects.union_left(evaluator, alternatives, right, branch, operator).await?));
        }
        if let Some(alternatives) = effects.finite_alternatives(&env, right, operator).await? {
            return Ok(Some(effects.union_right(evaluator, left, alternatives, branch, operator).await?));
        }
        Ok(None)
    }

    #[synchronous(finite_alternatives_sync)]
    #[capabilities(effects = EqualityEffects)]
    #[passive_values()]
    pub(in crate::types) async fn finite_alternatives_with<'db, E: EqualityEffects<'db>>(
        env: &ProgramEnvironment<'db>, ty: Type<'db>, operator: ComparisonOperator, effects: &E,
    ) -> Result<Option<Vec<Type<'db>>>, E::Error> {
        effects.checkpoint().await?;
        match ty {
            Type::EnumComplement(_) | Type::Intersection(_) | Type::NewTypeInstance(_) | Type::NominalInstance(_) => effects.finite_alternatives_other(env, ty, operator).await,
            _ => Ok(None),
        }
    }

    #[synchronous(structural_sync)]
    #[capabilities(effects = EqualityEffects, facts = EqualityFacts)]
    #[passive_values()]
    pub(in crate::types) async fn structural_with<'db, E: EqualityEffects<'db>>(
        evaluator: &mut ComparisonEvaluator<'db>, left: Type<'db>, right: Type<'db>, branch: ComparisonBranch, operator: ComparisonOperator, facts: EqualityFacts, effects: &E,
    ) -> Result<ComparisonResult<'db>, E::Error> {
        effects.checkpoint().await?;
        let env = effects.clone_environment(evaluator).await?;
        match (left, right) {
            (Type::LiteralValue(left_literal), Type::LiteralValue(right_literal)) => {
                let left_literal = facts.literal(left_literal);
                let right_literal = facts.literal(right_literal);
                match effects.literal_equality(&env, left_literal, right_literal, operator).await? {
                    Some(equal) => Ok(facts.result(operator, equal)),
                    None => effects.narrow_literals(&env, left, right, left_literal, right_literal, facts.condition(operator, branch)).await,
                }
            }
            _ => effects.structural_other(evaluator, &env, left, right, branch, operator).await,
        }
    }

    #[synchronous(literal_equality_sync)]
    #[capabilities(effects = EqualityEffects, facts = EqualityFacts)]
    #[passive_values()]
    pub(in crate::types) async fn literal_equality_with<'db, E: EqualityEffects<'db>>(
        env: &ProgramEnvironment<'db>, left: LiteralValueTypeKind<'db>, right: LiteralValueTypeKind<'db>, operator: ComparisonOperator, facts: EqualityFacts, effects: &E,
    ) -> Result<Option<bool>, E::Error> {
        effects.checkpoint().await?;
        match (left, right) {
            (LiteralValueTypeKind::Int(left), LiteralValueTypeKind::Int(right)) => Ok(Some(facts.integer_equal(left, right))),
            (LiteralValueTypeKind::Bool(left), LiteralValueTypeKind::Bool(right)) => Ok(Some(facts.boolean_equal(left, right))),
            (LiteralValueTypeKind::Int(left), LiteralValueTypeKind::Bool(right)) | (LiteralValueTypeKind::Bool(right), LiteralValueTypeKind::Int(left)) => Ok(Some(facts.integer_boolean_equal(left, right))),
            (LiteralValueTypeKind::String(left), LiteralValueTypeKind::String(right)) => Ok(Some(effects.string_equality(left, right).await?)),
            (LiteralValueTypeKind::Bytes(left), LiteralValueTypeKind::Bytes(right)) => Ok(Some(effects.bytes_equality(left, right).await?)),
            (LiteralValueTypeKind::LiteralString, LiteralValueTypeKind::LiteralString | LiteralValueTypeKind::String(_)) | (LiteralValueTypeKind::String(_), LiteralValueTypeKind::LiteralString) => Ok(None),
            _ => effects.literal_equality_other(env, left, right, operator).await,
        }
    }

    #[synchronous(singleton_sync)]
    #[capabilities(effects = EqualityEffects, facts = EqualityFacts)]
    #[passive_values()]
    pub(in crate::types) async fn singleton_with<'db, E: EqualityEffects<'db>>(
        evaluator: &ComparisonEvaluator<'db>, ty: Type<'db>, facts: EqualityFacts, effects: &E,
    ) -> Result<bool, E::Error> {
        effects.checkpoint().await?;
        match ty {
            Type::LiteralValue(literal) => Ok(facts.literal_singleton(facts.literal(literal))),
            _ => effects.singleton_other(evaluator, ty).await,
        }
    }

    #[synchronous(comparison_semantics_sync)]
    #[capabilities(effects = EqualityEffects, facts = EqualityFacts)]
    #[passive_values(KnownComparisonSemantics::Int, KnownComparisonSemantics::Str, KnownComparisonSemantics::Bytes)]
    pub(in crate::types) async fn comparison_semantics_with<'db, E: EqualityEffects<'db>>(
        evaluator: &ComparisonEvaluator<'db>, ty: Type<'db>, operator: ComparisonOperator, facts: EqualityFacts, effects: &E,
    ) -> Result<Option<KnownComparisonSemantics>, E::Error> {
        effects.checkpoint().await?;
        if let Type::LiteralValue(literal) = ty {
            match facts.literal(literal) {
                LiteralValueTypeKind::Int(_) | LiteralValueTypeKind::Bool(_) => return Ok(Some(KnownComparisonSemantics::Int)),
                LiteralValueTypeKind::String(_) | LiteralValueTypeKind::LiteralString => return Ok(Some(KnownComparisonSemantics::Str)),
                LiteralValueTypeKind::Bytes(_) => return Ok(Some(KnownComparisonSemantics::Bytes)),
                LiteralValueTypeKind::Enum(_) => {},
            }
        }
        effects.comparison_semantics_other(evaluator, ty, operator).await
    }

#[synchronous(comparison_truthiness_sync)]
#[capabilities(effects = EqualityEffects)]
#[passive_values(ComparisonBranch::Positive, Truthiness::AlwaysTrue, Truthiness::AlwaysFalse, Truthiness::Ambiguous)]
pub(in crate::types) async fn comparison_truthiness_with<'db, E: EqualityEffects<'db>>(
    evaluator: &mut ComparisonEvaluator<'db>, left: Type<'db>, right: Type<'db>, operator: ComparisonOperator, effects: &E,
) -> Result<Truthiness, E::Error> {
    effects.checkpoint().await?;
    Ok(match effects.evaluate(evaluator, left, right, ComparisonBranch::Positive, operator).await? {
        ComparisonResult::AlwaysTrue => Truthiness::AlwaysTrue,
        ComparisonResult::AlwaysFalse => Truthiness::AlwaysFalse,
        ComparisonResult::CanNarrow(_) | ComparisonResult::Ambiguous => Truthiness::Ambiguous,
    })
}

    #[synchronous(tuple_equality_sync)]
    #[capabilities(effects = EqualityEffects, facts = EqualityFacts)]
    #[passive_values(Truthiness::AlwaysTrue, Truthiness::AlwaysFalse, Truthiness::Ambiguous, ComparisonBranch::Positive, ComparisonOperator::Equality)]
    pub(in crate::types) async fn tuple_equality_with<'db, E: EqualityEffects<'db>>(
        evaluator: &mut ComparisonEvaluator<'db>, left: Type<'db>, right: Type<'db>, facts: EqualityFacts, effects: &E,
    ) -> Result<Truthiness, E::Error> {
        effects.checkpoint().await?;
        if effects.types_equal(left, right).await? && effects.singleton(evaluator, left).await? { return Ok(Truthiness::AlwaysTrue); }
        match effects.evaluate(evaluator, left, right, ComparisonBranch::Positive, ComparisonOperator::Equality).await? {
            ComparisonResult::AlwaysTrue => Ok(Truthiness::AlwaysTrue),
            // Reflexive equality makes a false result incompatible with shared object identity.
            ComparisonResult::AlwaysFalse
                if facts.known_semantics(effects.comparison_semantics(evaluator, left, ComparisonOperator::Equality).await?)
                    && facts.known_semantics(effects.comparison_semantics(evaluator, right, ComparisonOperator::Equality).await?) => Ok(Truthiness::AlwaysFalse),
            _ => Ok(Truthiness::Ambiguous),
        }
    }

    #[synchronous(tuple_element_sync)]
    #[capabilities(effects = EqualityEffects, facts = EqualityFacts)]
    #[passive_values(Truthiness::AlwaysTrue, Truthiness::AlwaysFalse, Truthiness::Ambiguous, Err)]
    pub(in crate::types) async fn tuple_element_with<'db, E: EqualityEffects<'db>>(
        evaluator: &mut TupleEqualityEvaluator<'db>, left: Type<'db>, right: Type<'db>, facts: EqualityFacts, effects: &E,
    ) -> Result<Result<Truthiness, BoolError<'db>>, E::Error> {
        effects.checkpoint().await?;
        let evaluator = facts.evaluator(evaluator);
        match effects.tuple_equality(evaluator, left, right).await? {
            Truthiness::AlwaysTrue => return Ok(Ok(Truthiness::AlwaysTrue)),
            Truthiness::AlwaysFalse => return Ok(Ok(Truthiness::AlwaysFalse)),
            Truthiness::Ambiguous => {},
        }
        let Some(result) = effects.dunder_equality(evaluator, left, right).await? else { return Ok(Ok(Truthiness::Ambiguous)); };
        // Identity can make a false equality result true, but cannot make a true result false.
        Ok(match effects.try_bool(evaluator, result).await? {
            Ok(Truthiness::AlwaysTrue) => Ok(Truthiness::AlwaysTrue),
            Ok(Truthiness::AlwaysFalse | Truthiness::Ambiguous) => Ok(Truthiness::Ambiguous),
            Err(error) => Err(error),
        })
    }
    /// Expands finite nominal and enum alternatives whose comparison behavior is known.
    #[synchronous(finite_alternatives_other_sync)]
    #[capabilities(effects = EqualityEffects, facts = EqualityFacts)]
    #[passive_values(KnownClass::Bool, ComparisonSoundnessPolicy::CONSERVATIVE)]
    pub(in crate::types) async fn finite_alternatives_other_with<'db, E: EqualityEffects<'db>>(
        env: &ProgramEnvironment<'db>, ty: Type<'db>, operator: ComparisonOperator, facts: EqualityFacts, effects: &E,
    ) -> Result<Option<Vec<Type<'db>>>, E::Error> {
        effects.nominal_checkpoint().await?;
        match ty {
            Type::EnumComplement(_) | Type::Intersection(_) | Type::NewTypeInstance(_) => effects.finite_specialized(env, ty, operator).await,
            Type::NominalInstance(instance) => {
                if effects.nominal_has_known_class(instance, KnownClass::Bool).await? {
                    return Ok(Some(effects.boolean_alternatives().await?));
                }
                if facts.known_semantics(effects.known_semantics(env, ty, operator, ComparisonSoundnessPolicy::CONSERVATIVE).await?) {
                    effects.enum_alternatives(env, instance).await
                } else {
                    Ok(None)
                }
            }
            _ => Ok(None),
        }
    }

    /// Compare values not handled by the enum, dynamic, or finite-value stages.
    #[synchronous(structural_other_sync)]
    #[capabilities(effects = EqualityEffects, facts = EqualityFacts)]
    #[passive_values(ComparisonResult::Ambiguous, KnownComparisonSemantics::Object, ComparisonSoundnessPolicy::CONSERVATIVE)]
    pub(in crate::types) async fn structural_other_with<'db, E: EqualityEffects<'db>>(
        evaluator: &mut ComparisonEvaluator<'db>, env: &ProgramEnvironment<'db>, left: Type<'db>, right: Type<'db>, branch: ComparisonBranch, operator: ComparisonOperator, facts: EqualityFacts, effects: &E,
    ) -> Result<ComparisonResult<'db>, E::Error> {
        effects.nominal_checkpoint().await?;
        match (left, right) {
            (Type::Never | Type::Divergent(_) | Type::AlwaysFalsy | Type::AlwaysTruthy | Type::ProtocolInstance(_) | Type::DataclassTransformer(_) | Type::TypeGuard(_) | Type::TypeIs(_), _)
            | (_, Type::Never | Type::Divergent(_) | Type::AlwaysFalsy | Type::AlwaysTruthy | Type::ProtocolInstance(_) | Type::DataclassTransformer(_) | Type::TypeGuard(_) | Type::TypeIs(_)) => return Ok(ComparisonResult::Ambiguous),
            (Type::Dynamic(_), _) => return effects.structural_specialized(evaluator, env, left, right, branch, operator).await,
            (_, Type::Dynamic(_)) => return Ok(ComparisonResult::Ambiguous),
            (Type::TypeVar(_) | Type::NewTypeInstance(_) | Type::Union(_), _)
            | (_, Type::TypeVar(_) | Type::NewTypeInstance(_) | Type::Union(_)) => return effects.structural_specialized(evaluator, env, left, right, branch, operator).await,
            _ => {},
        }

        // An excluded string literal rules out its runtime value only when the intersection
        // already proves that the string has literal origin.
        match (left, right) {
            (Type::Intersection(_), Type::LiteralValue(_)) | (Type::LiteralValue(_), Type::Intersection(_))
                if effects.excluded_string_literal(env, left, right).await? => return Ok(facts.result(operator, false)),
            _ => {},
        }

        match (left, right) {
            (Type::Intersection(_) | Type::LiteralValue(_), _) | (_, Type::LiteralValue(_)) => return effects.structural_specialized(evaluator, env, left, right, branch, operator).await,
            (Type::TypedDict(_), Type::TypedDict(_)) => return Ok(ComparisonResult::Ambiguous),
            (Type::TypedDict(_), other) | (other, Type::TypedDict(_)) => return Ok(match effects.comparison_semantics(evaluator, other, operator).await? {
                Some(KnownComparisonSemantics::Dict) | None => ComparisonResult::Ambiguous,
                Some(_) => facts.result(operator, false),
            }),
            (Type::ModuleLiteral(_), Type::ModuleLiteral(_)) => return Ok(facts.result(operator, effects.same_module(left, right).await?)),
            (Type::WrapperDescriptor(left_descriptor), Type::WrapperDescriptor(right_descriptor))
                if facts.same_descriptor(left_descriptor, right_descriptor) => return Ok(facts.result(operator, true)),
            (Type::KnownBoundMethod(KnownBoundMethodType::FunctionTypeDunderGet(_)), Type::KnownBoundMethod(KnownBoundMethodType::FunctionTypeDunderGet(_)))
            | (Type::KnownBoundMethod(KnownBoundMethodType::DunderCall(_)), Type::KnownBoundMethod(KnownBoundMethodType::DunderCall(_)))
                if effects.bound_method_identity(left, right).await? => return Ok(facts.result(operator, true)),
            _ => {},
        }

        if effects.identity_semantics(env, left, operator).await?
            && effects.identity_semantics(env, right, operator).await? {
            return Ok(facts.result(operator, effects.types_equal(left, right).await?));
        }
        if let (Type::NominalInstance(left_instance), Type::NominalInstance(right_instance)) = (left, right) {
            return effects.nominal_comparison(evaluator, left_instance, right_instance, operator).await;
        }
        if effects.singleton_type(env, left).await?
            && effects.equality_equivalent(env, left, right).await?
            && facts.semantics_is(effects.known_semantics(env, left, operator, ComparisonSoundnessPolicy::CONSERVATIVE).await?, KnownComparisonSemantics::Object) {
            return Ok(facts.result(operator, true));
        }
        Ok(ComparisonResult::Ambiguous)
    }

    /// Compare nominal instances when their inherited comparison implementations are known.
    ///
    /// The result is definite only when the implementations cannot compare equal, or when both types
    /// denote the same singleton, or when their fixed tuples differ in length or have a definite element comparison.
    #[synchronous(nominal_comparison_sync)]
    #[capabilities(effects = EqualityEffects, facts = EqualityFacts)]
    #[passive_values(Type::NominalInstance, ComparisonResult::Ambiguous, KnownComparisonSemantics::Object, KnownComparisonSemantics::Tuple)]
    pub(in crate::types) async fn nominal_comparison_with<'db, E: EqualityEffects<'db>>(
        evaluator: &mut ComparisonEvaluator<'db>, left_instance: NominalInstanceType<'db>, right_instance: NominalInstanceType<'db>, operator: ComparisonOperator, facts: EqualityFacts, effects: &E,
    ) -> Result<ComparisonResult<'db>, E::Error> {
        effects.nominal_checkpoint().await?;
        let env = facts.environment(evaluator);
        let left = Type::NominalInstance(left_instance);
        let right = Type::NominalInstance(right_instance);
        let Some(left_semantics) = effects.comparison_semantics(evaluator, left, operator).await? else { return Ok(ComparisonResult::Ambiguous); };
        let Some(right_semantics) = effects.comparison_semantics(evaluator, right, operator).await? else { return Ok(ComparisonResult::Ambiguous); };
        if !facts.same_semantics(left_semantics, right_semantics) {
            return effects.different_semantics(env, left, right, operator).await;
        }
        if facts.same_semantics(left_semantics, KnownComparisonSemantics::Object)
            && effects.equality_disjoint(env, left, right).await? {
            return Ok(facts.result(operator, false));
        }
        if effects.types_equal(left, right).await? && effects.singleton_type(env, left).await? {
            return Ok(facts.result(operator, true));
        }
        if facts.same_semantics(left_semantics, KnownComparisonSemantics::Tuple)
            && let Some(left_tuple) = effects.equality_tuple_spec(env, left_instance).await?
            && let Some(right_tuple) = effects.equality_tuple_spec(env, right_instance).await?
            && let Some(left_tuple) = facts.fixed_tuple(&left_tuple)
            && let Some(right_tuple) = facts.fixed_tuple(&right_tuple) {
            if !facts.same_tuple_length(left_tuple, right_tuple) { return Ok(facts.result(operator, false)); }
            let mut pairs = effects.equality_tuple_pairs(left_tuple, right_tuple).await?;
            #[passive_state]
            let mut all_equal = true;
            #[cursor_loop]
            while let Some(pair) = effects.next_equality_tuple_pair(&mut pairs).await? {
                let (left, right) = pair;
                match effects.tuple_equality(evaluator, left, right).await? {
                    Truthiness::AlwaysTrue => {},
                    Truthiness::AlwaysFalse => return Ok(facts.result(operator, false)),
                    Truthiness::Ambiguous => { all_equal = false; },
                }
            }
            if all_equal { Ok(facts.result(operator, true)) } else { Ok(ComparisonResult::Ambiguous) }
        } else {
            Ok(ComparisonResult::Ambiguous)
        }
    }

    /// Determine comparison semantics, optionally assuming that subclasses do not override the
    /// inherited comparison method.
    #[synchronous(known_semantics_sync)]
    #[capabilities(effects = EqualityEffects, facts = EqualityFacts)]
    #[passive_values(KnownComparisonSemantics::Int, KnownComparisonSemantics::Str, KnownComparisonSemantics::Bytes, KnownComparisonSemantics::Dict, KnownClass::Object)]
    pub(in crate::types) async fn known_semantics_with<'db, E: EqualityEffects<'db>>(
        env: &ProgramEnvironment<'db>, ty: Type<'db>, operator: ComparisonOperator, policy: ComparisonSoundnessPolicy, facts: EqualityFacts, effects: &E,
    ) -> Result<Option<KnownComparisonSemantics>, E::Error> {
        effects.nominal_checkpoint().await?;
        match ty {
            Type::LiteralValue(literal) => match facts.literal(literal) {
                LiteralValueTypeKind::Int(_) | LiteralValueTypeKind::Bool(_) => Ok(Some(KnownComparisonSemantics::Int)),
                LiteralValueTypeKind::String(_) | LiteralValueTypeKind::LiteralString => Ok(Some(KnownComparisonSemantics::Str)),
                LiteralValueTypeKind::Bytes(_) => Ok(Some(KnownComparisonSemantics::Bytes)),
                LiteralValueTypeKind::Enum(_) => effects.known_semantics_specialized(env, ty, operator, policy).await,
            },
            Type::TypedDict(_) => Ok(Some(KnownComparisonSemantics::Dict)),
            Type::EnumComplement(_) | Type::Intersection(_) => effects.known_semantics_specialized(env, ty, operator, policy).await,
            Type::NominalInstance(instance) => {
                if effects.nominal_is_final(env, instance).await?
                    || facts.allow_unsafe_equality(policy)
                        && (
                            // `object` can contain values whose classes define their own comparison
                            // method, so treating it as exact would incorrectly eliminate those values.
                            !effects.nominal_has_known_class(instance, KnownClass::Object).await?
                        ) {
                    effects.instance_semantics(env, ty, operator).await
                } else { Ok(None) }
            }
            Type::SpecialForm(_) | Type::KnownInstance(_) => effects.known_semantics_specialized(env, ty, operator, policy).await,
            _ => Ok(None),
        }
    }

    /// Return the builtin comparison implementation inherited by an instance.
    ///
    /// Returns `None` when lookup finds custom comparison behavior.
    #[synchronous(instance_semantics_sync)]
    #[capabilities(effects = EqualityEffects, facts = EqualityFacts)]
    #[passive_values(KnownComparisonSemantics::Tuple, KnownComparisonSemantics::Object, KnownClass::Tuple)]
    pub(in crate::types) async fn instance_semantics_with<'db, E: EqualityEffects<'db>>(
        env: &ProgramEnvironment<'db>, instance: Type<'db>, operator: ComparisonOperator, facts: EqualityFacts, effects: &E,
    ) -> Result<Option<KnownComparisonSemantics>, E::Error> {
        effects.nominal_checkpoint().await?;
        if !effects.nominal_class_available(env, instance).await? { return Ok(None); }
        let class = effects.equality_meta_type(env, instance).await?;
        let dunder = effects.equality_dunder(env, class, facts.dunder(operator)).await?;
        if facts.undefined_member(dunder) {
            if facts.inequality(operator) {
                let equality = effects.equality_dunder(env, class, "__eq__").await?;
                // `tuple.__ne__` delegates to its builtin equality implementation.
                let tuple = effects.equality_known_class(env, KnownClass::Tuple).await?;
                if effects.members_equal(equality, effects.equality_dunder(env, tuple, "__eq__").await?).await? {
                    return Ok(Some(KnownComparisonSemantics::Tuple));
                }
                if !facts.undefined_member(equality) { return Ok(None); }
            }
            return Ok(Some(KnownComparisonSemantics::Object));
        }
        let mut index = 0;
        #[cursor_loop]
        while let Some(builtin) = effects.next_builtin_semantics(&mut index).await? {
            let (known_class, semantics) = builtin;
            let class = effects.equality_known_class(env, known_class).await?;
            let member = effects.equality_dunder(env, class, facts.dunder(operator)).await?;
            if effects.same_member_implementation(dunder, member).await? {
                return Ok(Some(semantics));
            }
        }
        Ok(None)
    }

    /// Return whether two looked-up members originate from the same implementation.
    #[synchronous(same_member_implementation_sync)]
    #[capabilities(effects = EqualityEffects, facts = EqualityFacts)]
    #[passive_values()]
    pub(in crate::types) async fn same_member_implementation_with<'db, E: EqualityEffects<'db>>(
        left: PlaceAndQualifiers<'db>, right: PlaceAndQualifiers<'db>, facts: EqualityFacts, effects: &E,
    ) -> Result<bool, E::Error> {
        effects.nominal_checkpoint().await?;
        if !facts.same_qualifiers(left, right) { return Ok(false); }
        match (facts.function_member(left), facts.function_member(right)) {
            (Some(left), Some(right)) => {
                let left = effects.function_identity(left).await?;
                let right = effects.function_identity(right).await?;
                Ok(facts.same_function_literal(left, right))
            }
            _ => Ok(effects.members_equal(left, right).await?),
        }
    }

    /// Return whether `ty` is a singleton whose comparison uses object identity semantics.
    #[synchronous(identity_semantics_sync)]
    #[capabilities(effects = EqualityEffects, facts = EqualityFacts)]
    #[passive_values(KnownComparisonSemantics::Object, ComparisonSoundnessPolicy::CONSERVATIVE)]
    pub(in crate::types) async fn identity_semantics_with<'db, E: EqualityEffects<'db>>(
        env: &ProgramEnvironment<'db>, ty: Type<'db>, operator: ComparisonOperator, facts: EqualityFacts, effects: &E,
    ) -> Result<bool, E::Error> {
        effects.nominal_checkpoint().await?;
        match ty {
            Type::FunctionLiteral(_) | Type::ModuleLiteral(_) => Ok(true),
            Type::ClassLiteral(class) => {
                let instance = effects.metaclass_instance(env, class).await?;
                Ok(facts.semantics_is(effects.instance_semantics(env, instance, operator).await?, KnownComparisonSemantics::Object))
            }
            _ => Ok(effects.singleton_type(env, ty).await?
                && facts.semantics_is(effects.known_semantics(env, ty, operator, ComparisonSoundnessPolicy::CONSERVATIVE).await?, KnownComparisonSemantics::Object)),
        }
    }

    /// Returns true when a type is known to have a single inhabitant.
    #[synchronous(singleton_type_sync)]
    #[capabilities(effects = EqualityEffects, facts = EqualityFacts)]
    #[passive_values()]
    pub(in crate::types) async fn singleton_type_with<'db, E: EqualityEffects<'db>>(
        env: &ProgramEnvironment<'db>, ty: Type<'db>, facts: EqualityFacts, effects: &E,
    ) -> Result<bool, E::Error> {
        effects.nominal_checkpoint().await?;
        match ty {
            Type::Dynamic(_) | Type::Divergent(_) | Type::Never | Type::ProtocolInstance(_) | Type::SubclassOf(_) | Type::BoundSuper(_) | Type::GenericAlias(_) | Type::Callable(_) | Type::BoundMethod(_) | Type::KnownBoundMethod(_) | Type::DataclassDecorator(_) | Type::DataclassTransformer(_) | Type::PropertyInstance(_) | Type::SlotDescriptor(_) | Type::Union(_) | Type::AlwaysTruthy | Type::AlwaysFalsy | Type::TypeForm(_) | Type::TypedDict(_) => Ok(false),
            Type::LiteralValue(literal) => Ok(facts.literal_singleton(facts.literal(literal))),
            Type::FunctionLiteral(_) | Type::WrapperDescriptor(_) | Type::ClassLiteral(_) | Type::ModuleLiteral(_) | Type::KnownInstance(KnownInstanceType::Sentinel(_)) => Ok(true),
            Type::KnownInstance(_) => Ok(false),
            Type::NominalInstance(instance) => effects.singleton_nominal(instance).await,
            Type::SpecialForm(form) => Ok(facts.special_form_singleton(form)),
            Type::RecursiveVar(_) | Type::Recursive(_) | Type::TypeVar(_) | Type::Intersection(_) | Type::EnumComplement(_) | Type::TypeIs(_) | Type::TypeGuard(_) | Type::TypeAlias(_) | Type::NewTypeInstance(_) => effects.singleton_specialized(env, ty).await,
        }
    }
}

pub(super) struct OrdinaryEqualityEffects<'db> {
    pub db: &'db dyn Db,
}

impl<'db> SynchronousEqualityEffects<'db> for OrdinaryEqualityEffects<'db> {
    type Error = Infallible;
    fn checkpoint(&self) -> Result<(), Infallible> {
        Ok(())
    }
    fn clone_environment(
        &self,
        evaluator: &ComparisonEvaluator<'db>,
    ) -> Result<ProgramEnvironment<'db>, Infallible> {
        Ok(evaluator.env.clone())
    }
    fn alias(&self, ty: Type<'db>) -> Result<Type<'db>, Infallible> {
        Ok(ty.resolve_type_alias(self.db))
    }
    fn insert_active(
        &self,
        evaluator: &mut ComparisonEvaluator<'db>,
        key: ComparisonKey<'db>,
    ) -> Result<bool, Infallible> {
        Ok(evaluator.active.insert(key))
    }
    fn remove_active(
        &self,
        evaluator: &mut ComparisonEvaluator<'db>,
        key: ComparisonKey<'db>,
    ) -> Result<(), Infallible> {
        evaluator.active.remove(&key);
        Ok(())
    }
    fn evaluate(
        &self,
        evaluator: &mut ComparisonEvaluator<'db>,
        left: Type<'db>,
        right: Type<'db>,
        branch: ComparisonBranch,
        operator: ComparisonOperator,
    ) -> Result<ComparisonResult<'db>, Infallible> {
        evaluate_sync(
            evaluator,
            left,
            right,
            branch,
            operator,
            EqualityFacts,
            self,
        )
    }
    fn evaluate_once(
        &self,
        evaluator: &mut ComparisonEvaluator<'db>,
        left: Type<'db>,
        right: Type<'db>,
        branch: ComparisonBranch,
        operator: ComparisonOperator,
    ) -> Result<ComparisonResult<'db>, Infallible> {
        evaluate_once_sync(evaluator, left, right, branch, operator, self)
    }
    fn enum_comparison(
        &self,
        evaluator: &mut ComparisonEvaluator<'db>,
        left: Type<'db>,
        right: Type<'db>,
        branch: ComparisonBranch,
        operator: ComparisonOperator,
    ) -> Result<Option<ComparisonResult<'db>>, Infallible> {
        Ok(super::enums::evaluate_enum_comparison(
            evaluator, left, right, branch, operator,
        ))
    }
    fn dynamic_comparison(
        &self,
        evaluator: &mut ComparisonEvaluator<'db>,
        left: Type<'db>,
        right: Type<'db>,
        branch: ComparisonBranch,
        operator: ComparisonOperator,
    ) -> Result<Option<ComparisonResult<'db>>, Infallible> {
        dynamic_sync(
            evaluator,
            left,
            right,
            branch,
            operator,
            EqualityFacts,
            self,
        )
    }
    fn dynamic_constraint(
        &self,
        evaluator: &mut ComparisonEvaluator<'db>,
        env: &ProgramEnvironment<'db>,
        left: Type<'db>,
        right: Type<'db>,
        branch: ComparisonBranch,
        operator: ComparisonOperator,
    ) -> Result<Option<ComparisonResult<'db>>, Infallible> {
        Ok(super::evaluate_dynamic_comparison_other(
            evaluator, env, left, right, branch, operator,
        ))
    }
    fn finite_comparison(
        &self,
        evaluator: &mut ComparisonEvaluator<'db>,
        left: Type<'db>,
        right: Type<'db>,
        branch: ComparisonBranch,
        operator: ComparisonOperator,
    ) -> Result<Option<ComparisonResult<'db>>, Infallible> {
        finite_sync(evaluator, left, right, branch, operator, self)
    }
    fn finite_alternatives(
        &self,
        env: &ProgramEnvironment<'db>,
        ty: Type<'db>,
        operator: ComparisonOperator,
    ) -> Result<Option<Vec<Type<'db>>>, Infallible> {
        finite_alternatives_sync(env, ty, operator, self)
    }
    fn finite_alternatives_other(
        &self,
        env: &ProgramEnvironment<'db>,
        ty: Type<'db>,
        operator: ComparisonOperator,
    ) -> Result<Option<Vec<Type<'db>>>, Infallible> {
        Ok(super::finite_alternatives_other(self.db, env, ty, operator))
    }
    fn union_left(
        &self,
        evaluator: &mut ComparisonEvaluator<'db>,
        alternatives: Vec<Type<'db>>,
        right: Type<'db>,
        branch: ComparisonBranch,
        operator: ComparisonOperator,
    ) -> Result<ComparisonResult<'db>, Infallible> {
        Ok(super::evaluate_union_left(
            evaluator,
            &alternatives,
            right,
            branch,
            operator,
        ))
    }
    fn union_right(
        &self,
        evaluator: &mut ComparisonEvaluator<'db>,
        left: Type<'db>,
        alternatives: Vec<Type<'db>>,
        branch: ComparisonBranch,
        operator: ComparisonOperator,
    ) -> Result<ComparisonResult<'db>, Infallible> {
        Ok(super::evaluate_union_right(
            evaluator,
            left,
            &alternatives,
            branch,
            operator,
        ))
    }
    fn structural_comparison(
        &self,
        evaluator: &mut ComparisonEvaluator<'db>,
        left: Type<'db>,
        right: Type<'db>,
        branch: ComparisonBranch,
        operator: ComparisonOperator,
    ) -> Result<ComparisonResult<'db>, Infallible> {
        structural_sync(
            evaluator,
            left,
            right,
            branch,
            operator,
            EqualityFacts,
            self,
        )
    }
    fn structural_other(
        &self,
        evaluator: &mut ComparisonEvaluator<'db>,
        env: &ProgramEnvironment<'db>,
        left: Type<'db>,
        right: Type<'db>,
        branch: ComparisonBranch,
        operator: ComparisonOperator,
    ) -> Result<ComparisonResult<'db>, Infallible> {
        Ok(super::evaluate_structural_comparison_other(
            evaluator, env, left, right, branch, operator,
        ))
    }
    fn literal_equality(
        &self,
        env: &ProgramEnvironment<'db>,
        left: LiteralValueTypeKind<'db>,
        right: LiteralValueTypeKind<'db>,
        operator: ComparisonOperator,
    ) -> Result<Option<bool>, Infallible> {
        literal_equality_sync(env, left, right, operator, EqualityFacts, self)
    }
    fn literal_equality_other(
        &self,
        env: &ProgramEnvironment<'db>,
        left: LiteralValueTypeKind<'db>,
        right: LiteralValueTypeKind<'db>,
        operator: ComparisonOperator,
    ) -> Result<Option<bool>, Infallible> {
        Ok(super::known_literal_equality_other(
            self.db, env, left, right, operator,
        ))
    }
    fn string_equality(
        &self,
        left: StringLiteralType<'db>,
        right: StringLiteralType<'db>,
    ) -> Result<bool, Infallible> {
        Ok(left.value(self.db) == right.value(self.db))
    }
    fn bytes_equality(
        &self,
        left: BytesLiteralType<'db>,
        right: BytesLiteralType<'db>,
    ) -> Result<bool, Infallible> {
        Ok(left.value(self.db) == right.value(self.db))
    }
    fn narrow_literals(
        &self,
        env: &ProgramEnvironment<'db>,
        left: Type<'db>,
        right: Type<'db>,
        left_literal: LiteralValueTypeKind<'db>,
        right_literal: LiteralValueTypeKind<'db>,
        positive: bool,
    ) -> Result<ComparisonResult<'db>, Infallible> {
        Ok(super::narrow_literal_comparison(
            self.db,
            env,
            left,
            right,
            left_literal,
            right_literal,
            positive,
        ))
    }
    fn singleton(
        &self,
        evaluator: &ComparisonEvaluator<'db>,
        ty: Type<'db>,
    ) -> Result<bool, Infallible> {
        singleton_sync(evaluator, ty, EqualityFacts, self)
    }
    fn singleton_other(
        &self,
        evaluator: &ComparisonEvaluator<'db>,
        ty: Type<'db>,
    ) -> Result<bool, Infallible> {
        singleton_type_sync(&evaluator.env, ty, EqualityFacts, self)
    }
    fn comparison_semantics(
        &self,
        evaluator: &ComparisonEvaluator<'db>,
        ty: Type<'db>,
        operator: ComparisonOperator,
    ) -> Result<Option<KnownComparisonSemantics>, Infallible> {
        comparison_semantics_sync(evaluator, ty, operator, EqualityFacts, self)
    }
    fn comparison_semantics_other(
        &self,
        evaluator: &ComparisonEvaluator<'db>,
        ty: Type<'db>,
        operator: ComparisonOperator,
    ) -> Result<Option<KnownComparisonSemantics>, Infallible> {
        Ok(KnownComparisonSemantics::of_type_with_policy(
            self.db,
            &evaluator.env,
            ty,
            operator,
            evaluator.soundness_policy,
        ))
    }
    fn tuple_equality(
        &self,
        evaluator: &mut ComparisonEvaluator<'db>,
        left: Type<'db>,
        right: Type<'db>,
    ) -> Result<Truthiness, Infallible> {
        tuple_equality_sync(evaluator, left, right, EqualityFacts, self)
    }
    fn dunder_equality(
        &self,
        evaluator: &ComparisonEvaluator<'db>,
        left: Type<'db>,
        right: Type<'db>,
    ) -> Result<Option<Type<'db>>, Infallible> {
        Ok(Type::try_call_rich_comparison_dunder(
            self.db,
            &evaluator.env,
            left,
            right,
            "__eq__",
            "__eq__",
            MemberLookupPolicy::default(),
        ))
    }
    fn try_bool(
        &self,
        evaluator: &ComparisonEvaluator<'db>,
        ty: Type<'db>,
    ) -> Result<Result<Truthiness, BoolError<'db>>, Infallible> {
        Ok(ty.try_bool(self.db, &evaluator.env))
    }
    fn nominal_checkpoint(&self) -> Result<(), Infallible> {
        Ok(())
    }
    fn known_semantics(
        &self,
        env: &ProgramEnvironment<'db>,
        ty: Type<'db>,
        operator: ComparisonOperator,
        policy: ComparisonSoundnessPolicy,
    ) -> Result<Option<KnownComparisonSemantics>, Infallible> {
        known_semantics_sync(env, ty, operator, policy, EqualityFacts, self)
    }
    fn instance_semantics(
        &self,
        env: &ProgramEnvironment<'db>,
        ty: Type<'db>,
        operator: ComparisonOperator,
    ) -> Result<Option<KnownComparisonSemantics>, Infallible> {
        instance_semantics_sync(env, ty, operator, EqualityFacts, self)
    }
    fn known_semantics_specialized(
        &self,
        env: &ProgramEnvironment<'db>,
        ty: Type<'db>,
        operator: ComparisonOperator,
        policy: ComparisonSoundnessPolicy,
    ) -> Result<Option<KnownComparisonSemantics>, Infallible> {
        Ok(super::nominal_source::known_semantics_specialized(
            self.db, env, ty, operator, policy,
        ))
    }
    fn nominal_is_final(
        &self,
        env: &ProgramEnvironment<'db>,
        instance: NominalInstanceType<'db>,
    ) -> Result<bool, Infallible> {
        Ok(instance.class(self.db, env).is_final(self.db))
    }
    fn nominal_has_known_class(
        &self,
        instance: NominalInstanceType<'db>,
        class: KnownClass,
    ) -> Result<bool, Infallible> {
        Ok(instance.has_known_class(self.db, class))
    }
    fn nominal_class_available(
        &self,
        env: &ProgramEnvironment<'db>,
        ty: Type<'db>,
    ) -> Result<bool, Infallible> {
        Ok(ty.nominal_class(self.db, env).is_some())
    }
    fn equality_meta_type(
        &self,
        env: &ProgramEnvironment<'db>,
        ty: Type<'db>,
    ) -> Result<Type<'db>, Infallible> {
        Ok(ty.to_meta_type(self.db, env))
    }
    fn equality_dunder(
        &self,
        env: &ProgramEnvironment<'db>,
        ty: Type<'db>,
        name: &'static str,
    ) -> Result<PlaceAndQualifiers<'db>, Infallible> {
        Ok(super::lookup_dunder(self.db, env, ty, name))
    }
    fn equality_known_class(
        &self,
        env: &ProgramEnvironment<'db>,
        class: KnownClass,
    ) -> Result<Type<'db>, Infallible> {
        Ok(class.to_class_literal(self.db, env))
    }
    fn same_member_implementation(
        &self,
        left: PlaceAndQualifiers<'db>,
        right: PlaceAndQualifiers<'db>,
    ) -> Result<bool, Infallible> {
        same_member_implementation_sync(left, right, EqualityFacts, self)
    }
    fn function_identity(
        &self,
        function: FunctionType<'db>,
    ) -> Result<FunctionLiteral<'db>, Infallible> {
        Ok(function.literal(self.db))
    }
    fn next_builtin_semantics(
        &self,
        index: &mut usize,
    ) -> Result<Option<(KnownClass, KnownComparisonSemantics)>, Infallible> {
        Ok(super::nominal_source::next_builtin_semantics(index))
    }
    fn identity_semantics(
        &self,
        env: &ProgramEnvironment<'db>,
        ty: Type<'db>,
        operator: ComparisonOperator,
    ) -> Result<bool, Infallible> {
        identity_semantics_sync(env, ty, operator, EqualityFacts, self)
    }
    fn metaclass_instance(
        &self,
        env: &ProgramEnvironment<'db>,
        class: ClassLiteral<'db>,
    ) -> Result<Type<'db>, Infallible> {
        Ok(class.metaclass_instance_type(self.db, env))
    }
    fn singleton_type(
        &self,
        env: &ProgramEnvironment<'db>,
        ty: Type<'db>,
    ) -> Result<bool, Infallible> {
        singleton_type_sync(env, ty, EqualityFacts, self)
    }
    fn singleton_nominal(&self, instance: NominalInstanceType<'db>) -> Result<bool, Infallible> {
        Ok(instance.is_singleton(self.db))
    }
    fn singleton_specialized(
        &self,
        env: &ProgramEnvironment<'db>,
        ty: Type<'db>,
    ) -> Result<bool, Infallible> {
        Ok(ty.is_singleton(self.db, env))
    }
    fn finite_specialized(
        &self,
        env: &ProgramEnvironment<'db>,
        ty: Type<'db>,
        operator: ComparisonOperator,
    ) -> Result<Option<Vec<Type<'db>>>, Infallible> {
        Ok(super::nominal_source::finite_specialized(
            self.db, env, ty, operator,
        ))
    }
    fn boolean_alternatives(&self) -> Result<Vec<Type<'db>>, Infallible> {
        Ok(vec![Type::bool_literal(true), Type::bool_literal(false)])
    }
    fn enum_alternatives(
        &self,
        env: &ProgramEnvironment<'db>,
        instance: NominalInstanceType<'db>,
    ) -> Result<Option<Vec<Type<'db>>>, Infallible> {
        Ok(crate::types::enums::enum_member_literals(
            self.db,
            instance.class_literal(self.db, env),
            None,
        )
        .map(Iterator::collect))
    }
    fn structural_specialized(
        &self,
        evaluator: &mut ComparisonEvaluator<'db>,
        env: &ProgramEnvironment<'db>,
        left: Type<'db>,
        right: Type<'db>,
        branch: ComparisonBranch,
        operator: ComparisonOperator,
    ) -> Result<ComparisonResult<'db>, Infallible> {
        Ok(super::nominal_source::structural_specialized(
            evaluator, env, left, right, branch, operator,
        ))
    }
    fn excluded_string_literal(
        &self,
        env: &ProgramEnvironment<'db>,
        left: Type<'db>,
        right: Type<'db>,
    ) -> Result<bool, Infallible> {
        Ok(super::nominal_source::excluded_string_literal(
            self.db, env, left, right,
        ))
    }
    fn same_module(&self, left: Type<'db>, right: Type<'db>) -> Result<bool, Infallible> {
        Ok(super::nominal_source::same_module(self.db, left, right))
    }
    fn bound_method_identity(&self, left: Type<'db>, right: Type<'db>) -> Result<bool, Infallible> {
        Ok(super::nominal_source::bound_method_identity(
            self.db, left, right,
        ))
    }
    fn equality_equivalent(
        &self,
        env: &ProgramEnvironment<'db>,
        left: Type<'db>,
        right: Type<'db>,
    ) -> Result<bool, Infallible> {
        Ok(left.is_equivalent_to(self.db, env, right))
    }
    fn equality_disjoint(
        &self,
        env: &ProgramEnvironment<'db>,
        left: Type<'db>,
        right: Type<'db>,
    ) -> Result<bool, Infallible> {
        Ok(left.is_disjoint_from(self.db, env, right))
    }
    fn different_semantics(
        &self,
        env: &ProgramEnvironment<'db>,
        left: Type<'db>,
        right: Type<'db>,
        operator: ComparisonOperator,
    ) -> Result<ComparisonResult<'db>, Infallible> {
        Ok(super::compare_different_semantics(
            self.db, env, left, right, operator,
        ))
    }
    fn nominal_comparison(
        &self,
        evaluator: &mut ComparisonEvaluator<'db>,
        left: NominalInstanceType<'db>,
        right: NominalInstanceType<'db>,
        operator: ComparisonOperator,
    ) -> Result<ComparisonResult<'db>, Infallible> {
        nominal_comparison_sync(evaluator, left, right, operator, EqualityFacts, self)
    }
    fn equality_tuple_spec(
        &self,
        env: &ProgramEnvironment<'db>,
        instance: NominalInstanceType<'db>,
    ) -> Result<Option<Cow<'db, TupleSpec<'db>>>, Infallible> {
        Ok(instance.tuple_spec(self.db, env))
    }
    fn equality_tuple_pairs<'tuple>(
        &self,
        left: &'tuple FixedLengthTuple<Type<'db>>,
        right: &'tuple FixedLengthTuple<Type<'db>>,
    ) -> Result<NominalTuplePairs<'tuple, 'db>, Infallible> {
        Ok(super::nominal_source::tuple_pairs(left, right))
    }
    fn next_equality_tuple_pair(
        &self,
        pairs: &mut NominalTuplePairs<'_, 'db>,
    ) -> Result<Option<(Type<'db>, Type<'db>)>, Infallible> {
        Ok(pairs.next())
    }
    fn types_equal(&self, left: Type<'db>, right: Type<'db>) -> Result<bool, Infallible> {
        Ok(left == right)
    }
    fn members_equal(
        &self,
        left: PlaceAndQualifiers<'db>,
        right: PlaceAndQualifiers<'db>,
    ) -> Result<bool, Infallible> {
        Ok(left == right)
    }
}
