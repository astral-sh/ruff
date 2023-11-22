use ruff_python_ast as ast;
use ruff_python_ast::Stmt;
use ruff_text_size::{Ranged, TextSize};

use ruff_source_file::{Locator, UniversalNewlines};

/// Return `true` if a function's return statement include at least one
/// non-`None` value.
pub(super) fn result_exists(returns: &[&ast::StmtReturn]) -> bool {
    returns.iter().any(|stmt| {
        stmt.value
            .as_deref()
            .is_some_and(|value| !value.is_none_literal_expr())
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
    // Find the end of the last line that's "part of" the statement.
    for line in locator.after(stmt.end()).universal_newlines() {
        if !line.ends_with('\\') {
            return stmt.end() + line.end();
        }
    }
    locator.text_len()
}
