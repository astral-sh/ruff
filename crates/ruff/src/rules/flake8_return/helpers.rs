use rustpython_parser::ast::{Constant, Expr, ExprKind, Location, Stmt};

use ruff_python_ast::helpers::to_absolute;
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
pub fn end_of_last_statement(stmt: &Stmt, locator: &Locator) -> Location {
    let contents = locator.after(stmt.end_location.unwrap());

    // End-of-file, so just return the end of the statement.
    if contents.is_empty() {
        return stmt.end_location.unwrap();
    }

    // Otherwise, find the end of the last line that's "part of" the statement.
    for (lineno, line) in contents.universal_newlines().enumerate() {
        if line.ends_with('\\') {
            continue;
        }
        return to_absolute(
            Location::new(lineno + 1, line.chars().count()),
            stmt.end_location.unwrap(),
        );
    }

    unreachable!("Expected to find end-of-statement")
}
