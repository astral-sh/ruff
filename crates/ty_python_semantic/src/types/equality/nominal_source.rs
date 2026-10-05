//! Ordinary children of the shared equality dispatch and borrowed tuple iteration.

use super::{
    ComparisonBranch, ComparisonEvaluator, ComparisonOperator, ComparisonResult,
    ComparisonSoundnessPolicy, KnownComparisonSemantics, LiteralOperand, all_values_compare_equal,
    compare_literal_to_other, evaluate_intersection_left, evaluate_union_left,
    evaluate_union_right, finite_alternatives,
};
use crate::types::tuple::FixedLengthTuple;
use crate::types::{
    IntersectionBuilder, KnownBoundMethodType, KnownClass, Type, TypeVarBoundOrConstraints,
};
use crate::{Db, ProgramEnvironment};

/// Borrowed element pairs for left-to-right equality comparison of fixed tuples.
pub(in crate::types) type NominalTuplePairs<'tuple, 'db> = std::iter::Zip<
    std::iter::Copied<std::slice::Iter<'tuple, Type<'db>>>,
    std::iter::Copied<std::slice::Iter<'tuple, Type<'db>>>,
>;

/// Creates a borrowed left-to-right cursor over paired fixed-tuple elements.
pub(in crate::types) fn tuple_pairs<'tuple, 'db>(
    left: &'tuple FixedLengthTuple<Type<'db>>,
    right: &'tuple FixedLengthTuple<Type<'db>>,
) -> NominalTuplePairs<'tuple, 'db> {
    left.all_elements()
        .iter()
        .copied()
        .zip(right.all_elements().iter().copied())
}

/// Compare structural families whose children use their ordinary specialized algorithms.
pub(super) fn structural_specialized<'db>(
    evaluator: &mut ComparisonEvaluator<'db>,
    env: &ProgramEnvironment<'db>,
    left: Type<'db>,
    right: Type<'db>,
    branch: ComparisonBranch,
    operator: ComparisonOperator,
) -> ComparisonResult<'db> {
    let db = evaluator.db;

    match (left, right) {
        (Type::Dynamic(_), other) => {
            if !operator.condition_expects_equality(branch)
                && all_values_compare_equal(evaluator, other, operator)
            {
                ComparisonResult::CanNarrow(
                    IntersectionBuilder::new(db, env)
                        .add_positive(left)
                        .add_negative(other)
                        .build(),
                )
            } else {
                ComparisonResult::Ambiguous
            }
        }

        // A constrained TypeVar selects one constraint for the entire specialization, so each
        // alternative can be checked independently without losing that correlation.
        (Type::TypeVar(left_var), Type::TypeVar(right_var))
            if left_var.is_same_typevar_as(db, right_var)
                && let Some(TypeVarBoundOrConstraints::Constraints(constraints)) =
                    left_var.typevar(db).bound_or_constraints(db, env)
                && constraints.elements(db).iter().all(|constraint| {
                    all_values_compare_equal(evaluator, *constraint, operator)
                }) =>
        {
            operator.result_from_equality(true)
        }
        (Type::TypeVar(var), other) => match var.typevar(db).bound_or_constraints(db, env) {
            None => ComparisonResult::Ambiguous,
            Some(TypeVarBoundOrConstraints::UpperBound(_)) => {
                if !operator.condition_expects_equality(branch)
                    && all_values_compare_equal(evaluator, other, operator)
                {
                    ComparisonResult::CanNarrow(other.negate(db, env))
                } else {
                    ComparisonResult::Ambiguous
                }
            }
            Some(TypeVarBoundOrConstraints::Constraints(constraints)) => {
                evaluator.evaluate(constraints.as_type(db, env), other, branch, operator)
            }
        },
        (other, Type::TypeVar(var)) => match var.typevar(db).bound_or_constraints(db, env) {
            Some(TypeVarBoundOrConstraints::Constraints(constraints)) => {
                evaluator.evaluate(other, constraints.as_type(db, env), branch, operator)
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

        _ => ComparisonResult::Ambiguous,
    }
}

/// Expand finite enum-related alternatives after the shared outer type dispatch.
pub(super) fn finite_specialized<'db>(
    db: &'db dyn Db,
    env: &ProgramEnvironment<'db>,
    ty: Type<'db>,
    operator: ComparisonOperator,
) -> Option<Vec<Type<'db>>> {
    match ty {
        Type::EnumComplement(complement) => {
            KnownComparisonSemantics::of_type(db, env, ty, operator)
                .is_some()
                .then(|| complement.remaining_literal_types(db, env))
        }
        Type::Intersection(intersection) => {
            let (comparison_type, complement) = if let Some(complement) =
                intersection.enum_complement(db, env)
            {
                (ty, complement)
            } else {
                if !intersection.positive(db).iter().any(|positive| {
                    matches!(positive.resolve_type_alias(db), Type::NewTypeInstance(_))
                }) {
                    return None;
                }

                let expanded = intersection.with_expanded_typevars_and_newtypes(db, env);
                let complement = match expanded {
                    Type::LiteralValue(literal) if literal.is_enum() => {
                        return KnownComparisonSemantics::of_type(db, env, expanded, operator)
                            .is_some()
                            .then(|| vec![expanded]);
                    }
                    Type::EnumComplement(complement) => complement,
                    Type::Intersection(intersection) => intersection.enum_complement(db, env)?,
                    _ => return None,
                };
                (expanded, complement)
            };
            KnownComparisonSemantics::of_type(db, env, comparison_type, operator)
                .is_some()
                .then(|| complement.remaining_literal_types(db, env))
        }
        Type::NewTypeInstance(newtype) => {
            let base = newtype.concrete_base_type(db);
            if base.is_enum(db, env) {
                finite_alternatives(db, env, base, operator)
            } else {
                None
            }
        }
        _ => None,
    }
}

