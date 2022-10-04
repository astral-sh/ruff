use log::error;
use rustpython_ast::{Expr, Stmt, StmtKind};

use crate::ast::checks;
use crate::autofix::{fixer, fixes};
use crate::check_ast::Checker;
use crate::checks::CheckCode;

pub fn print_call(checker: &mut Checker, expr: &Expr, func: &Expr) {
    if let Some(mut check) = checks::check_print_call(
        expr,
        func,
        checker.settings.enabled.contains(&CheckCode::T201),
        checker.settings.enabled.contains(&CheckCode::T203),
    ) {
        if matches!(checker.autofix, fixer::Mode::Generate | fixer::Mode::Apply) {
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

                match fixes::remove_stmt(
                    checker.parents[context.defined_by],
                    context.defined_in.map(|index| checker.parents[index]),
                    &deleted,
                ) {
                    Ok(fix) => {
                        if fix.content.is_empty() || fix.content == "pass" {
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
