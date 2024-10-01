use ast::{Expr, StmtIf};
use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast as ast;
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;

/// ## What it does
/// Checks for too many Boolean expressions in an `if` statement.
///
/// By default, this rule allows up to 5 expressions. This can be configured
/// using the [`lint.pylint.max-bool-expr`] option.
///
/// ## Why is this bad?
/// `if` statements with many Boolean expressions are harder to understand
/// and maintain. Consider assigning the result of the Boolean expression,
/// or any of its sub-expressions, to a variable.
///
/// ## Example
/// ```python
/// if a and b and c and d and e and f and g and h:
///     ...
/// ```
///
/// ## Options
/// - `lint.pylint.max-bool-expr`
#[violation]
pub struct TooManyBooleanExpressions {
    expressions: usize,
    max_expressions: usize,
}

impl Violation for TooManyBooleanExpressions {
    #[derive_message_formats]
    fn message(&self) -> String {
        let TooManyBooleanExpressions {
            expressions,
            max_expressions,
        } = self;
        format!("Too many Boolean expressions ({expressions} > {max_expressions})")
    }
}

/// PLR0916
pub(crate) fn too_many_boolean_expressions(checker: &mut Checker, stmt: &StmtIf) {
    if let Some(bool_op) = stmt.test.as_bool_op_expr() {
        let expressions = count_bools(bool_op);
        if expressions > checker.settings.pylint.max_bool_expr {
            checker.diagnostics.push(Diagnostic::new(
                TooManyBooleanExpressions {
                    expressions,
                    max_expressions: checker.settings.pylint.max_bool_expr,
                },
                bool_op.range(),
            ));
        }
    }

    for elif in &stmt.elif_else_clauses {
        if let Some(bool_op) = elif.test.as_ref().and_then(Expr::as_bool_op_expr) {
            let expressions = count_bools(bool_op);
            if expressions > checker.settings.pylint.max_bool_expr {
                checker.diagnostics.push(Diagnostic::new(
                    TooManyBooleanExpressions {
                        expressions,
                        max_expressions: checker.settings.pylint.max_bool_expr,
                    },
                    bool_op.range(),
                ));
            }
        }
    }
}

/// Count the number of Boolean expressions in a `bool_op` expression.
fn count_bools(bool_op: &ast::ExprBoolOp) -> usize {
    bool_op
        .values
        .iter()
        .map(|expr| {
            if let Expr::BoolOp(bool_op) = expr {
                count_bools(bool_op)
            } else {
                1
            }
        })
        .sum::<usize>()
}
