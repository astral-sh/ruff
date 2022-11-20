use log::error;
use rustpython_ast::{Expr, Stmt};

use crate::ast::types::Range;
use crate::autofix::helpers;
use crate::check_ast::Checker;
use crate::pyupgrade::checks;

/// U001
pub fn useless_metaclass_type(checker: &mut Checker, stmt: &Stmt, value: &Expr, targets: &[Expr]) {
    if let Some(mut check) =
        checks::useless_metaclass_type(targets, value, Range::from_located(stmt))
    {
        if checker.patch(check.kind.code()) {
            let context = checker.binding_context();
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
                Err(e) => error!("Failed to fix remove metaclass type: {}", e),
            }
        }
        checker.add_check(check);
    }
}
