use ruff_text_size::TextSize;
use rustpython_parser::ast;
use rustpython_parser::ast::{Expr, Ranged, Stmt};

use ruff_python_ast::source_code::Locator;
use ruff_python_whitespace::UniversalNewlines;

/// Return `true` if a function's return statement include at least one
/// non-`None` value.
pub(super) fn result_exists(returns: &[&ast::StmtReturn]) -> bool {
    returns.iter().any(|stmt| {
        stmt.value.as_deref().map_or(false, |value| {
            !matches!(
                value,
                Expr::Constant(constant) if constant.value.is_none()
            )
        })
    })
}

/// Given a statement, find its "logical end".
///
/// For example: the statement could be following by a trailing semicolon, by an end-of-line
/// comment, or by any number of continuation lines (and then by a comment, and so on).
///
/// This method assumes that the statement is the last statement in its body; specifically, that
/// the statement isn't followed by a semicolon, followed by a multi-line statement.
pub(super) fn end_of_last_statement(stmt: &Stmt, locator: &Locator) -> TextSize {
    if stmt.end() == locator.text_len() {
        // End-of-file, so just return the end of the statement.
        stmt.end()
    } else {
        // Otherwise, find the end of the last line that's "part of" the statement.
        let contents = locator.after(stmt.end());

        for line in contents.universal_newlines() {
            if !line.ends_with('\\') {
                return stmt.end() + line.end();
            }
        }

        unreachable!("Expected to find end-of-statement")
    }
}
