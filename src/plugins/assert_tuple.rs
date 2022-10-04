use rustpython_ast::{Expr, Stmt};

use crate::ast::checks;
use crate::ast::types::{CheckLocator, Range};
use crate::check_ast::Checker;

pub fn assert_tuple(checker: &mut Checker, stmt: &Stmt, test: &Expr) {
    if let Some(check) =
        checks::check_assert_tuple(test, checker.locate_check(Range::from_located(stmt)))
    {
        checker.add_check(check);
    }
}
