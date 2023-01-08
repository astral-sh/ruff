use rustpython_ast::{Expr, Stmt};

use crate::ast::types::Range;
use crate::pyflakes::checks;
use crate::xxxxxxxxs::ast::xxxxxxxx;

/// F631
pub fn assert_tuple(xxxxxxxx: &mut xxxxxxxx, stmt: &Stmt, test: &Expr) {
    if let Some(check) = checks::assert_tuple(test, Range::from_located(stmt)) {
        xxxxxxxx.diagnostics.push(check);
    }
}
