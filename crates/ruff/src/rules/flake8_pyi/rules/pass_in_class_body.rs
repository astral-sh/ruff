use ruff_diagnostics::{AlwaysAutofixableViolation, Diagnostic, Fix};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::{self as ast};
use ruff_text_size::Ranged;

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
pub(crate) fn pass_in_class_body(checker: &mut Checker, class_def: &ast::StmtClassDef) {
    // `pass` is required in these situations (or handled by `pass_statement_stub_body`).
    if class_def.body.len() < 2 {
        return;
    }

    for stmt in &class_def.body {
        if !stmt.is_pass_stmt() {
            continue;
        }

        let mut diagnostic = Diagnostic::new(PassInClassBody, stmt.range());
        if checker.patch(diagnostic.kind.rule()) {
            let edit =
                autofix::edits::delete_stmt(stmt, Some(stmt), checker.locator(), checker.indexer());
            diagnostic.set_fix(Fix::automatic(edit).isolate(Checker::isolation(Some(
                checker.semantic().current_statement_id(),
            ))));
        }
        checker.diagnostics.push(diagnostic);
    }
}
