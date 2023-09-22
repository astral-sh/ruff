use ruff_diagnostics::{AlwaysAutofixableViolation, Diagnostic, Edit, Fix};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::Stmt;
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;
use crate::registry::Rule;

/// ## What it does
/// Checks for `pass` statements in empty stub bodies.
///
/// ## Why is this bad?
/// For consistency, empty stub bodies should contain `...` instead of `pass`.
///
/// Additionally, an ellipsis better conveys the intent of the stub body (that
/// the body has been implemented, but has been intentionally left blank to
/// document the interface).
///
/// ## Example
/// ```python
/// def foo(bar: int) -> list[int]:
///     pass
/// ```
///
/// Use instead:
/// ```python
/// def foo(bar: int) -> list[int]: ...
/// ```
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
    let [Stmt::Pass(pass)] = body else {
        return;
    };

    let mut diagnostic = Diagnostic::new(PassStatementStubBody, pass.range());
    if checker.patch(Rule::PassStatementStubBody) {
        diagnostic.set_fix(Fix::automatic(Edit::range_replacement(
            format!("..."),
            pass.range(),
        )));
    };
    checker.diagnostics.push(diagnostic);
}
