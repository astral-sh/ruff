use rustpython_parser::ast::Stmt;

use crate::ast::types::Range;
use crate::check_ast::Checker;
use crate::pyupgrade::checks;

pub fn unnecessary_future_import(checker: &mut Checker, stmt: &Stmt, name: &str) {
    if let Some(check) = checks::unnecessary_future_import(
        checker.settings.target_version,
        name,
        Range::from_located(stmt),
    ) {
        // TODO - remove unnecessary import
        // if checker.patch() {
        // }
        checker.add_check(check);
    }
}
