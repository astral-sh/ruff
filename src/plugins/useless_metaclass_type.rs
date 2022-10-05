use log::error;
use rustpython_ast::{Expr, Stmt};

use crate::ast::checks;
use crate::ast::types::{CheckLocator, Range};
use crate::autofix::{fixer, fixes};
use crate::check_ast::Checker;

pub fn useless_metaclass_type(
    checker: &mut Checker,
    stmt: &Stmt,
    value: &Expr,
    targets: &Vec<Expr>,
) {
    if let Some(mut check) = checks::check_useless_metaclass_type(
        targets,
        value,
        checker.locate_check(Range::from_located(stmt)),
    ) {
        if matches!(checker.autofix, fixer::Mode::Generate | fixer::Mode::Apply) {
            let context = checker.binding_context();
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
        checker.add_check(check);
    }
}
