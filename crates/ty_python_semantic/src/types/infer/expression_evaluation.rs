//! Evaluation of already-inferred expressions, including paths that cannot produce a value.

use ruff_python_ast as ast;
use ty_python_core::{Truthiness, expression::ExpressionContext};

use crate::{Db, ProgramEnvironment, types::Type};

/// Determine whether an expression can finish evaluating and which boolean outcomes it can produce.
///
/// Value context preserves the inferred result type, including objects whose truthiness can change
/// between tests. Condition context follows short-circuit paths without re-testing their results.
/// The callbacks read existing inference results; this evaluator does not infer or cache types.
pub(crate) struct ExpressionEvaluator<'a, 'db, T, C> {
    db: &'db dyn Db,
    env: &'a ProgramEnvironment<'db>,
    expression_type: T,
    comparison_truthiness: C,
}

impl<'a, 'db, T, C> ExpressionEvaluator<'a, 'db, T, C>
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
            db,
            env,
            expression_type,
            comparison_truthiness,
        }
    }

    /// Return `None` when evaluation cannot produce either boolean outcome.
    pub(crate) fn truthiness(
        &self,
        expression: &ast::Expr,
        context: ExpressionContext,
    ) -> Option<Truthiness> {
        self.evaluate(expression, context)
            .map(|outcome| outcome.truthiness(self.db, self.env))
    }

    /// Check completion without testing the result object's truthiness.
    pub(crate) fn can_complete(&self, expression: &ast::Expr) -> bool {
        self.evaluate(expression, ExpressionContext::Value)
            .is_some()
    }

    fn evaluate(
        &self,
        expression: &ast::Expr,
        context: ExpressionContext,
    ) -> Option<EvaluationOutcome<'db>> {
        let value = || {
            let ty = (self.expression_type)(expression);
            (!ty.is_equivalent_to(self.db, self.env, Type::Never))
                .then_some(EvaluationOutcome::Value(ty))
        };

        match expression {
            ast::Expr::BoolOp(ast::ExprBoolOp { op, values, .. }) => {
                let short_circuit = Truthiness::from(op.is_or());
                let mut result = short_circuit.negate();
                for (index, operand) in values.iter().enumerate() {
                    if context == ExpressionContext::Value && index + 1 == values.len() {
                        self.evaluate(operand, context)?;
                        return value();
                    }
                    let Some(truthiness) = self.truthiness(operand, context) else {
                        return result
                            .is_ambiguous()
                            .then_some(EvaluationOutcome::Condition(short_circuit));
                    };
                    if truthiness == short_circuit || truthiness.is_ambiguous() {
                        if context == ExpressionContext::Value {
                            // A short-circuit path can produce an object. Its truthiness may
                            // change when the caller tests that object again.
                            return value();
                        }
                        if truthiness == short_circuit {
                            return Some(EvaluationOutcome::Condition(short_circuit));
                        }
                        result = Truthiness::Ambiguous;
                    }
                }
                match context {
                    ExpressionContext::Value => value(),
                    ExpressionContext::Condition => Some(EvaluationOutcome::Condition(result)),
                }
            }
            ast::Expr::UnaryOp(ast::ExprUnaryOp {
                op: ast::UnaryOp::Not,
                operand,
                ..
            }) if context == ExpressionContext::Condition => self
                .truthiness(operand, context)
                .map(|truthiness| EvaluationOutcome::Condition(truthiness.negate())),
            ast::Expr::If(ast::ExprIf {
                test, body, orelse, ..
            }) => {
                let outcome = match self.truthiness(test, ExpressionContext::Condition)? {
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
                                let body = body.truthiness(self.db, self.env);
                                let orelse = orelse.truthiness(self.db, self.env);
                                Some(EvaluationOutcome::Condition(if body == orelse {
                                    body
                                } else {
                                    Truthiness::Ambiguous
                                }))
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
enum EvaluationOutcome<'db> {
    Value(Type<'db>),
    Condition(Truthiness),
}

impl<'db> EvaluationOutcome<'db> {
    fn truthiness(self, db: &'db dyn Db, env: &ProgramEnvironment<'db>) -> Truthiness {
        match self {
            Self::Value(ty) => ty.bool(db, env),
            Self::Condition(truthiness) => truthiness,
        }
    }
}
