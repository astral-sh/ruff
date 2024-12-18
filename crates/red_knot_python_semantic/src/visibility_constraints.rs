use ruff_index::IndexVec;

use crate::semantic_index::{
    ast_ids::HasScopedExpressionId,
    constraint::{Constraint, ConstraintNode, PatternConstraintKind},
};
use crate::semantic_index::{ScopedConstraintId, ScopedVisibilityConstraintId};
use crate::types::{infer_expression_types, Truthiness};
use crate::Db;

const MAX_RECURSION_DEPTH: usize = 10;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum VisibilityConstraintRef {
    None,
    Single(ScopedConstraintId),
    Negated(ScopedVisibilityConstraintId),
    Sequence(ScopedVisibilityConstraintId, ScopedVisibilityConstraintId),
    Merged(ScopedVisibilityConstraintId, ScopedVisibilityConstraintId),
}

#[derive(Debug, PartialEq, Eq)]
pub(crate) struct VisibilityConstraints {
    constraints: IndexVec<ScopedVisibilityConstraintId, VisibilityConstraintRef>,
}

impl VisibilityConstraints {
    pub(crate) fn new() -> Self {
        Self {
            constraints: IndexVec::from_iter([VisibilityConstraintRef::None]),
        }
    }

    pub(crate) fn add(
        &mut self,
        constraint: VisibilityConstraintRef,
    ) -> ScopedVisibilityConstraintId {
        self.constraints.push(constraint)
    }

    pub(crate) fn add_merged(
        &mut self,
        a: ScopedVisibilityConstraintId,
        b: ScopedVisibilityConstraintId,
    ) -> ScopedVisibilityConstraintId {
        match (&self.constraints[a], &self.constraints[b]) {
            (_, VisibilityConstraintRef::Negated(id)) if a == *id => {
                ScopedVisibilityConstraintId::from_u32(0)
            }
            (VisibilityConstraintRef::Negated(id), _) if *id == b => {
                ScopedVisibilityConstraintId::from_u32(0)
            }
            _ => self.add(VisibilityConstraintRef::Merged(a, b)),
        }
    }

    /// Analyze the statically known visibility for a given visibility constraint.
    pub(crate) fn analyze<'db>(
        self: &VisibilityConstraints,
        db: &'db dyn Db,
        all_constraints: &IndexVec<ScopedConstraintId, Constraint<'db>>,
        id: ScopedVisibilityConstraintId,
    ) -> Truthiness {
        self.analyze_impl(db, all_constraints, id, MAX_RECURSION_DEPTH)
    }

    fn analyze_impl<'db>(
        self: &VisibilityConstraints,
        db: &'db dyn Db,
        constraints: &IndexVec<ScopedConstraintId, Constraint<'db>>,
        id: ScopedVisibilityConstraintId,
        max_depth: usize,
    ) -> Truthiness {
        if max_depth == 0 {
            return Truthiness::Ambiguous;
        }

        let visibility_constraint = &self.constraints[id];
        match visibility_constraint {
            VisibilityConstraintRef::Single(id) => {
                let constraint = &constraints[*id];

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
                        PatternConstraintKind::Singleton(..)
                        | PatternConstraintKind::Unsupported => Truthiness::Ambiguous,
                    },
                }
            }
            VisibilityConstraintRef::Negated(inner_id) => self
                .analyze_impl(db, constraints, *inner_id, max_depth - 1)
                .negate(),
            VisibilityConstraintRef::None => Truthiness::AlwaysTrue,
            VisibilityConstraintRef::Sequence(lhs, rhs) => {
                let lhs = self.analyze_impl(db, constraints, *lhs, max_depth - 1);

                if lhs == Truthiness::AlwaysFalse {
                    return Truthiness::AlwaysFalse;
                }

                let rhs = self.analyze_impl(db, constraints, *rhs, max_depth - 1);

                if rhs == Truthiness::AlwaysFalse {
                    Truthiness::AlwaysFalse
                } else if lhs == Truthiness::AlwaysTrue && rhs == Truthiness::AlwaysTrue {
                    Truthiness::AlwaysTrue
                } else {
                    Truthiness::Ambiguous
                }
            }
            VisibilityConstraintRef::Merged(lhs_id, rhs_id) => {
                let lhs = self.analyze_impl(db, constraints, *lhs_id, max_depth - 1);

                if lhs == Truthiness::AlwaysTrue {
                    return Truthiness::AlwaysTrue;
                }

                let rhs = self.analyze_impl(db, constraints, *rhs_id, max_depth - 1);

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
}
