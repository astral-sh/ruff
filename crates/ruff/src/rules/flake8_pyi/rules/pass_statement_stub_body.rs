use rustpython_parser::ast::{Ranged, Stmt};

use ruff_diagnostics::{AlwaysAutofixableViolation, Diagnostic, Edit, Fix};
use ruff_macros::{derive_message_formats, violation};

use crate::checkers::ast::Checker;
use crate::registry::Rule;

#[violation]
pub struct PassStatementStubBody;

impl AlwaysAutofixableViolation for PassStatementStubBody {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Empty body should contain `...`, not `pass`")
    }

    fn autofix_title(&self) -> String {
        format!("Replace `pass` with `...`")
    }
}

/// PYI009
pub(crate) fn pass_statement_stub_body(checker: &mut Checker, body: &[Stmt]) {
    if body.len() != 1 {
        return;
    }
    if body[0].is_pass_stmt() {
        let mut diagnostic = Diagnostic::new(PassStatementStubBody, body[0].range());
        if checker.patch(Rule::PassStatementStubBody) {
            diagnostic.set_fix(Fix::automatic(Edit::range_replacement(
                format!("..."),
                body[0].range(),
            )));
        };
        checker.diagnostics.push(diagnostic);
    }
}
