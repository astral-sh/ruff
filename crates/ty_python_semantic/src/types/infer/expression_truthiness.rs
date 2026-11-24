//! Truthiness of already-inferred expressions, including paths that cannot produce a value.

use std::cell::Cell;

use ruff_python_ast as ast;
use ty_python_core::{Truthiness, expression::ExpressionContext};

use crate::{Db, ProgramEnvironment, types::Type};

/// Determine whether an expression can finish evaluating and which boolean outcomes it can produce.
///
/// Value context preserves the inferred result type, including objects whose truthiness can change
/// between tests. Condition context follows short-circuit paths without re-testing their results.
/// For example, `items and False` is always false as a condition, but its value can be `items`:
///
/// ```python
/// items = []
/// if items and False:
///     print("unreachable")
///
/// saved = items and False  # Produces the empty list itself.
/// items.append(1)
/// if saved:
///     print("reachable")  # Tests the same list again, now that it is nonempty.
/// ```
///
/// An expression can also fail to produce any value. Short-circuiting can bypass such an expression:
///
/// ```python
/// from typing import Never
///
/// def check(flag: bool, value: Never):
///     if flag or bool(value):
///         print("reachable when flag is true")
///     else:
///         print("unreachable")  # Evaluating value cannot produce a result.
/// ```
///
/// The callbacks read existing inference results; this analyzer does not infer or cache types.
pub(crate) struct TruthinessAnalyzer<'a, 'db, T, C, const INTRINSIC: bool = false> {
    deferred: Cell<bool>,
    db: &'db dyn Db,
    env: &'a ProgramEnvironment<'db>,
    expression_type: T,
    comparison_truthiness: C,
}

impl<'a, 'db, T, C> TruthinessAnalyzer<'a, 'db, T, C>
where
    T: Fn(&ast::Expr) -> Type<'db>,
    C: Fn(&ast::Expr) -> Option<Truthiness>,
{
    /// Read value types and comparison-chain outcomes from the same inference region.
    pub(crate) fn new(
        db: &'db dyn Db,
        env: &'a ProgramEnvironment<'db>,
        expression_type: T,
        comparison_truthiness: C,
    ) -> Self {
        Self {
            deferred: Cell::new(false),
            db,
            env,
            expression_type,
            comparison_truthiness,
        }
    }

    /// Follow the usual short-circuit paths, deferring the entire condition if
    /// any visited operand requires further inference. A call argument, for
    /// example, only needs to complete, even if its own truthiness is unknown.
    pub(super) fn intrinsic_truthiness(
        self,
        expression: &ast::Expr,
        context: ExpressionContext,
    ) -> Option<Truthiness> {
        let analyzer = TruthinessAnalyzer::<_, _, true> {
            db: self.db,
            env: self.env,
            expression_type: self.expression_type,
            comparison_truthiness: self.comparison_truthiness,
            deferred: Cell::new(false),
        };
        let outcome = analyzer.truthiness(expression, context);
        (!analyzer.deferred.get()).then_some(outcome)
    }
}

