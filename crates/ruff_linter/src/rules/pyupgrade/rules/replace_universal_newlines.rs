use ruff_diagnostics::{AlwaysFixableViolation, Diagnostic, Edit, Fix};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast as ast;
use ruff_python_semantic::Modules;
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;
use crate::fix::edits::{remove_argument, Parentheses};

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

impl AlwaysFixableViolation for ReplaceUniversalNewlines {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("`universal_newlines` is deprecated, use `text`")
    }

    fn fix_title(&self) -> String {
        "Replace with `text` keyword argument".to_string()
    }
}

/// UP021
pub(crate) fn replace_universal_newlines(checker: &mut Checker, call: &ast::ExprCall) {
    if !checker.semantic().seen_module(Modules::SUBPROCESS) {
        return;
    }

    if checker
        .semantic()
        .resolve_qualified_name(&call.func)
        .is_some_and(|qualified_name| matches!(qualified_name.segments(), ["subprocess", "run"]))
    {
        let Some(kwarg) = call.arguments.find_keyword("universal_newlines") else {
            return;
        };

        let Some(arg) = kwarg.arg.as_ref() else {
            return;
        };

        let mut diagnostic = Diagnostic::new(ReplaceUniversalNewlines, arg.range());

        if call.arguments.find_keyword("text").is_some() {
            diagnostic.try_set_fix(|| {
                remove_argument(
                    kwarg,
                    &call.arguments,
                    Parentheses::Preserve,
                    checker.locator().contents(),
                )
                .map(Fix::safe_edit)
            });
        } else {
            diagnostic.set_fix(Fix::safe_edit(Edit::range_replacement(
                "text".to_string(),
                arg.range(),
            )));
        }
        checker.diagnostics.push(diagnostic);
    }
}
