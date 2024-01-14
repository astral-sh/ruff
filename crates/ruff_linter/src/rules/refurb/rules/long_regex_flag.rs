use ruff_diagnostics::{AlwaysFixableViolation, Diagnostic, Edit, Fix};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::Expr;
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;

/// ## What it does
/// Checks for uses of shorthand regex flags such as `re.I`.
///
/// ## Why is this bad?
/// These single-character flags are not as descriptive as the full names.
///
/// ## Example
/// ```python
/// if re.match("^hello", "hello world", re.I):
///     ...
/// ```
///
/// Use instead:
/// ```python
/// if re.match("^hello", "hello world", re.IGNORECASE):
///     ...
/// ```
///
#[violation]
pub struct LongRegexFlag {
    short: &'static str,
    long: &'static str,
}

impl AlwaysFixableViolation for LongRegexFlag {
    #[derive_message_formats]
    fn message(&self) -> String {
        let LongRegexFlag { short, .. } = self;
        format!("Use of shorthand `re.{short}`")
    }

    fn fix_title(&self) -> String {
        let LongRegexFlag { long, .. } = self;
        format!("Replace with `re.{long}`")
    }
}

/// FURB167
pub(crate) fn long_regex_flag(checker: &mut Checker, expr: &Expr) {
    let Some((short, long)) = checker
        .semantic()
        .resolve_call_path(expr)
        .and_then(|call_path| match call_path.as_slice() {
            ["re", "A"] => Some(("A", "ASCII")),
            ["re", "I"] => Some(("I", "IGNORECASE")),
            ["re", "L"] => Some(("L", "LOCALE")),
            ["re", "M"] => Some(("M", "MULTILINE")),
            ["re", "S"] => Some(("S", "DOTALL")),
            ["re", "T"] => Some(("T", "TEMPLATE")),
            ["re", "U"] => Some(("U", "UNICODE")),
            ["re", "X"] => Some(("X", "VERBOSE")),
            _ => None,
        })
    else {
        return;
    };

    let mut diagnostic = Diagnostic::new(LongRegexFlag { short, long }, expr.range());

    diagnostic.set_fix(Fix::safe_edit(Edit::replacement(
        format!("re.{long}"),
        expr.start(),
        expr.end(),
    )));

    checker.diagnostics.push(diagnostic);
}
