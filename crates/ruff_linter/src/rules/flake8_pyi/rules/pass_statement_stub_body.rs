use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_python_ast::Stmt;
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;
use crate::{AlwaysFixableViolation, Edit, Fix};

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
/// - [Typing documentation - Writing and Maintaining Stub Files](https://typing.python.org/en/latest/guides/writing_stubs.html)
#[derive(ViolationMetadata)]
#[violation_metadata(stable_since = "v0.0.253")]
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

    let mut diagnostic = checker.report_diagnostic(PassStatementStubBody, pass.range());
    diagnostic.set_fix(Fix::safe_edit(Edit::range_replacement(
        "...".to_string(),
        pass.range(),
    )));
}
