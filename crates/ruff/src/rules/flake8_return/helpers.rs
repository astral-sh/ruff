use ruff_text_size::TextSize;
use rustpython_parser::ast::{Constant, Expr, ExprKind, Stmt};

use ruff_python_ast::newlines::StrExt;
use ruff_python_ast::source_code::Locator;

/// Return `true` if a function's return statement include at least one
/// non-`None` value.
pub fn result_exists(returns: &[(&Stmt, Option<&Expr>)]) -> bool {
    returns.iter().any(|(_, expr)| {
        expr.map(|expr| {
            !matches!(
                expr.node,
                ExprKind::Constant {
                    value: Constant::None,
                    ..
                }
            )
        })
        .unwrap_or(false)
    })
}

/// Given a statement, find its "logical end".
///
/// For example: the statement could be following by a trailing semicolon, by an end-of-line
/// comment, or by any number of continuation lines (and then by a comment, and so on).
///
/// This method assumes that the statement is the last statement in its body; specifically, that
/// the statement isn't followed by a semicolon, followed by a multi-line statement.
pub fn end_of_last_statement(stmt: &Stmt, locator: &Locator) -> TextSize {
    // End-of-file, so just return the end of the statement.
    if stmt.end() == locator.text_len() {
        stmt.end()
    }
    // Otherwise, find the end of the last line that's "part of" the statement.
    else {
        let contents = locator.after(stmt.end());

        for line in contents.universal_newlines() {
            if !line.ends_with('\\') {
                return stmt.end() + line.end();
            }
        }

        unreachable!("Expected to find end-of-statement")
    }
}
