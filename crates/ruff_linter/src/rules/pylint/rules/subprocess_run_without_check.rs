use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_python_ast as ast;
use ruff_python_semantic::Modules;
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;
use crate::fix::edits::add_argument;
use crate::{Fix, FixAvailability, Violation};

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
///
/// This rule's fix is marked as display-only because it's not clear whether the
/// potential exception was meant to be ignored by setting `check=False` or if
/// the author simply forgot to include `check=True`. The fix adds
/// `check=False`, making the existing behavior explicit but possibly masking
/// the original intention.
///
/// ## References
/// - [Python documentation: `subprocess.run`](https://docs.python.org/3/library/subprocess.html#subprocess.run)
#[derive(ViolationMetadata)]
#[violation_metadata(stable_since = "v0.0.285")]
pub(crate) struct SubprocessRunWithoutCheck;

impl Violation for SubprocessRunWithoutCheck {
    // The fix is always set on the diagnostic, but display-only fixes aren't
    // considered "fixable" in the tests.
    const FIX_AVAILABILITY: FixAvailability = FixAvailability::Sometimes;

    #[derive_message_formats]
    fn message(&self) -> String {
        "`subprocess.run` without explicit `check` argument".to_string()
    }

    fn fix_title(&self) -> Option<String> {
        Some("Add explicit `check=False`".to_string())
    }
}

/// PLW1510
pub(crate) fn subprocess_run_without_check(checker: &Checker, call: &ast::ExprCall) {
    if !checker.semantic().seen_module(Modules::SUBPROCESS) {
        return;
    }

    if checker
        .semantic()
        .resolve_qualified_name(&call.func)
        .is_some_and(|qualified_name| matches!(qualified_name.segments(), ["subprocess", "run"]))
    {
        if call.arguments.find_keyword("check").is_none() {
            let mut diagnostic =
                checker.report_diagnostic(SubprocessRunWithoutCheck, call.func.range());
            diagnostic.set_fix(Fix::display_only_edit(add_argument(
                "check=False",
                &call.arguments,
                checker.tokens(),
            )));
        }
    }
}
