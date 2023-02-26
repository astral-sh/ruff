use ruff_macros::{define_violation, derive_message_formats};
use rustpython_parser::ast::{Located, StmtKind};

use crate::ast::types::Range;
use crate::checkers::ast::Checker;
use crate::registry::Diagnostic;
use crate::violation::Violation;

define_violation!(
    pub struct PreferEllipsisOverPass;
);
impl Violation for PreferEllipsisOverPass {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Empty body should contain '...', not 'pass'")
    }
}

/// PYI009
pub fn prefer_ellipsis_over_pass(checker: &mut Checker, body: &Vec<Located<StmtKind>>) {
    if body.len() != 1 {
        return;
    }
    if body[0].node == StmtKind::Pass {
        checker.diagnostics.push(Diagnostic::new(
            PreferEllipsisOverPass,
            Range::from_located(&body[0]),
        ));
    }
}
