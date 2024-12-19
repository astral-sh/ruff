use ruff_index::IndexVec;

use crate::semantic_index::ScopedVisibilityConstraintId;
use crate::semantic_index::{
    ast_ids::HasScopedExpressionId,
    constraint::{Constraint, ConstraintNode, PatternConstraintKind},
};
use crate::types::{infer_expression_types, Truthiness};
use crate::Db;

const MAX_RECURSION_DEPTH: usize = 10;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum VisibilityConstraint<'db> {
    AlwaysTrue,
    Ambiguous,
    VisibleIf(Constraint<'db>),
    VisibleIfNot(ScopedVisibilityConstraintId),
    KleeneAnd(ScopedVisibilityConstraintId, ScopedVisibilityConstraintId),
    KleeneOr(ScopedVisibilityConstraintId, ScopedVisibilityConstraintId),
}

#[derive(Debug, PartialEq, Eq)]
pub(crate) struct VisibilityConstraints<'db> {
    constraints: IndexVec<ScopedVisibilityConstraintId, VisibilityConstraint<'db>>,
}

impl<'db> VisibilityConstraints<'db> {
    pub(crate) fn new() -> Self {
        Self {
            constraints: IndexVec::from_iter([VisibilityConstraint::AlwaysTrue]),
        }
    }

    pub(crate) fn add(
        &mut self,
        constraint: VisibilityConstraint<'db>,
    ) -> ScopedVisibilityConstraintId {
        self.constraints.push(constraint)
    }

    pub(crate) fn add_or_constraint(
        &mut self,
        a: ScopedVisibilityConstraintId,
        b: ScopedVisibilityConstraintId,
    ) -> ScopedVisibilityConstraintId {
        match (&self.constraints[a], &self.constraints[b]) {
            (_, VisibilityConstraint::VisibleIfNot(id)) if a == *id => {
                ScopedVisibilityConstraintId::ALWAYS_TRUE
            }
            (VisibilityConstraint::VisibleIfNot(id), _) if *id == b => {
                ScopedVisibilityConstraintId::ALWAYS_TRUE
            }
            _ => self.add(VisibilityConstraint::KleeneOr(a, b)),
        }
    }

    pub(crate) fn add_and_constraint(
        &mut self,
        a: ScopedVisibilityConstraintId,
        b: ScopedVisibilityConstraintId,
    ) -> ScopedVisibilityConstraintId {
        if a == ScopedVisibilityConstraintId::ALWAYS_TRUE {
            b
        } else if b == ScopedVisibilityConstraintId::ALWAYS_TRUE {
            a
        } else {
            self.add(VisibilityConstraint::KleeneAnd(a, b))
        }
    }

    /// Analyze the statically known visibility for a given visibility constraint.
    pub(crate) fn evaluate(&self, db: &'db dyn Db, id: ScopedVisibilityConstraintId) -> Truthiness {
        self.evaluate_impl(db, id, MAX_RECURSION_DEPTH)
    }

    fn evaluate_impl(
        &self,
        db: &'db dyn Db,
        id: ScopedVisibilityConstraintId,
        max_depth: usize,
    ) -> Truthiness {
        if max_depth == 0 {
            return Truthiness::Ambiguous;
        }

        let visibility_constraint = &self.constraints[id];
        match visibility_constraint {
            VisibilityConstraint::AlwaysTrue => Truthiness::AlwaysTrue,
            VisibilityConstraint::Ambiguous => Truthiness::Ambiguous,
            VisibilityConstraint::VisibleIf(constraint) => Self::analyze_single(db, constraint),
            VisibilityConstraint::VisibleIfNot(negated) => {
                self.evaluate_impl(db, *negated, max_depth - 1).negate()
            }
            VisibilityConstraint::KleeneAnd(lhs, rhs) => {
                let lhs = self.evaluate_impl(db, *lhs, max_depth - 1);

                if lhs == Truthiness::AlwaysFalse {
                    return Truthiness::AlwaysFalse;
                }

                let rhs = self.evaluate_impl(db, *rhs, max_depth - 1);

                if rhs == Truthiness::AlwaysFalse {
                    Truthiness::AlwaysFalse
                } else if lhs == Truthiness::AlwaysTrue && rhs == Truthiness::AlwaysTrue {
                    Truthiness::AlwaysTrue
                } else {
                    Truthiness::Ambiguous
                }
            }
            VisibilityConstraint::KleeneOr(lhs_id, rhs_id) => {
                let lhs = self.evaluate_impl(db, *lhs_id, max_depth - 1);

                if lhs == Truthiness::AlwaysTrue {
                    return Truthiness::AlwaysTrue;
                }

                let rhs = self.evaluate_impl(db, *rhs_id, max_depth - 1);

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
}
