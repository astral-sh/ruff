use log::error;
use rustpython_parser::ast::{Expr, ExprKind, Stmt};

use ruff_diagnostics::{AlwaysAutofixableViolation, Diagnostic};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::types::Range;

use crate::autofix::helpers;
use crate::checkers::ast::Checker;
use crate::registry::AsRule;

#[violation]
pub struct UselessMetaclassType;

impl AlwaysAutofixableViolation for UselessMetaclassType {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("`__metaclass__ = type` is implied")
    }

    fn autofix_title(&self) -> String {
        "Remove `__metaclass__ = type`".to_string()
    }
}

fn rule(targets: &[Expr], value: &Expr, location: Range) -> Option<Diagnostic> {
    if targets.len() != 1 {
        return None;
    }
    let ExprKind::Name { id, .. } = targets.first().map(|expr| &expr.node).unwrap() else {
        return None;
    };
    if id != "__metaclass__" {
        return None;
    }
    let ExprKind::Name { id, .. } = &value.node else {
        return None;
    };
    if id != "type" {
        return None;
    }
    Some(Diagnostic::new(UselessMetaclassType, location))
}

/// UP001
pub fn useless_metaclass_type(checker: &mut Checker, stmt: &Stmt, value: &Expr, targets: &[Expr]) {
    let Some(mut diagnostic) =
        rule(targets, value, Range::from(stmt)) else {
            return;
        };
    if checker.patch(diagnostic.kind.rule()) {
        let deleted: Vec<&Stmt> = checker.deletions.iter().map(Into::into).collect();
        let defined_by = checker.ctx.current_stmt();
        let defined_in = checker.ctx.current_stmt_parent();
        match helpers::delete_stmt(
            defined_by.into(),
            defined_in.map(Into::into),
            &deleted,
            checker.locator,
            checker.indexer,
            checker.stylist,
        ) {
            Ok(fix) => {
                if fix.content.is_empty() || fix.content == "pass" {
                    checker.deletions.insert(defined_by.clone());
                }
                diagnostic.amend(fix);
            }
            Err(e) => error!("Failed to fix remove metaclass type: {e}"),
        }
    }
    checker.diagnostics.push(diagnostic);
}
