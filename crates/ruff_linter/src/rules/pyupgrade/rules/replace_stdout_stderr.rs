use anyhow::Result;

use ruff_diagnostics::{Diagnostic, Edit, Fix, FixAvailability, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::{self as ast, Keyword};
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;
use crate::fix::edits::{remove_argument, Parentheses};

/// ## What it does
/// Checks for uses of `subprocess.run` that send `stdout` and `stderr` to a
/// pipe.
///
/// ## Why is this bad?
/// As of Python 3.7, `subprocess.run` has a `capture_output` keyword argument
/// that can be set to `True` to capture `stdout` and `stderr` outputs. This is
/// equivalent to setting `stdout` and `stderr` to `subprocess.PIPE`, but is
/// more explicit and readable.
///
/// ## Example
/// ```python
/// import subprocess
///
/// subprocess.run(["foo"], stdout=subprocess.PIPE, stderr=subprocess.PIPE)
/// ```
///
/// Use instead:
/// ```python
/// import subprocess
///
/// subprocess.run(["foo"], capture_output=True)
/// ```
///
/// ## References
/// - [Python 3.7 release notes](https://docs.python.org/3/whatsnew/3.7.html#subprocess)
/// - [Python documentation: `subprocess.run`](https://docs.python.org/3/library/subprocess.html#subprocess.run)
#[violation]
pub struct ReplaceStdoutStderr;

impl Violation for ReplaceStdoutStderr {
    const FIX_AVAILABILITY: FixAvailability = FixAvailability::Sometimes;

    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Sending `stdout` and `stderr` to `PIPE` is deprecated, use `capture_output`")
    }

    fn fix_title(&self) -> Option<String> {
        Some("Replace with `capture_output` keyword argument".to_string())
    }
}

/// UP022
pub(crate) fn replace_stdout_stderr(checker: &mut Checker, call: &ast::ExprCall) {
    if checker
        .semantic()
        .resolve_call_path(&call.func)
        .is_some_and(|call_path| matches!(call_path.as_slice(), ["subprocess", "run"]))
    {
        // Find `stdout` and `stderr` kwargs.
        let Some(stdout) = call.arguments.find_keyword("stdout") else {
            return;
        };
        let Some(stderr) = call.arguments.find_keyword("stderr") else {
            return;
        };

        // Verify that they're both set to `subprocess.PIPE`.
        if !checker
            .semantic()
            .resolve_call_path(&stdout.value)
            .is_some_and(|call_path| matches!(call_path.as_slice(), ["subprocess", "PIPE"]))
            || !checker
                .semantic()
                .resolve_call_path(&stderr.value)
                .is_some_and(|call_path| matches!(call_path.as_slice(), ["subprocess", "PIPE"]))
        {
            return;
        }

        let mut diagnostic = Diagnostic::new(ReplaceStdoutStderr, call.range());
        if call.arguments.find_keyword("capture_output").is_none() {
            diagnostic
                .try_set_fix(|| generate_fix(stdout, stderr, call, checker.locator().contents()));
        }
        checker.diagnostics.push(diagnostic);
    }
}

/// Generate a [`Edit`] for a `stdout` and `stderr` [`Keyword`] pair.
fn generate_fix(
    stdout: &Keyword,
    stderr: &Keyword,
    call: &ast::ExprCall,
    source: &str,
) -> Result<Fix> {
    let (first, second) = if stdout.start() < stderr.start() {
        (stdout, stderr)
    } else {
        (stderr, stdout)
    };
    // Replace one argument with `capture_output=True`, and remove the other.
    Ok(Fix::unsafe_edits(
        Edit::range_replacement("capture_output=True".to_string(), first.range()),
        [remove_argument(
            second,
            &call.arguments,
            Parentheses::Preserve,
            source,
        )?],
    ))
}
