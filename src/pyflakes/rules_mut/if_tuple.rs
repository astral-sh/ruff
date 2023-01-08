use rustpython_ast::{Expr, Stmt};

use crate::ast::types::Range;
use crate::checkers::ast::Checker;
use crate::pyflakes::rules;

/// F634
pub fn if_tuple(checker: &mut Checker, stmt: &Stmt, test: &Expr) {
    if let Some(diagnostic) = rules::if_tuple(test, Range::from_located(stmt)) {
        checker.diagnostics.push(diagnostic);
    }
}
