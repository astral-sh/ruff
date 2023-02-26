use ruff_macros::{define_violation, derive_message_formats};
use rustpython_parser::ast::{Constant, ExprKind, Located, StmtKind};

use crate::ast::types::Range;
use crate::checkers::ast::Checker;
use crate::registry::Diagnostic;
use crate::violation::Violation;

define_violation!(
    pub struct BanDocStringsInStubs;
);
impl Violation for BanDocStringsInStubs {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Docstrings should not be included in stubs")
    }
}

/// PYI021
pub fn ban_doc_strings_in_stubs(checker: &mut Checker, ast: &Vec<Located<StmtKind>>) {
    if let Some(stmt) = ast.first() {
        if let StmtKind::Expr { value } = &stmt.node {
            if let ExprKind::Constant {
                value: Constant::Str(_),
                ..
            } = &value.node
            {
                checker.diagnostics.push(Diagnostic::new(
                    BanDocStringsInStubs,
                    Range::from_located(value),
                ));
            }
        }
    }
}
