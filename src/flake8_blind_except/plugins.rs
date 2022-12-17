use rustpython_ast::{Expr, ExprKind, Stmt, StmtKind};

use crate::ast::types::Range;
use crate::checkers::ast::Checker;
use crate::checks::{Check, CheckKind};

/// BLE001
pub fn blind_except(
    checker: &mut Checker,
    type_: Option<&Expr>,
    name: Option<&str>,
    body: &[Stmt],
) {
    let Some(type_) = type_ else {
        return;
    };
    let ExprKind::Name { id, .. } = &type_.node else {
        return;
    };
    for exception in ["BaseException", "Exception"] {
        if id == exception && checker.is_builtin(exception) {
            // If the exception is re-raised, don't flag an error.
            if !body.iter().any(|stmt| {
                if let StmtKind::Raise { exc, .. } = &stmt.node {
                    if let Some(exc) = exc {
                        if let ExprKind::Name { id, .. } = &exc.node {
                            name.map_or(false, |name| name == id)
                        } else {
                            false
                        }
                    } else {
                        true
                    }
                } else {
                    false
                }
            }) {
                checker.add_check(Check::new(
                    CheckKind::BlindExcept(id.to_string()),
                    Range::from_located(type_),
                ));
            }
        }
    }
}