/// Classify comparison behavior for non-nominal families that require specialized children.
pub(super) fn known_semantics_specialized<'db>(
    db: &'db dyn Db,
    env: &ProgramEnvironment<'db>,
    ty: Type<'db>,
    operator: ComparisonOperator,
    soundness_policy: ComparisonSoundnessPolicy,
) -> Option<KnownComparisonSemantics> {
    match ty {
        Type::LiteralValue(literal) => {
            KnownComparisonSemantics::of_literal(db, env, literal.kind(), operator)
        }
        Type::EnumComplement(complement) => KnownComparisonSemantics::of_instance(
            db,
            env,
            complement.enum_class(db).to_non_generic_instance(db, env),
            operator,
        ),
        Type::Intersection(intersection)
            if let Some(complement) = intersection.enum_complement(db, env) =>
        {
            let instance = complement.enum_class(db).to_non_generic_instance(db, env);
            KnownComparisonSemantics::of_instance(db, env, instance, operator)
        }
        Type::Intersection(intersection) => {
            let mut semantics = intersection.positive(db).iter().map(|element| {
                KnownComparisonSemantics::of_type_with_policy(
                    db,
                    env,
                    *element,
                    operator,
                    soundness_policy,
                )
            });
            let first = semantics.next().flatten()?;
            semantics
                .all(|semantics| semantics == Some(first))
                .then_some(first)
        }
        Type::SpecialForm(special_form) => KnownComparisonSemantics::of_type_with_policy(
            db,
            env,
            special_form.instance_fallback(db, env),
            operator,
            soundness_policy,
        ),
        Type::KnownInstance(instance) => KnownComparisonSemantics::of_instance(
            db,
            env,
            instance.instance_fallback(db, env),
            operator,
        ),
        _ => None,
    }
}

/// Check whether an intersection excludes a string literal with proven literal origin.
pub(super) fn excluded_string_literal<'db>(
    db: &'db dyn Db,
    env: &ProgramEnvironment<'db>,
    left: Type<'db>,
    right: Type<'db>,
) -> bool {
    match (left, right) {
        (Type::Intersection(intersection), Type::LiteralValue(literal))
        | (Type::LiteralValue(literal), Type::Intersection(intersection)) => {
            literal.is_string()
                && intersection
                    .positive(db)
                    .iter()
                    .any(|element| element.is_subtype_of(db, env, Type::literal_string()))
                && Type::Intersection(intersection).is_disjoint_from(
                    db,
                    env,
                    Type::LiteralValue(literal),
                )
        }
        _ => false,
    }
}

/// Compare the underlying module identities after the shared module-pair dispatch.
pub(super) fn same_module(db: &dyn Db, left: Type<'_>, right: Type<'_>) -> bool {
    match (left, right) {
        (Type::ModuleLiteral(left), Type::ModuleLiteral(right)) => {
            left.module(db) == right.module(db)
        }
        _ => false,
    }
}

/// Recognize equal builtin bound-method wrappers around a function literal.
pub(super) fn bound_method_identity(db: &dyn Db, left: Type<'_>, right: Type<'_>) -> bool {
    match (left, right) {
        (
            Type::KnownBoundMethod(KnownBoundMethodType::FunctionTypeDunderGet(left)),
            Type::KnownBoundMethod(KnownBoundMethodType::FunctionTypeDunderGet(right)),
        )
        | (
            Type::KnownBoundMethod(KnownBoundMethodType::DunderCall(left)),
            Type::KnownBoundMethod(KnownBoundMethodType::DunderCall(right)),
        ) => left.inner(db).is_function_literal() && left == right,
        _ => false,
    }
}

const BUILTIN_COMPARISON_SEMANTICS: &[(KnownClass, KnownComparisonSemantics); 5] = &[
    (KnownClass::Int, KnownComparisonSemantics::Int),
    (KnownClass::Str, KnownComparisonSemantics::Str),
    (KnownClass::Bytes, KnownComparisonSemantics::Bytes),
    (KnownClass::Tuple, KnownComparisonSemantics::Tuple),
    (KnownClass::Dict, KnownComparisonSemantics::Dict),
];

/// Visit builtin comparison implementations in their ordinary lookup order.
pub(in crate::types) fn next_builtin_semantics(
    index: &mut usize,
) -> Option<(KnownClass, KnownComparisonSemantics)> {
    let builtin = BUILTIN_COMPARISON_SEMANTICS.get(*index).copied();
    if builtin.is_some() {
        *index += 1;
    }
    builtin
}
