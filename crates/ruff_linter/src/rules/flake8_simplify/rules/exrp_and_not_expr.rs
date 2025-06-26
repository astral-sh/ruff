use ruff_python_ast::{self as ast, BoolOp, Expr, UnaryOp};
use ruff_text_size::Ranged;

use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_python_ast::helpers::contains_effect;

use crate::checkers::ast::Checker;
use crate::rules::flake8_simplify::helpers::is_same_expr;
use crate::{AlwaysFixableViolation, Edit, Fix};
/// ## What it does
/// Checks for `and` expressions that include both an expression and its
/// negation.
///
/// ## Why is this bad?
/// An `and` expression that includes both an expression and its negation will
/// always evaluate to `False`.
///
/// ## Example
/// ```python
/// x and not x
/// ```
///
/// ## References
/// - [Python documentation: Boolean operations](https://docs.python.org/3/reference/expressions.html#boolean-operations)
#[derive(ViolationMetadata)]
pub(crate) struct ExprAndNotExpr {
    name: String,
}

impl AlwaysFixableViolation for ExprAndNotExpr {
    #[derive_message_formats]
    fn message(&self) -> String {
        let ExprAndNotExpr { name } = self;
        format!("Use `False` instead of `{name} and not {name}`")
    }

    fn fix_title(&self) -> String {
        "Replace with `False`".to_string()
    }
}

/// SIM220
pub(crate) fn expr_and_not_expr(checker: &Checker, expr: &Expr) {
    let Expr::BoolOp(ast::ExprBoolOp {
        op: BoolOp::And,
        values,
        range: _,
        node_index: _,
    }) = expr
    else {
        return;
    };
    if values.len() < 2 {
        return;
    }

    // Collect all negated and non-negated expressions.
    let mut negated_expr = vec![];
    let mut non_negated_expr = vec![];
    for expr in values {
        if let Expr::UnaryOp(ast::ExprUnaryOp {
            op: UnaryOp::Not,
            operand,
            range: _,
            node_index: _,
        }) = expr
        {
            negated_expr.push(operand);
        } else {
            non_negated_expr.push(expr);
        }
    }

    if negated_expr.is_empty() {
        return;
    }

    if contains_effect(expr, |id| checker.semantic().has_builtin_binding(id)) {
        return;
    }

    for negate_expr in negated_expr {
        for non_negate_expr in &non_negated_expr {
            if let Some(id) = is_same_expr(negate_expr, non_negate_expr) {
                let mut diagnostic = checker.report_diagnostic(
                    ExprAndNotExpr {
                        name: id.to_string(),
                    },
                    expr.range(),
                );
                diagnostic.set_fix(Fix::unsafe_edit(Edit::range_replacement(
                    "False".to_string(),
                    expr.range(),
                )));
            }
        }
    }
}
