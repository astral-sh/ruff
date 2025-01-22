//! # Visibility constraints
//!
//! During semantic index building, we collect visibility constraints for each binding and
//! declaration. These constraints are then used during type-checking to determine the static
//! visibility of a certain definition. This allows us to re-analyze control flow during type
//! checking, potentially "hiding" some branches that we can statically determine to never be
//! taken. Consider the following example first. We added implicit "unbound" definitions at the
//! start of the scope. Note how visibility constraints can apply to bindings outside of the
//! if-statement:
//! ```py
//! x = <unbound>  # not a live binding for the use of x below, shadowed by `x = 1`
//! y = <unbound>  # visibility constraint: ~test
//!
//! x = 1  # visibility constraint: ~test
//! if test:
//!     x = 2  # visibility constraint: test
//!
//!     y = 2  # visibility constraint: test
//!
//! use(x)
//! use(y)
//! ```
//! The static truthiness of the `test` condition can either be always-false, ambiguous, or
//! always-true. Similarly, we have the same three options when evaluating a visibility constraint.
//! This outcome determines the visibility of a definition: always-true means that the definition
//! is definitely visible for a given use, always-false means that the definition is definitely
//! not visible, and ambiguous means that we might see this definition or not. In the latter case,
//! we need to consider both options during type inference and boundness analysis. For the example
//! above, these are the possible type inference / boundness results for the uses of `x` and `y`:
//!
//! ```text
//!       | `test` truthiness | `~test` truthiness | type of `x`     | boundness of `y` |
//!       |-------------------|--------------------|-----------------|------------------|
//!       | always false      | always true        | `Literal[1]`    | unbound          |
//!       | ambiguous         | ambiguous          | `Literal[1, 2]` | possibly unbound |
//!       | always true       | always false       | `Literal[2]`    | bound            |
//! ```
//!
//! ### Sequential constraints (ternary AND)
//!
//! As we have seen above, visibility constraints can apply outside of a control flow element.
//! So we need to consider the possibility that multiple constraints apply to the same binding.
//! Here, we consider what happens if multiple `if`-statements lead to a sequence of constraints.
//! Consider the following example:
//! ```py
//! x = 0
//!
//! if test1:
//!     x = 1
//!
//! if test2:
//!     x = 2
//! ```
//! The binding `x = 2` is easy to analyze. Its visibility corresponds to the truthiness of `test2`.
//! For the `x = 1` binding, things are a bit more interesting. It is always visible if `test1` is
//! always-true *and* `test2` is always-false. It is never visible if `test1` is always-false *or*
//! `test2` is always-true. And it is ambiguous otherwise. This corresponds to a ternary *test1 AND
//! ~test2* operation in three-valued Kleene logic [Kleene]:
//!
//! ```text
//!       | AND          | always-false | ambiguous    | always-true  |
//!       |--------------|--------------|--------------|--------------|
//!       | always false | always-false | always-false | always-false |
//!       | ambiguous    | always-false | ambiguous    | ambiguous    |
//!       | always true  | always-false | ambiguous    | always-true  |
//! ```
//!
//! The `x = 0` binding can be handled similarly, with the difference that both `test1` and `test2`
//! are negated:
//! ```py
//! x = 0  # ~test1 AND ~test2
//!
//! if test1:
//!     x = 1  # test1 AND ~test2
//!
//! if test2:
//!     x = 2  # test2
//! ```
//!
//! ### Merged constraints (ternary OR)
//!
//! Finally, we consider what happens in "parallel" control flow. Consider the following example
//! where we have omitted the test condition for the outer `if` for clarity:
//! ```py
//! x = 0
//!
//! if <…>:
//!     if test1:
//!         x = 1
//! else:
//!     if test2:
//!         x = 2
//!
//! use(x)
//! ```
//! At the usage of `x`, i.e. after control flow has been merged again, the visibility of the `x =
//! 0` binding behaves as follows: the binding is always visible if `test1` is always-false *or*
//! `test2` is always-false; and it is never visible if `test1` is always-true *and* `test2` is
//! always-true. This corresponds to a ternary *OR* operation in Kleene logic:
//!
//! ```text
//!       | OR           | always-false | ambiguous    | always-true  |
//!       |--------------|--------------|--------------|--------------|
//!       | always false | always-false | ambiguous    | always-true  |
//!       | ambiguous    | ambiguous    | ambiguous    | always-true  |
//!       | always true  | always-true  | always-true  | always-true  |
//! ```
//!
//! Using this, we can annotate the visibility constraints for the example above:
//! ```py
//! x = 0  # ~test1 OR ~test2
//!
//! if <…>:
//!     if test1:
//!         x = 1  # test1
//! else:
//!     if test2:
//!         x = 2  # test2
//!
//! use(x)
//! ```
//!
//! ### Explicit ambiguity
//!
//! In some cases, we explicitly add a `VisibilityConstraint::Ambiguous` constraint to all bindings
//! in a certain control flow path. We do this when branching on something that we can not (or
//! intentionally do not want to) analyze statically. `for` loops are one example:
//! ```py
//! x = <unbound>
//!
//! for _ in range(2):
//!    x = 1
//! ```
//! Here, we report an ambiguous visibility constraint before branching off. If we don't do this,
//! the `x = <unbound>` binding would be considered unconditionally visible in the no-loop case.
//! And since the other branch does not have the live `x = <unbound>` binding, we would incorrectly
//! create a state where the `x = <unbound>` binding is always visible.
//!
//!
//! ### Properties
//!
//! The ternary `AND` and `OR` operations have the property that `~a OR ~b = ~(a AND b)`. This
//! means we could, in principle, get rid of either of these two to simplify the representation.
//!
//! However, we already apply negative constraints `~test1` and `~test2` to the "branches not
//! taken" in the example above. This means that the tree-representation `~test1 OR ~test2` is much
//! cheaper/shallower than basically creating `~(~(~test1) AND ~(~test2))`. Similarly, if we wanted
//! to get rid of `AND`, we would also have to create additional nodes. So for performance reasons,
//! there is a small "duplication" in the code between those two constraint types.
//!
//! [Kleene]: <https://en.wikipedia.org/wiki/Three-valued_logic#Kleene_and_Priest_logics>

