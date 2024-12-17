use ruff_index::IndexVec;

use crate::semantic_index::{
    ast_ids::HasScopedExpressionId,
    constraint::{Constraint, ConstraintNode, PatternConstraintKind},
    visibility_constraint::VisibilityConstraintRef,
    ScopedConstraintId, ScopedVisibilityConstraintId,
};
use crate::types::{infer_expression_types, Truthiness};
use crate::Db;

const MAX_RECURSION_DEPTH: usize = 10;

/// Analyze the statically known visibility for a given visibility constraint.
pub(crate) fn analyze<'db>(
    db: &'db dyn Db,
    all_constraints: &IndexVec<ScopedConstraintId, Constraint<'db>>,
    all_visibility_constraints: &IndexVec<ScopedVisibilityConstraintId, VisibilityConstraintRef>,
    visibility_constraint_id: ScopedVisibilityConstraintId,
) -> Truthiness {
    analyze_impl(
        db,
        all_constraints,
        all_visibility_constraints,
        visibility_constraint_id,
        MAX_RECURSION_DEPTH,
    )
}

fn analyze_impl<'db>(
    db: &'db dyn Db,
    all_constraints: &IndexVec<ScopedConstraintId, Constraint<'db>>,
    all_visibility_constraints: &IndexVec<ScopedVisibilityConstraintId, VisibilityConstraintRef>,
    visibility_constraint_id: ScopedVisibilityConstraintId,
    max_depth: usize,
) -> Truthiness {
    if max_depth == 0 {
        return Truthiness::Ambiguous;
    }

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
                    PatternConstraintKind::Value(value, guard) => {
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
                            let truthiness =
                                Truthiness::from(subject_ty.is_equivalent_to(db, value_ty));

                            if truthiness.is_always_true() && guard.is_some() {
                                // Fall back to ambiguous, the guard might change the result.
                                Truthiness::Ambiguous
                            } else {
                                truthiness
                            }
                        } else {
                            Truthiness::Ambiguous
                        }
                    }
                    PatternConstraintKind::Singleton(..) | PatternConstraintKind::Unsupported => {
                        Truthiness::Ambiguous
                    }
                },
            }
        }
        VisibilityConstraintRef::Negated(visibility_constraint_id) => analyze_impl(
            db,
            all_constraints,
            all_visibility_constraints,
            *visibility_constraint_id,
            max_depth - 1,
        )
        .negate(),
        VisibilityConstraintRef::None => Truthiness::AlwaysTrue,
        VisibilityConstraintRef::Sequence(lhs_id, rhs_id) => {
            let lhs = analyze_impl(
                db,
                all_constraints,
                all_visibility_constraints,
                *lhs_id,
                max_depth - 1,
            );

            if lhs == Truthiness::AlwaysFalse {
                return Truthiness::AlwaysFalse;
            }

            let rhs = analyze_impl(
                db,
                all_constraints,
                all_visibility_constraints,
                *rhs_id,
                max_depth - 1,
            );

            if rhs == Truthiness::AlwaysFalse {
                Truthiness::AlwaysFalse
            } else if lhs == Truthiness::AlwaysTrue && rhs == Truthiness::AlwaysTrue {
                Truthiness::AlwaysTrue
            } else {
                Truthiness::Ambiguous
            }
        }
        VisibilityConstraintRef::Merged(lhs_id, rhs_id) => {
            let lhs = analyze_impl(
                db,
                all_constraints,
                all_visibility_constraints,
                *lhs_id,
                max_depth - 1,
            );

            if lhs == Truthiness::AlwaysTrue {
                return Truthiness::AlwaysTrue;
            }

            let rhs = analyze_impl(
                db,
                all_constraints,
                all_visibility_constraints,
                *rhs_id,
                max_depth - 1,
            );

            if rhs == Truthiness::AlwaysTrue {
                Truthiness::AlwaysTrue
            } else if lhs == Truthiness::AlwaysFalse && rhs == Truthiness::AlwaysFalse {
                Truthiness::AlwaysFalse
            } else {
                Truthiness::Ambiguous
            }
        }
    }
}
