use log::error;
use rustpython_ast::{Expr, Stmt};

use crate::ast::types::Range;
use crate::autofix::helpers;
use crate::pyupgrade::checks;
use crate::xxxxxxxxs::ast::xxxxxxxx;

/// UP001
pub fn useless_metaclass_type(
    xxxxxxxx: &mut xxxxxxxx,
    stmt: &Stmt,
    value: &Expr,
    targets: &[Expr],
) {
    let Some(mut check) =
        checks::useless_metaclass_type(targets, value, Range::from_located(stmt)) else {
            return;
        };
    if xxxxxxxx.patch(check.kind.code()) {
        let deleted: Vec<&Stmt> = xxxxxxxx
            .deletions
            .iter()
            .map(std::convert::Into::into)
            .collect();
        let defined_by = xxxxxxxx.current_stmt();
        let defined_in = xxxxxxxx.current_stmt_parent();
        match helpers::delete_stmt(
            defined_by.into(),
            defined_in.map(std::convert::Into::into),
            &deleted,
            xxxxxxxx.locator,
        ) {
            Ok(fix) => {
                if fix.content.is_empty() || fix.content == "pass" {
                    xxxxxxxx.deletions.insert(defined_by.clone());
                }
                check.amend(fix);
            }
            Err(e) => error!("Failed to fix remove metaclass type: {e}"),
        }
    }
    xxxxxxxx.diagnostics.push(check);
}
