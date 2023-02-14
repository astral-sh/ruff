use ruff_macros::{define_violation, derive_message_formats};
use rustpython_parser::ast::{ExprKind, Stmt, StmtKind};

use crate::ast::helpers;
use crate::checkers::ast::Checker;
use crate::registry::Diagnostic;
use crate::violation::Violation;

define_violation!(
    pub struct FStringDocstring;
);
impl Violation for FStringDocstring {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!(
            "f-string used as docstring. This will be interpreted by python as a joined string \
             rather than a docstring."
        )
    }
}

/// B021
pub fn f_string_docstring(checker: &mut Checker, body: &[Stmt]) {
    let Some(stmt) = body.first() else {
        return;
    };
    let StmtKind::Expr { value } = &stmt.node else {
        return;
    };
    let ExprKind::JoinedStr { .. } = value.node else {
        return;
    };
    checker.diagnostics.push(Diagnostic::new(
        FStringDocstring,
        helpers::identifier_range(stmt, checker.locator),
    ));
}
