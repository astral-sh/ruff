use log::error;
use ruff_text_size::TextRange;
use rustpython_parser::ast::{Expr, ExprKind, Stmt};

use ruff_diagnostics::{AlwaysAutofixableViolation, Diagnostic, Fix};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::types::RefEquality;

use crate::autofix::actions;
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

fn rule(targets: &[Expr], value: &Expr, location: TextRange) -> Option<Diagnostic> {
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
        rule(targets, value, stmt.range()) else {
            return;
        };
    if checker.patch(diagnostic.kind.rule()) {
        let deleted: Vec<&Stmt> = checker.deletions.iter().map(Into::into).collect();
        let defined_by = checker.ctx.stmt();
        let defined_in = checker.ctx.stmt_parent();
        match actions::delete_stmt(
            defined_by,
            defined_in,
            &deleted,
            checker.locator,
            checker.indexer,
            checker.stylist,
        ) {
            Ok(edit) => {
                if edit.is_deletion() || edit.content() == Some("pass") {
                    checker.deletions.insert(RefEquality(defined_by));
                }
                #[allow(deprecated)]
                diagnostic.set_fix(Fix::unspecified(edit));
            }
            Err(e) => error!("Failed to fix remove metaclass type: {e}"),
        }
    }
    checker.diagnostics.push(diagnostic);
}
