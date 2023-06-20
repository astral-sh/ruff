use rustpython_parser::ast::{Ranged, Stmt};

use ruff_diagnostics::{AlwaysAutofixableViolation, Diagnostic, Fix};
use ruff_macros::{derive_message_formats, violation};

use crate::autofix;
use crate::checkers::ast::Checker;
use crate::registry::AsRule;

#[violation]
pub struct PassInClassBody;

impl AlwaysAutofixableViolation for PassInClassBody {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Class body must not contain `pass`")
    }

    fn autofix_title(&self) -> String {
        format!("Remove unnecessary `pass`")
    }
}

/// PYI012
pub(crate) fn pass_in_class_body<'a>(
    checker: &mut Checker<'a>,
    parent: &'a Stmt,
    body: &'a [Stmt],
) {
    // `pass` is required in these situations (or handled by `pass_statement_stub_body`).
    if body.len() < 2 {
        return;
    }

    for stmt in body {
        if !stmt.is_pass_stmt() {
            continue;
        }

        let mut diagnostic = Diagnostic::new(PassInClassBody, stmt.range());
        if checker.patch(diagnostic.kind.rule()) {
            let edit =
                autofix::edits::delete_stmt(stmt, Some(parent), checker.locator, checker.indexer);
            diagnostic.set_fix(Fix::automatic(edit).isolate(checker.isolation(Some(parent))));
        }
        checker.diagnostics.push(diagnostic);
    }
}
