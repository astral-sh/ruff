use ruff_diagnostics::{AlwaysFixableViolation, Diagnostic, Edit, Fix};
use ruff_macros::{derive_message_formats, violation};
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
/// The [recommended style for functions and methods](https://typing.readthedocs.io/en/latest/source/stubs.html#functions-and-methods)
/// in the typing docs.
#[violation]
pub struct PassStatementStubBody;

impl AlwaysFixableViolation for PassStatementStubBody {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Empty body should contain `...`, not `pass`")
    }

    fn fix_title(&self) -> String {
        format!("Replace `pass` with `...`")
    }
}

/// PYI009
pub(crate) fn pass_statement_stub_body(checker: &mut Checker, body: &[Stmt]) {
    let [Stmt::Pass(pass)] = body else {
        return;
    };

    let mut diagnostic = Diagnostic::new(PassStatementStubBody, pass.range());
    diagnostic.set_fix(Fix::safe_edit(Edit::range_replacement(
        format!("..."),
        pass.range(),
    )));
    checker.diagnostics.push(diagnostic);
}
