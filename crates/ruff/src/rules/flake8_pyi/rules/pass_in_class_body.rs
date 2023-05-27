use log::error;
use rustpython_parser::ast::{Ranged, Stmt};

use ruff_diagnostics::{AlwaysAutofixableViolation, Diagnostic};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::types::RefEquality;

use crate::autofix::actions::delete_stmt;
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
        if stmt.is_pass_stmt() {
            let mut diagnostic = Diagnostic::new(PassInClassBody, stmt.range());

            if checker.patch(diagnostic.kind.rule()) {
                let deleted: Vec<&Stmt> = checker.deletions.iter().map(Into::into).collect();
                match delete_stmt(
                    stmt,
                    Some(parent),
                    &deleted,
                    checker.locator,
                    checker.indexer,
                    checker.stylist,
                ) {
                    Ok(fix) => {
                        if fix.is_deletion() || fix.content() == Some("pass") {
                            checker.deletions.insert(RefEquality(stmt));
                        }
                        #[allow(deprecated)]
                        diagnostic.set_fix_from_edit(fix);
                    }
                    Err(e) => {
                        error!("Failed to delete `pass` statement: {}", e);
                    }
                };
            };

            checker.diagnostics.push(diagnostic);
        }
    }
}
