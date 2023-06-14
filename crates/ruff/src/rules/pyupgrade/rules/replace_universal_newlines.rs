use ruff_text_size::{TextLen, TextRange};
use rustpython_parser::ast::{Expr, Keyword, Ranged};

use ruff_diagnostics::{AlwaysAutofixableViolation, Diagnostic, Edit, Fix};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::helpers::find_keyword;

use crate::checkers::ast::Checker;
use crate::registry::AsRule;

/// ## What it does
/// Checks for `universal_newlines` keyword argument in `subprocess.run` calls.
///
/// ## Why is this bad?
/// As of Python 3.7, `text` is an alias for `universal_newlines`. `text` is
/// more understandable and explicit than `universal_newlines`.
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
pub(crate) fn replace_universal_newlines(checker: &mut Checker, func: &Expr, kwargs: &[Keyword]) {
    if checker
        .semantic()
        .resolve_call_path(func)
        .map_or(false, |call_path| {
            call_path.as_slice() == ["subprocess", "run"]
        })
    {
        let Some(kwarg) = find_keyword(kwargs, "universal_newlines") else { return; };
        let range = TextRange::at(kwarg.start(), "universal_newlines".text_len());
        let mut diagnostic = Diagnostic::new(ReplaceUniversalNewlines, range);
        if checker.patch(diagnostic.kind.rule()) {
            #[allow(deprecated)]
            diagnostic.set_fix(Fix::unspecified(Edit::range_replacement(
                "text".to_string(),
                range,
            )));
        }
        checker.diagnostics.push(diagnostic);
    }
}
