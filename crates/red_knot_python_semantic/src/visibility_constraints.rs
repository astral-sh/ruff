use crate::semantic_index::{
    ast_ids::HasScopedExpressionId,
    constraint::{Constraint, ConstraintNode, PatternConstraintKind},
};
use crate::types::{infer_expression_types, Truthiness};
use crate::Db;

const MAX_RECURSION_DEPTH: usize = 10;

#[salsa::interned]
pub(crate) struct VisibilityConstraint<'db> {
    kind: VisibilityConstraintKind<'db>,
}

impl<'db> VisibilityConstraint<'db> {
    pub(crate) fn always_true(db: &'db dyn Db) -> Self {
        Self::new(db, VisibilityConstraintKind::AlwaysTrue)
    }

    pub(crate) fn ambiguous(db: &'db dyn Db) -> Self {
        Self::new(db, VisibilityConstraintKind::Ambiguous)
    }

    pub(crate) fn visible_if(db: &'db dyn Db, constraint: Constraint<'db>) -> Self {
        Self::new(db, VisibilityConstraintKind::VisibleIf(constraint))
    }

    pub(crate) fn visible_if_not(db: &'db dyn Db, constraint: VisibilityConstraint<'db>) -> Self {
        Self::new(db, VisibilityConstraintKind::VisibleIfNot(constraint))
    }

    pub(crate) fn kleene_and(
        db: &'db dyn Db,
        lhs: VisibilityConstraint<'db>,
        rhs: VisibilityConstraint<'db>,
    ) -> Self {
        Self::new(db, VisibilityConstraintKind::KleeneAnd(lhs, rhs))
    }

    pub(crate) fn kleene_or(
        db: &'db dyn Db,
        lhs: VisibilityConstraint<'db>,
        rhs: VisibilityConstraint<'db>,
    ) -> Self {
        Self::new(db, VisibilityConstraintKind::KleeneOr(lhs, rhs))
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(crate) enum VisibilityConstraintKind<'db> {
    AlwaysTrue,
    Ambiguous,
    VisibleIf(Constraint<'db>),
    VisibleIfNot(VisibilityConstraint<'db>),
    KleeneAnd(VisibilityConstraint<'db>, VisibilityConstraint<'db>),
    KleeneOr(VisibilityConstraint<'db>, VisibilityConstraint<'db>),
}

/// Analyze the statically known visibility for a given visibility constraint.
pub(crate) fn evaluate<'db>(db: &'db dyn Db, constraint: VisibilityConstraint<'db>) -> Truthiness {
    evaluate_impl(db, constraint, MAX_RECURSION_DEPTH)
}

fn evaluate_impl<'db>(
    db: &'db dyn Db,
    constraint: VisibilityConstraint<'db>,
    max_depth: usize,
) -> Truthiness {
    if max_depth == 0 {
        return Truthiness::Ambiguous;
    }

    match constraint.kind(db) {
        VisibilityConstraintKind::AlwaysTrue => Truthiness::AlwaysTrue,
        VisibilityConstraintKind::Ambiguous => Truthiness::Ambiguous,
        VisibilityConstraintKind::VisibleIf(constraint) => analyze_single(db, &constraint),
        VisibilityConstraintKind::VisibleIfNot(negated) => {
            evaluate_impl(db, negated, max_depth - 1).negate()
        }
        VisibilityConstraintKind::KleeneAnd(lhs, rhs) => {
            let lhs = evaluate_impl(db, lhs, max_depth - 1);

            if lhs == Truthiness::AlwaysFalse {
                return Truthiness::AlwaysFalse;
            }

            let rhs = evaluate_impl(db, rhs, max_depth - 1);

            if rhs == Truthiness::AlwaysFalse {
                Truthiness::AlwaysFalse
            } else if lhs == Truthiness::AlwaysTrue && rhs == Truthiness::AlwaysTrue {
                Truthiness::AlwaysTrue
            } else {
                Truthiness::Ambiguous
            }
        }
        VisibilityConstraintKind::KleeneOr(lhs, rhs) => {
            let lhs = evaluate_impl(db, lhs, max_depth - 1);

            if lhs == Truthiness::AlwaysTrue {
                return Truthiness::AlwaysTrue;
            }

            let rhs = evaluate_impl(db, rhs, max_depth - 1);

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

fn analyze_single(db: &dyn Db, constraint: &Constraint) -> Truthiness {
    match constraint.node {
        ConstraintNode::Expression(test_expr) => {
            let inference = infer_expression_types(db, test_expr);
            let scope = test_expr.scope(db);
            let ty =
                inference.expression_ty(test_expr.node_ref(db).scoped_expression_id(db, scope));

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
                let value_ty =
                    inference.expression_ty(value.node_ref(db).scoped_expression_id(db, scope));

                if subject_ty.is_single_valued(db) {
                    let truthiness = Truthiness::from(subject_ty.is_equivalent_to(db, value_ty));

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
