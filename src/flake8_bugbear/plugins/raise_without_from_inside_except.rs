use rustpython_ast::{Expr, ExprKind, Stmt};

use crate::ast::types::Range;
use crate::check_ast::Checker;
use crate::checks::{Check, CheckKind};
use crate::python::string::is_lower;

pub fn raise_without_from_inside_except(checker: &mut Checker, stmt: &Stmt, exc: &Expr) {
    match &exc.node {
        ExprKind::Name { id, .. } if is_lower(id) => {}
        _ => {
            checker.add_check(Check::new(
                CheckKind::RaiseWithoutFromInsideExcept,
                Range::from_located(stmt),
            ));
        }
    }
}
