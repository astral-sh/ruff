use rustpython_ast::{Expr, Stmt};

use crate::ast::types::Range;
use crate::checkers::ast::Checker;
use crate::pyflakes::checks;

/// F631
pub fn assert_tuple(checker: &mut Checker, stmt: &Stmt, test: &Expr) {
    if let Some(check) = checks::assert_tuple(test, Range::from_located(stmt)) {
        checker.diagnostics.push(check);
    }
}