use ruff_index::IndexVec;

use crate::semantic_index::ScopedVisibilityConstraintId;
use crate::semantic_index::{
    ast_ids::HasScopedExpressionId,
    constraint::{Constraint, ConstraintNode, PatternConstraintKind},
};
use crate::types::{infer_expression_types, Truthiness};
use crate::Db;

/// The maximum depth of recursion when evaluating visibility constraints.
///
/// This is a performance optimization that prevents us from descending deeply in case of
/// pathological cases. The actual limit here has been derived from performance testing on
/// the `black` codebase. When increasing the limit beyond 32, we see a 5x runtime increase
/// resulting from a few files with a lot of boolean expressions and `if`-statements.
const MAX_RECURSION_DEPTH: usize = 24;

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

impl Default for VisibilityConstraints<'_> {
    fn default() -> Self {
        Self {
            constraints: IndexVec::from_iter([VisibilityConstraint::AlwaysTrue]),
        }
    }
}

impl<'db> VisibilityConstraints<'db> {
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
                let ty = inference
                    .expression_type(test_expr.node_ref(db).scoped_expression_id(db, scope));

                ty.bool(db).negate_if(!constraint.is_positive)
            }
            ConstraintNode::Pattern(inner) => match inner.kind(db) {
                PatternConstraintKind::Value(value, guard) => {
                    let subject_expression = inner.subject(db);
                    let inference = infer_expression_types(db, *subject_expression);
                    let scope = subject_expression.scope(db);
                    let subject_ty = inference.expression_type(
                        subject_expression
                            .node_ref(db)
                            .scoped_expression_id(db, scope),
                    );

                    let inference = infer_expression_types(db, *value);
                    let scope = value.scope(db);
                    let value_ty = inference
                        .expression_type(value.node_ref(db).scoped_expression_id(db, scope));

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
                | PatternConstraintKind::Class(..)
                | PatternConstraintKind::Unsupported => Truthiness::Ambiguous,
            },
        }
    }
}