impl<'db, T, C, const INTRINSIC: bool> TruthinessAnalyzer<'_, 'db, T, C, INTRINSIC>
where
    T: Fn(&ast::Expr) -> Type<'db>,
    C: Fn(&ast::Expr) -> Option<Truthiness>,
{
    /// Test a value only when a boolean outcome is required.
    fn test(&self, outcome: EvaluationOutcome<'db>) -> Truthiness {
        match outcome {
            EvaluationOutcome::Condition(truthiness) => truthiness,
            EvaluationOutcome::Value(ty) if INTRINSIC => {
                ty.intrinsic_truthiness(self.db, self.env)
                    .unwrap_or_else(|| {
                        // Stop this path. The caller discards the entire result.
                        self.deferred.set(true);
                        Truthiness::Uninhabited
                    })
            }
            EvaluationOutcome::Value(ty) => ty.bool(self.db, self.env),
        }
    }

    /// Return `Uninhabited` when evaluation cannot produce either boolean outcome.
    pub(crate) fn truthiness(
        &self,
        expression: &ast::Expr,
        context: ExpressionContext,
    ) -> Truthiness {
        self.evaluate(expression, context)
            .map_or(Truthiness::Uninhabited, |outcome| self.test(outcome))
    }

    /// Check completion without testing the result object's truthiness.
    fn can_complete(&self, expression: &ast::Expr) -> bool {
        self.evaluate(expression, ExpressionContext::Value)
            .is_some()
    }

    /// Return `None` when the expression cannot finish evaluating, even if its inferred type is
    /// inhabited, as in `bool(never)`. This lets required operands propagate non-completion with `?`.
    /// A `Value` outcome has not tested the resulting object's truthiness; a `Condition` outcome
    /// records that test and can be `Uninhabited` if the test itself cannot complete.
    fn evaluate(
        &self,
        expression: &ast::Expr,
        context: ExpressionContext,
    ) -> Option<EvaluationOutcome<'db>> {
        let value = || {
            let ty = (self.expression_type)(expression);
            let never = if INTRINSIC {
                let possible = ty.might_be_equivalent_to_never(self.db);
                if possible && !matches!(ty, Type::Never) {
                    // A full relation check can require unfolding a type alias.
                    self.deferred.set(true);
                }
                possible
            } else {
                ty.is_equivalent_to(self.db, self.env, Type::Never)
            };
            (!never).then_some(EvaluationOutcome::Value(ty))
        };

        match expression {
            ast::Expr::BoolOp(ast::ExprBoolOp { op, values, .. }) => {
                if context == ExpressionContext::Condition {
                    let truthiness = values.iter().fold(
                        Truthiness::from(op.is_and()),
                        |result, operand| match op {
                            ast::BoolOp::And => {
                                result.and_then(|| self.truthiness(operand, context))
                            }
                            ast::BoolOp::Or => result.or_else(|| self.truthiness(operand, context)),
                        },
                    );
                    return Some(EvaluationOutcome::Condition(truthiness));
                }

                for (index, operand) in values.iter().enumerate() {
                    if index + 1 == values.len() {
                        self.evaluate(operand, context)?;
                        return value();
                    }
                    let truthiness = self.truthiness(operand, context);
                    if truthiness.is_uninhabited() {
                        return None;
                    }
                    if truthiness == Truthiness::from(op.is_or()) || truthiness.is_ambiguous() {
                        // A short-circuit path can produce an object. Its truthiness may
                        // change when the caller tests that object again.
                        return value();
                    }
                }
                value()
            }
            ast::Expr::UnaryOp(ast::ExprUnaryOp {
                op: ast::UnaryOp::Not,
                operand,
                ..
            }) if context == ExpressionContext::Condition => Some(EvaluationOutcome::Condition(
                self.truthiness(operand, context).negate(),
            )),
            ast::Expr::If(ast::ExprIf {
                test, body, orelse, ..
            }) => {
                let outcome = match self.truthiness(test, ExpressionContext::Condition) {
                    Truthiness::Uninhabited => None,
                    Truthiness::AlwaysTrue => self.evaluate(body, context),
                    Truthiness::AlwaysFalse => self.evaluate(orelse, context),
                    Truthiness::Ambiguous => {
                        let body_outcome = self.evaluate(body, context);
                        if context == ExpressionContext::Value && body_outcome.is_some() {
                            return value();
                        }
                        let orelse_outcome = self.evaluate(orelse, context);
                        match (body_outcome, orelse_outcome) {
                            (None, outcome) | (outcome, None) => outcome,
                            (Some(body), Some(orelse)) => {
                                let body = self.test(body);
                                let orelse = self.test(orelse);
                                Some(EvaluationOutcome::Condition(body.union(orelse)))
                            }
                        }
                    }
                }?;
                match context {
                    ExpressionContext::Value => value(),
                    ExpressionContext::Condition => Some(outcome),
                }
            }
            _ => {
                let operands_complete = match expression {
                    ast::Expr::UnaryOp(ast::ExprUnaryOp { operand, .. }) => {
                        self.can_complete(operand)
                    }
                    ast::Expr::Named(ast::ExprNamed { value, .. })
                    | ast::Expr::Starred(ast::ExprStarred { value, .. }) => {
                        self.can_complete(value)
                    }
                    ast::Expr::Call(call) => {
                        self.can_complete(&call.func)
                            && call
                                .arguments
                                .args
                                .iter()
                                .chain(call.arguments.keywords.iter().map(|keyword| &keyword.value))
                                .all(|argument| self.can_complete(argument))
                    }
                    ast::Expr::Compare(compare) => {
                        // Later comparisons can be skipped by short-circuiting.
                        self.can_complete(compare.first_operand())
                            && compare
                                .iter()
                                .next()
                                .is_none_or(|(_, _, right)| self.can_complete(right))
                    }
                    _ => true,
                };
                if !operands_complete {
                    return None;
                }
                if context == ExpressionContext::Condition
                    && let Some(truthiness) = (self.comparison_truthiness)(expression)
                {
                    return Some(EvaluationOutcome::Condition(truthiness));
                }
                value()
            }
        }
    }
}

/// Keep value truthiness lazy: checking whether an argument can complete does not call `__bool__`.
#[derive(Clone, Copy)]
enum EvaluationOutcome<'db> {
    Value(Type<'db>),
    Condition(Truthiness),
}
