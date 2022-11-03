use log::error;
use rustpython_ast::{Expr, Stmt, StmtKind};

use crate::ast::types::{CheckLocator, Range};
use crate::autofix::helpers;
use crate::check_ast::Checker;
use crate::checks::CheckCode;
use crate::flake8_print::checks;

pub fn print_call(checker: &mut Checker, expr: &Expr, func: &Expr) {
    if let Some(mut check) = checks::print_call(
        expr,
        func,
        checker.settings.enabled.contains(&CheckCode::T201),
        checker.settings.enabled.contains(&CheckCode::T203),
        checker.locate_check(Range::from_located(expr)),
    ) {
        if checker.patch() {
            let context = checker.binding_context();
            if matches!(
                checker.parents[context.defined_by].node,
                StmtKind::Expr { .. }
            ) {
                let deleted: Vec<&Stmt> = checker
                    .deletions
                    .iter()
                    .map(|index| checker.parents[*index])
                    .collect();

                match helpers::remove_stmt(
                    checker.parents[context.defined_by],
                    context.defined_in.map(|index| checker.parents[index]),
                    &deleted,
                ) {
                    Ok(fix) => {
                        if fix.patch.content.is_empty() || fix.patch.content == "pass" {
                            checker.deletions.insert(context.defined_by);
                        }
                        check.amend(fix)
                    }
                    Err(e) => error!("Failed to fix unused imports: {}", e),
                }
            }
        }

        checker.add_check(check);
    }
}
