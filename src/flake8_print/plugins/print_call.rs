use log::error;
use rustpython_ast::{Expr, Stmt, StmtKind};

use crate::ast::types::Range;
use crate::autofix::helpers;
use crate::check_ast::Checker;
use crate::checks::CheckCode;
use crate::flake8_print::checks;

/// T201, T203
pub fn print_call(checker: &mut Checker, expr: &Expr, func: &Expr) {
    let Some(mut check) = checks::print_call(
        func,
        checker.settings.enabled.contains(&CheckCode::T201),
        checker.settings.enabled.contains(&CheckCode::T203),
        Range::from_located(expr),
    ) else {
        return;
    };

    if checker.patch(check.kind.code()) {
        let defined_by = checker.current_parent();
        let defined_in = checker.current_grandparent();
        if matches!(defined_by.0.node, StmtKind::Expr { .. }) {
            let deleted: Vec<&Stmt> = checker.deletions.iter().map(|node| node.0).collect();
            match helpers::remove_stmt(defined_by.0, defined_in.map(|node| node.0), &deleted) {
                Ok(fix) => {
                    if fix.content.is_empty() || fix.content == "pass" {
                        checker.deletions.insert(defined_by.clone());
                    }
                    check.amend(fix);
                }
                Err(e) => error!("Failed to remove print call: {e}"),
            }
        }
    }

    checker.add_check(check);
}
