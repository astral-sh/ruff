use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::traversal;
use ruff_python_ast::{self as ast, ExceptHandler, Expr, Int, MatchCase, Number, Operator, Stmt};
use ruff_text_size::{Ranged, TextRange};

use crate::checkers::ast::Checker;
use crate::rules::flake8_simplify::rules::ast_bool_op::is_same_expr;

/// ## What it does
/// Checks for loops which has explicit loop-index variables that can be simplified by using `enumerate()`.
///
/// ## Why this is bad?
/// Using `enumerate()` is more readable and concise. It could lead to more efficient
/// and less error-prone code.
///
/// ## Example
/// ```python
/// sum = 0
/// idx = 0
/// for item in items:
///    sum += func(item, idx)
///   idx += 1
/// ```
///
/// Use instead:
/// ```python
/// sum = 0
/// for idx, item in enumerate(items):
///    sum += func(item, idx)
/// ```
///
/// ## References
/// - [Python documentation: enumerate()](https://docs.python.org/3/library/functions.html#enumerate)
#[violation]
pub struct EnumerateForLoop {
    index_var: String,
}

impl Violation for EnumerateForLoop {
    #[derive_message_formats]
    fn message(&self) -> String {
        let EnumerateForLoop { index_var } = self;
        format!("Use enumereate() for index variable `{index_var}` in for loop")
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

    // Check if loop body contains a continue statement
    if loop_possibly_continues(body) {
        return;
    };
    // Check if index variable is initialized to zero prior to loop
    let Some((prev_stmt, index_var)) = get_candidate_loop_index(checker, stmt) else {
        return;
    };

    // Check if loop body contains an index increment statement matching `index_var
    if body.iter().any(|stmt| is_index_increment(stmt, index_var)) {
        let diagonastic = Diagnostic::new(
            EnumerateForLoop {
                index_var: checker.generator().expr(index_var),
            },
            TextRange::new(prev_stmt.start(), stmt.end()),
        );
        checker.diagnostics.push(diagonastic);
    }
}

/// Recursively check if the `for` loop body contains a `continue` statement
fn loop_possibly_continues(body: &[Stmt]) -> bool {
    body.iter().any(|stmt| match stmt {
        Stmt::Continue(_) => true,
        Stmt::If(ast::StmtIf {
            body,
            elif_else_clauses,
            ..
        }) => {
            loop_possibly_continues(body)
                || elif_else_clauses
                    .iter()
                    .any(|clause| loop_possibly_continues(&clause.body))
        }
        Stmt::With(ast::StmtWith { body, .. }) => loop_possibly_continues(body),
        Stmt::Match(ast::StmtMatch { cases, .. }) => cases
            .iter()
            .any(|MatchCase { body, .. }| loop_possibly_continues(body)),
        Stmt::Try(ast::StmtTry {
            body,
            handlers,
            orelse,
            finalbody,
            ..
        }) => {
            loop_possibly_continues(body)
                || loop_possibly_continues(orelse)
                || loop_possibly_continues(finalbody)
                || handlers.iter().any(|handler| match handler {
                    ExceptHandler::ExceptHandler(ast::ExceptHandlerExceptHandler {
                        body, ..
                    }) => loop_possibly_continues(body),
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
