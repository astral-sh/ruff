use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_python_ast::{self as ast};
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;
use crate::fix;
use crate::{AlwaysFixableViolation, Fix};

/// ## What it does
/// Checks for the presence of the `pass` statement in non-empty class bodies
/// in `.pyi` files.
///
/// ## Why is this bad?
/// The `pass` statement is always unnecessary in non-empty class bodies in
/// stubs.
///
/// ## Example
/// ```pyi
/// class MyClass:
///     x: int
///     pass
/// ```
///
/// Use instead:
/// ```pyi
/// class MyClass:
///     x: int
/// ```
#[derive(ViolationMetadata)]
#[violation_metadata(stable_since = "v0.0.260")]
pub(crate) struct PassInClassBody;

impl AlwaysFixableViolation for PassInClassBody {
    #[derive_message_formats]
    fn message(&self) -> String {
        "Class body must not contain `pass`".to_string()
    }

    fn fix_title(&self) -> String {
        "Remove unnecessary `pass`".to_string()
    }
}

/// PYI012
pub(crate) fn pass_in_class_body(checker: &Checker, class_def: &ast::StmtClassDef) {
    // `pass` is required in these situations (or handled by `pass_statement_stub_body`).
    if class_def.body.len() < 2 {
        return;
    }

    for stmt in &class_def.body {
        if !stmt.is_pass_stmt() {
            continue;
        }

        let mut diagnostic = checker.report_diagnostic(PassInClassBody, stmt.range());
        let edit = fix::edits::delete_stmt(stmt, Some(stmt), checker.locator(), checker.indexer());
        diagnostic.set_fix(Fix::safe_edit(edit).isolate(Checker::isolation(
            checker.semantic().current_statement_id(),
        )));
    }
}
