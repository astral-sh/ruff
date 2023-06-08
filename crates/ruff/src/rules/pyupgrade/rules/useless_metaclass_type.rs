use rustpython_parser::ast::{self, Expr, Ranged, Stmt};

use ruff_diagnostics::{AlwaysAutofixableViolation, Diagnostic, Fix};
use ruff_macros::{derive_message_formats, violation};

use crate::autofix;
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

/// UP001
pub(crate) fn useless_metaclass_type(
    checker: &mut Checker,
    stmt: &Stmt,
    value: &Expr,
    targets: &[Expr],
) {
    if targets.len() != 1 {
        return;
    }
    let Expr::Name(ast::ExprName { id, .. }) = targets.first().unwrap() else {
        return ;
    };
    if id != "__metaclass__" {
        return;
    }
    let Expr::Name(ast::ExprName { id, .. }) = value else {
        return ;
    };
    if id != "type" {
        return;
    }

    let mut diagnostic = Diagnostic::new(UselessMetaclassType, stmt.range());
    if checker.patch(diagnostic.kind.rule()) {
        let stmt = checker.semantic_model().stmt();
        let parent = checker.semantic_model().stmt_parent();
        let edit = autofix::edits::delete_stmt(
            stmt,
            parent,
            checker.locator,
            checker.indexer,
            checker.stylist,
        );
        diagnostic.set_fix(Fix::automatic(edit).isolate(checker.isolation(parent)));
    }
    checker.diagnostics.push(diagnostic);
}
