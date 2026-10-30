use std::collections::VecDeque;

use ruff_python_ast::{
    ExceptHandler, Expr, Operator,
    flat::{self, fields as ast},
};

use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_text_size::Ranged;

use crate::Violation;
use crate::checkers::ast::Checker;
use crate::codes::Category;

/// ## What it does
/// Checks for exception handlers that catch non-exception classes.
///
/// ## Why is this bad?
/// Catching classes that do not inherit from `BaseException` will raise a
/// `TypeError`.
///
/// ## Example
/// ```python
/// try:
///     1 / 0
/// except 1:
///     ...
/// ```
///
/// Use instead:
/// ```python
/// try:
///     1 / 0
/// except ZeroDivisionError:
///     ...
/// ```
///
/// ## References
/// - [Python documentation: `except` clause](https://docs.python.org/3/reference/compound_stmts.html#except-clause)
/// - [Python documentation: Built-in Exceptions](https://docs.python.org/3/library/exceptions.html#built-in-exceptions)
#[derive(ViolationMetadata)]
#[violation_metadata(stable_since = "v0.0.255", category = Category::Correctness)]
pub(crate) struct ExceptWithNonExceptionClasses {
    is_star: bool,
}

impl Violation for ExceptWithNonExceptionClasses {
    #[derive_message_formats]
    fn message(&self) -> String {
        if self.is_star {
            "`except*` handlers should only be exception classes or tuples of exception classes"
                .to_string()
        } else {
            "`except` handlers should only be exception classes or tuples of exception classes"
                .to_string()
        }
    }
}

/// B030
pub(crate) fn except_with_non_exception_classes(
    checker: &Checker,
    except_handler: flat::View<'_, ExceptHandler>,
) {
    let ast::ExceptHandler::ExceptHandler(ast::ExceptHandlerExceptHandler { type_, .. }) =
        except_handler.fields();
    let Some(type_) = type_ else {
        return;
    };
    for expr in flatten_iterables(type_) {
        if !matches!(
            expr.classify(),
            flat::ExprKind::Subscript(_)
                | flat::ExprKind::Attribute(_)
                | flat::ExprKind::Name(_)
                | flat::ExprKind::Call(_),
        ) {
            let is_star = checker
                .semantic()
                .current_statement()
                .as_try_stmt()
                .is_some_and(flat::View::is_star);
            checker.report_diagnostic(ExceptWithNonExceptionClasses { is_star }, expr.range());
        }
    }
}

/// Given an [`Expr`], flatten any [`Expr::Starred`] expressions and any
/// [`Expr::BinOp`] expressions into a flat list of expressions.
///
/// This should leave any unstarred iterables alone (subsequently raising a
/// warning for B029).
fn flatten_iterables(expr: flat::View<'_, Expr>) -> Vec<flat::View<'_, Expr>> {
    // Unpack the top-level Tuple into queue, otherwise add as-is.
    let mut exprs_to_process: VecDeque<flat::View<'_, Expr>> = match expr.classify() {
        flat::ExprKind::Tuple(expr_tuple) => {
            let elts = expr_tuple.elts();
            elts.iter().collect()
        }
        _ => vec![expr].into(),
    };
    let mut flattened_exprs: Vec<flat::View<'_, Expr>> = Vec::with_capacity(exprs_to_process.len());

    while let Some(expr) = exprs_to_process.pop_front() {
        match expr.classify() {
            flat::ExprKind::Starred(expr_starred) => {
                let value = expr_starred.value();
                match value.fields() {
                    ast::Expr::Tuple(ast::ExprTuple { elts, .. })
                    | ast::Expr::List(ast::ExprList { elts, .. }) => {
                        exprs_to_process.append(&mut elts.iter().collect());
                    }
                    ast::Expr::BinOp(ast::ExprBinOp {
                        op: Operator::Add, ..
                    }) => {
                        exprs_to_process.push_back(value);
                    }
                    _ => flattened_exprs.push(value),
                }
            }
            flat::ExprKind::BinOp(expr_bin_op) if expr_bin_op.op() == Operator::Add => {
                let left = expr_bin_op.left();
                let right = expr_bin_op.right();

                for expr in [left, right] {
                    // If left or right are tuples, starred, or binary operators, flatten them.
                    match expr.classify() {
                        flat::ExprKind::Tuple(expr_tuple) => {
                            let elts = expr_tuple.elts();

                            exprs_to_process.append(&mut elts.iter().collect());
                        }
                        flat::ExprKind::Starred(expr_starred) => {
                            let value = expr_starred.value();

                            exprs_to_process.push_back(value);
                        }
                        flat::ExprKind::BinOp(expr_bin_op) if expr_bin_op.op() == Operator::Add => {
                            exprs_to_process.push_back(expr);
                        }
                        _ => flattened_exprs.push(expr),
                    }
                }
            }
            _ => flattened_exprs.push(expr),
        }
    }

    flattened_exprs
}
