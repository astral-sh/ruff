use ruff_diagnostics::{AlwaysFixableViolation, Diagnostic, Fix};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::{self as ast};
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;
use crate::fix;

/// ## What it does
/// Checks for the presence of the `pass` statement within a class body
/// in a stub (`.pyi`) file.
///
/// ## Why is this bad?
/// In stub files, class definitions are intended to provide type hints, but
/// are never actually evaluated. As such, it's unnecessary to include a `pass`
/// statement in a class body, since it has no effect.
///
/// Instead of `pass`, prefer `...` to indicate that the class body is empty
/// and adhere to common stub file conventions.
///
/// ## Example
/// ```python
/// class MyClass:
///     pass
/// ```
///
/// Use instead:
/// ```python
/// class MyClass:
///     ...
/// ```
///
/// ## References
/// - [Mypy documentation: Stub files](https://mypy.readthedocs.io/en/stable/stubs.html)
#[violation]
pub struct PassInClassBody;

impl AlwaysFixableViolation for PassInClassBody {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Class body must not contain `pass`")
    }

    fn fix_title(&self) -> String {
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
        let edit = fix::edits::delete_stmt(stmt, Some(stmt), checker.locator(), checker.indexer());
        diagnostic.set_fix(Fix::safe_edit(edit).isolate(Checker::isolation(
            checker.semantic().current_statement_id(),
        )));
        checker.diagnostics.push(diagnostic);
    }
}
