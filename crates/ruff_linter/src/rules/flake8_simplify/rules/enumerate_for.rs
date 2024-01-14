use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::traversal;
use ruff_python_ast::{self as ast, ExceptHandler, Expr, Int, MatchCase, Number, Operator, Stmt};
use ruff_text_size::{Ranged, TextRange};

use crate::checkers::ast::Checker;
use crate::rules::flake8_simplify::rules::ast_bool_op::is_same_expr;

/// ## What it does
/// Checks for `for` loops with explicit loop-index variables that can be replaced
/// with `enumerate()`.
///
/// ## Why this is bad?
/// When iterating over a sequence, it's often desirable to keep track of the
/// index of each element alongside the element itself. Prefer the `enumerate`
/// builtin over manually incrementing a counter variable within the loop, as
/// `enumerate` is more concise and idiomatic.
///
/// ## Example
/// ```python
/// fruits = ["apple", "banana", "cherry"]
/// for fruit in fruits:
///     print(f"{i + 1}. {fruit}")
///     i += 1
/// ```
///
/// Use instead:
/// ```python
/// fruits = ["apple", "banana", "cherry"]
/// for i, fruit in enumerate(fruits):
///     print(f"{i + 1}. {fruit}")
/// ```
///
/// ## References
/// - [Python documentation: `enumerate`](https://docs.python.org/3/library/functions.html#enumerate)
#[violation]
pub struct EnumerateForLoop {
    index: String,
}

impl Violation for EnumerateForLoop {
    #[derive_message_formats]
    fn message(&self) -> String {
        let EnumerateForLoop { index } = self;
        format!("Use `enumerate()` for index variable `{index}` in `for` loop")
    }
}

/// SIM113
pub(crate) fn use_enumerate_in_for_loop(checker: &mut Checker, stmt: &Stmt) {
    if !checker.semantic().current_scope().kind.is_function() {
        return;
    }
    let Stmt::For(ast::StmtFor { body, .. }) = stmt else {
        return;
    };

    // Check if loop body contains a continue statement.
    if has_continue(body) {
        return;
    };

    // Check if index variable is initialized to zero prior to loop.
    let Some((prev_stmt, index)) = get_candidate_loop_index(checker, stmt) else {
        return;
    };

    // Check if loop body contains an index increment statement matching `index`.
    if body.iter().any(|stmt| is_index_increment(stmt, index)) {
        let diagnostic = Diagnostic::new(
            EnumerateForLoop {
                index: checker.generator().expr(index),
            },
            TextRange::new(prev_stmt.start(), stmt.end()),
        );
        checker.diagnostics.push(diagnostic);
    }
}

/// Recursively check if the `for` loop body contains a `continue` statement
fn has_continue(body: &[Stmt]) -> bool {
    body.iter().any(|stmt| match stmt {
        Stmt::Continue(_) => true,
        Stmt::If(ast::StmtIf {
            body,
            elif_else_clauses,
            ..
        }) => {
            has_continue(body)
                || elif_else_clauses
                    .iter()
                    .any(|clause| has_continue(&clause.body))
        }
        Stmt::With(ast::StmtWith { body, .. }) => has_continue(body),
        Stmt::Match(ast::StmtMatch { cases, .. }) => cases
            .iter()
            .any(|MatchCase { body, .. }| has_continue(body)),
        Stmt::Try(ast::StmtTry {
            body,
            handlers,
            orelse,
            finalbody,
            ..
        }) => {
            has_continue(body)
                || has_continue(orelse)
                || has_continue(finalbody)
                || handlers.iter().any(|handler| match handler {
                    ExceptHandler::ExceptHandler(ast::ExceptHandlerExceptHandler {
                        body, ..
                    }) => has_continue(body),
                })
        }

        _ => false,
    })
}

/// Check previous statement of `for` loop to find a possible index variable
/// which is initialized to zero
/// Ex:
/// ```python
/// idx = 0
/// for item in items:
///     ...
/// ```
/// Return (&Stmt, &Expr) to initialization stmt and `idx` variable
fn get_candidate_loop_index<'a>(
    checker: &'a Checker,
    stmt: &'a Stmt,
) -> Option<(&'a Stmt, &'a Expr)> {
    let parent = checker.semantic().current_statement_parent()?;
    let suite = traversal::suite(stmt, parent)?;
    let prev_stmt = traversal::prev_sibling(stmt, suite)?;
    // check if it's a possible index initialization i.e. idx = 0
    let Stmt::Assign(ast::StmtAssign { targets, value, .. }) = prev_stmt else {
        return None;
    };
    let [Expr::Name(ast::ExprName { id: _, .. })] = targets.as_slice() else {
        return None;
    };
    if let Expr::NumberLiteral(ast::ExprNumberLiteral {
        value: Number::Int(value),
        ..
    }) = value.as_ref()
    {
        if matches!(*value, Int::ZERO) {
            return Some((prev_stmt, &targets[0]));
        }
    }

    None
}

// Check if `stmt` is `index_var` += 1
fn is_index_increment(stmt: &Stmt, index_var: &Expr) -> bool {
    let Stmt::AugAssign(ast::StmtAugAssign {
        target, op, value, ..
    }) = stmt
    else {
        return false;
    };
    if !matches!(op, Operator::Add) {
        return false;
    }
    let Some(_) = is_same_expr(index_var, target) else {
        return false;
    };
    if let Expr::NumberLiteral(ast::ExprNumberLiteral {
        value: Number::Int(value),
        ..
    }) = value.as_ref()
    {
        if matches!(*value, Int::ONE) {
            return true;
        }
    }
    false
}
