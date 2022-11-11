use rustpython_ast::{AliasData, Located};
use rustpython_parser::ast::Stmt;

use crate::ast::types::Range;
use crate::check_ast::Checker;
use crate::pyupgrade::checks;
use crate::pyupgrade::fixes::remove_unnecessary_future_import;

pub fn unnecessary_future_import(checker: &mut Checker, stmt: &Stmt, names: &[Located<AliasData>]) {
    if let Some(mut check) = checks::unnecessary_future_import(
        checker.settings.target_version,
        names,
        Range::from_located(stmt),
    ) {
        if checker.patch() {
            if let Ok(fix) = remove_unnecessary_future_import(
                checker.locator,
                stmt,
                checker.settings.target_version,
                names,
            ) {
                check.amend(fix);
            }
        }
        checker.add_check(check);
    }
}
