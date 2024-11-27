use ruff_index::IndexVec;

use crate::semantic_index::{
    ast_ids::HasScopedExpressionId,
    constraint::{Constraint, ConstraintNode, PatternConstraintKind},
    visibility_constraint::VisibilityConstraintRef,
    ScopedConstraintId, ScopedVisibilityConstraintId,
};
use crate::types::{infer_expression_types, Truthiness};
use crate::Db;

/// Analyze the statically known visibility for a given visibility constraint.
pub(crate) fn static_visibility<'db>(
    db: &'db dyn Db,
    all_constraints: &IndexVec<ScopedConstraintId, Constraint<'db>>,
    all_visibility_constraints: &IndexVec<ScopedVisibilityConstraintId, VisibilityConstraintRef>,
    visibility_constraint_id: ScopedVisibilityConstraintId,
) -> Truthiness {
    let visibility_constraint = &all_visibility_constraints[visibility_constraint_id];
    match visibility_constraint {
        VisibilityConstraintRef::Single(id) => {
            let constraint = &all_constraints[*id];

            match constraint.node {
                ConstraintNode::Expression(test_expr) => {
                    let inference = infer_expression_types(db, test_expr);
                    let scope = test_expr.scope(db);
                    let ty = inference
                        .expression_ty(test_expr.node_ref(db).scoped_expression_id(db, scope));

                    ty.bool(db).negate_if(!constraint.is_positive)
                }
                ConstraintNode::Pattern(inner) => match inner.kind(db) {
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
                            Truthiness::from(subject_ty.is_equivalent_to(db, value_ty))
                        } else {
                            Truthiness::Ambiguous
                        }
                    }
                    PatternConstraintKind::Singleton(_) | PatternConstraintKind::Unsupported => {
                        Truthiness::Ambiguous
                    }
                },
            }
        }
        VisibilityConstraintRef::Negated(visibility_constraint_id) => static_visibility(
            db,
            all_constraints,
            all_visibility_constraints,
            *visibility_constraint_id,
        )
        .negate(),
        VisibilityConstraintRef::None => Truthiness::AlwaysTrue,
        VisibilityConstraintRef::And(lhs_id, rhs_id) => {
            let lhs = static_visibility(db, all_constraints, all_visibility_constraints, *lhs_id);
            let rhs = static_visibility(db, all_constraints, all_visibility_constraints, *rhs_id);

            if lhs == Truthiness::AlwaysFalse || rhs == Truthiness::AlwaysFalse {
                Truthiness::AlwaysFalse
            } else if lhs == Truthiness::AlwaysTrue && rhs == Truthiness::AlwaysTrue {
                Truthiness::AlwaysTrue
            } else {
                Truthiness::Ambiguous
            }
        }
        VisibilityConstraintRef::Or(lhs_id, rhs_id) => {
            let lhs = static_visibility(db, all_constraints, all_visibility_constraints, *lhs_id);
            let rhs = static_visibility(db, all_constraints, all_visibility_constraints, *rhs_id);

            if lhs == Truthiness::AlwaysFalse && rhs == Truthiness::AlwaysFalse {
                Truthiness::AlwaysFalse
            } else if lhs == Truthiness::AlwaysTrue || rhs == Truthiness::AlwaysTrue {
                Truthiness::AlwaysTrue
            } else {
                Truthiness::Ambiguous
            }
        }
    }
}
