use ruff_diagnostics::{AlwaysFixableViolation, Applicability, Diagnostic, Fix};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast as ast;
use ruff_python_semantic::Modules;
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;
use crate::fix::edits::add_argument;

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
/// ## Fix safety
/// This rule's fix is marked as unsafe for function calls that contain
/// `**kwargs`, as adding a `check` keyword argument to such a call may lead
/// to a duplicate keyword argument error.
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
    if !checker.semantic().seen_module(Modules::SUBPROCESS) {
        return;
    }

    if checker
        .semantic()
        .resolve_qualified_name(&call.func)
        .is_some_and(|qualified_name| matches!(qualified_name.segments(), ["subprocess", "run"]))
    {
        if call.arguments.find_keyword("check").is_none() {
            let mut diagnostic = Diagnostic::new(SubprocessRunWithoutCheck, call.func.range());
            diagnostic.set_fix(Fix::applicable_edit(
                add_argument(
                    "check=False",
                    &call.arguments,
                    checker.comment_ranges(),
                    checker.locator().contents(),
                ),
                // If the function call contains `**kwargs`, mark the fix as unsafe.
                if call
                    .arguments
                    .keywords
                    .iter()
                    .any(|keyword| keyword.arg.is_none())
                {
                    Applicability::Unsafe
                } else {
                    Applicability::Safe
                },
            ));
            checker.diagnostics.push(diagnostic);
        }
    }
}
