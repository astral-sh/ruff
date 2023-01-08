use rustpython_ast::{Expr, Stmt};

use crate::ast::types::Range;
use crate::pyflakes::checks;
use crate::xxxxxxxxs::ast::xxxxxxxx;

/// F634
pub fn if_tuple(xxxxxxxx: &mut xxxxxxxx, stmt: &Stmt, test: &Expr) {
    if let Some(check) = checks::if_tuple(test, Range::from_located(stmt)) {
        xxxxxxxx.diagnostics.push(check);
    }
}
