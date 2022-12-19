use log::error;
use rustpython_ast::{Expr, Stmt};

use crate::ast::types::Range;
use crate::autofix::helpers;
use crate::checkers::ast::Checker;
use crate::pyupgrade::checks;

/// UP001
pub fn useless_metaclass_type(checker: &mut Checker, stmt: &Stmt, value: &Expr, targets: &[Expr]) {
    let Some(mut check) =
        checks::useless_metaclass_type(targets, value, Range::from_located(stmt)) else {
            return;
        };
    if checker.patch(check.kind.code()) {
        let deleted: Vec<&Stmt> = checker.deletions.iter().map(|node| node.0).collect();
        let defined_by = checker.current_stmt();
        let defined_in = checker.current_stmt_parent();
        match helpers::delete_stmt(
            defined_by.0,
            defined_in.map(|node| node.0),
            &deleted,
            checker.locator,
        ) {
            Ok(fix) => {
                if fix.content.is_empty() || fix.content == "pass" {
                    checker.deletions.insert(defined_by.clone());
                }
                check.amend(fix);
            }
            Err(e) => error!("Failed to fix remove metaclass type: {e}"),
        }
    }
    checker.add_check(check);
}
