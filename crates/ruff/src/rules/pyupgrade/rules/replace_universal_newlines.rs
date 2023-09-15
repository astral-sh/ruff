use ruff_diagnostics::{AlwaysAutofixableViolation, Diagnostic, Edit, Fix};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast as ast;
use ruff_text_size::Ranged;

use crate::autofix::edits::{remove_argument, Parentheses};
use crate::checkers::ast::Checker;
use crate::registry::AsRule;

/// ## What it does
/// Checks for uses of `subprocess.run` that set the `universal_newlines`
/// keyword argument.
///
/// ## Why is this bad?
/// As of Python 3.7, the `universal_newlines` keyword argument has been
/// renamed to `text`, and now exists for backwards compatibility. The
/// `universal_newlines` keyword argument may be removed in a future version of
/// Python. Prefer `text`, which is more explicit and readable.
///
/// ## Example
/// ```python
/// import subprocess
///
/// subprocess.run(["foo"], universal_newlines=True)
/// ```
///
/// Use instead:
/// ```python
/// import subprocess
///
/// subprocess.run(["foo"], text=True)
/// ```
///
/// ## References
/// - [Python 3.7 release notes](https://docs.python.org/3/whatsnew/3.7.html#subprocess)
/// - [Python documentation: `subprocess.run`](https://docs.python.org/3/library/subprocess.html#subprocess.run)
#[violation]
pub struct ReplaceUniversalNewlines;

impl AlwaysAutofixableViolation for ReplaceUniversalNewlines {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("`universal_newlines` is deprecated, use `text`")
    }

    fn autofix_title(&self) -> String {
        "Replace with `text` keyword argument".to_string()
    }
}

/// UP021
pub(crate) fn replace_universal_newlines(checker: &mut Checker, call: &ast::ExprCall) {
    if checker
        .semantic()
        .resolve_call_path(&call.func)
        .is_some_and(|call_path| matches!(call_path.as_slice(), ["subprocess", "run"]))
    {
        let Some(kwarg) = call.arguments.find_keyword("universal_newlines") else {
            return;
        };

        let Some(arg) = kwarg.arg.as_ref() else {
            return;
        };

        let mut diagnostic = Diagnostic::new(ReplaceUniversalNewlines, arg.range());

        if checker.patch(diagnostic.kind.rule()) {
            if call.arguments.find_keyword("text").is_some() {
                diagnostic.try_set_fix(|| {
                    remove_argument(
                        kwarg,
                        &call.arguments,
                        Parentheses::Preserve,
                        checker.locator().contents(),
                    )
                    .map(Fix::suggested)
                });
            } else {
                diagnostic.set_fix(Fix::suggested(Edit::range_replacement(
                    "text".to_string(),
                    arg.range(),
                )));
            }
        }
        checker.diagnostics.push(diagnostic);
    }
}
