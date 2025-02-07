use ruff_diagnostics::{AlwaysFixableViolation, Diagnostic, Edit, Fix};
use ruff_macros::{derive_message_formats, ViolationMetadata};
use ruff_python_ast::Stmt;
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;

/// ## What it does
/// Checks for `pass` statements in empty stub bodies.
///
/// ## Why is this bad?
/// For stylistic consistency, `...` should always be used rather than `pass`
/// in stub files.
///
/// ## Example
/// ```pyi
/// def foo(bar: int) -> list[int]: pass
/// ```
///
/// Use instead:
/// ```pyi
/// def foo(bar: int) -> list[int]: ...
/// ```
///
/// ## References
/// - [Typing documentation - Writing and Maintaining Stub Files](https://typing.readthedocs.io/en/latest/guides/writing_stubs.html)
#[derive(ViolationMetadata)]
pub(crate) struct PassStatementStubBody;

impl AlwaysFixableViolation for PassStatementStubBody {
    #[derive_message_formats]
    fn message(&self) -> String {
        "Empty body should contain `...`, not `pass`".to_string()
    }

    fn fix_title(&self) -> String {
        "Replace `pass` with `...`".to_string()
    }
}

/// PYI009
pub(crate) fn pass_statement_stub_body(checker: &Checker, body: &[Stmt]) {
    let [Stmt::Pass(pass)] = body else {
        return;
    };

    let mut diagnostic = Diagnostic::new(PassStatementStubBody, pass.range());
    diagnostic.set_fix(Fix::safe_edit(Edit::range_replacement(
        "...".to_string(),
        pass.range(),
    )));
    checker.report_diagnostic(diagnostic);
}
