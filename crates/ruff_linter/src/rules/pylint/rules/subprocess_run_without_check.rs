use crate::fix::edits::add_argument;
use ruff_diagnostics::{AlwaysFixableViolation, Diagnostic, Fix};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast as ast;
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;

/// ## What it does
/// Checks for uses of `subprocess.run` without an explicit `check` argument.
///
/// ## Why is this bad?
/// By default, `subprocess.run` does not check the return code of the process
/// it runs. This can lead to silent failures.
///
/// Instead, consider using `check=True` to raise an exception if the process
/// fails, or set `check=False` explicitly to mark the behavior as intentional.
///
/// ## Example
/// ```python
/// import subprocess
///
/// subprocess.run(["ls", "nonexistent"])  # No exception raised.
/// ```
///
/// Use instead:
/// ```python
/// import subprocess
///
/// subprocess.run(["ls", "nonexistent"], check=True)  # Raises exception.
/// ```
///
/// Or:
/// ```python
/// import subprocess
///
/// subprocess.run(["ls", "nonexistent"], check=False)  # Explicitly no check.
/// ```
///
/// ## References
/// - [Python documentation: `subprocess.run`](https://docs.python.org/3/library/subprocess.html#subprocess.run)
#[violation]
pub struct SubprocessRunWithoutCheck;

impl AlwaysFixableViolation for SubprocessRunWithoutCheck {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("`subprocess.run` without explicit `check` argument")
    }

    fn fix_title(&self) -> String {
        "Add explicit `check=False`".to_string()
    }
}

/// PLW1510
pub(crate) fn subprocess_run_without_check(checker: &mut Checker, call: &ast::ExprCall) {
    if checker
        .semantic()
        .resolve_call_path(&call.func)
        .is_some_and(|call_path| matches!(call_path.as_slice(), ["subprocess", "run"]))
    {
        if call.arguments.find_keyword("check").is_none() {
            let mut diagnostic = Diagnostic::new(SubprocessRunWithoutCheck, call.func.range());
            diagnostic.set_fix(Fix::safe_edit(add_argument(
                "check=False",
                &call.arguments,
                checker.indexer().comment_ranges(),
                checker.locator().contents(),
            )));
            checker.diagnostics.push(diagnostic);
        }
    }
}
