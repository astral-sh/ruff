use crate::autofix::helpers::delete_stmt;
use ruff_diagnostics::{AlwaysAutofixableViolation, Diagnostic};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::types::Range;

use crate::checkers::ast::Checker;

use crate::registry::AsRule;
use rustpython_parser::ast::{Stmt, StmtKind};

#[violation]
pub struct PassInClassBody;

impl AlwaysAutofixableViolation for PassInClassBody {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Class body must not contain `pass`")
    }

    fn autofix_title(&self) -> String {
        format!("Remove `pass` from the class body")
    }
}

/// PYI012
pub fn pass_in_class_body(checker: &mut Checker, body: &[Stmt]) {
        // pass is required in these situations
    if body.len() < 2 {
        return;
    }

    // Loops through all the Located in a ClassDef body and checks for
    // nodes to match StmtKind::Pass
    for located in body {
        if matches!(located.node, StmtKind::Pass) {
            let mut diagnostic = Diagnostic::new(PassInClassBody, Range::from(located));

            if checker.patch(diagnostic.kind.rule()) {
                diagnostic.try_amend(|| {
                    delete_stmt(
                        located,
                        None,
                        &[],
                        checker.locator,
                        checker.indexer,
                        checker.stylist,
                    )
                });
            };

            checker.diagnostics.push(diagnostic);
        }
    }
}
