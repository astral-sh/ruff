use crate::semantic_index::{
    ast_ids::HasScopedExpressionId,
    branching_condition::BranchingCondition,
    constraint::{Constraint, ConstraintNode, PatternConstraintKind},
    BranchingConditionsIterator,
};
use crate::types::{infer_expression_types, Truthiness};
use crate::Db;

/// The result of a static-truthiness analysis.
///
/// Consider the following example:
/// ```py
/// a = 1
/// if True:
///     b = 1
///     if <bool>:
///         c = 1
///         if False:
///             d = 1
/// ```
///
/// Given an iterator over the branching conditions for each of these bindings, we would get:
/// ```
/// - a: {any_always_false: false, all_always_true: true,  at_least_one_condition: false}
/// - b: {any_always_false: false, all_always_true: true,  at_least_one_condition: true}
/// - c: {any_always_false: false, all_always_true: false, at_least_one_condition: true}
/// - d: {any_always_false: true,  all_always_true: false, at_least_one_condition: true}
/// ```
pub(crate) struct StaticTruthiness {
    /// Is any of the branching conditions always false? (false if there are no conditions)
    pub(crate) any_always_false: bool,
    /// Are all of the branching conditions always true? (true if there are no conditions)
    pub(crate) all_always_true: bool,
    /// Is there at least one branching condition?
    pub(crate) at_least_one_condition: bool,
}

impl StaticTruthiness {
    /// Analyze the (statically known) truthiness for a list of branching conditions.
    pub(crate) fn analyze<'db>(
        db: &'db dyn Db,
        branching_conditions: BranchingConditionsIterator<'_, 'db>,
    ) -> Self {
        let mut result = Self {
            any_always_false: false,
            all_always_true: true,
            at_least_one_condition: false,
        };

        for condition in branching_conditions {
            let truthiness = match condition {
                BranchingCondition::ConditionalOn(Constraint {
                    node: ConstraintNode::Expression(test_expr),
                    is_positive,
                }) => {
                    let inference = infer_expression_types(db, test_expr);
                    let scope = test_expr.scope(db);
                    let ty = inference
                        .expression_ty(test_expr.node_ref(db).scoped_expression_id(db, scope));

                    ty.bool(db).negate_if(!is_positive)
                }
                BranchingCondition::ConditionalOn(Constraint {
                    node: ConstraintNode::Pattern(inner),
                    ..
                }) => match inner.kind(db) {
                    PatternConstraintKind::Value(value) => {
                        let subject_expression = inner.subject(db);
                        let inference = infer_expression_types(db, *subject_expression);
                        let scope = subject_expression.scope(db);
                        let subject_ty = inference.expression_ty(
                            subject_expression
                                .node_ref(db)
                                .scoped_expression_id(db, scope),
                        );

                        let inference = infer_expression_types(db, *value);
                        let scope = value.scope(db);
                        let value_ty = inference
                            .expression_ty(value.node_ref(db).scoped_expression_id(db, scope));

                        if subject_ty.is_single_valued(db) {
                            Truthiness::from_bool(subject_ty.is_equivalent_to(db, value_ty))
                        } else {
                            Truthiness::Ambiguous
                        }
                    }
                    PatternConstraintKind::Singleton(_) | PatternConstraintKind::Unsupported => {
                        Truthiness::Ambiguous
                    }
                },
                BranchingCondition::Ambiguous => Truthiness::Ambiguous,
            };

            result.any_always_false |= truthiness.is_always_false();
            result.all_always_true &= truthiness.is_always_true();
            result.at_least_one_condition = true;
        }

        result
    }
}
