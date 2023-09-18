use ruff_python_ast::Expr;

use ruff_diagnostics::{AutofixKind, Diagnostic, Edit, Fix, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;
use crate::registry::AsRule;

/// ## What it does
/// Checks for uses of `io.open`.
///
/// ## Why is this bad?
/// In Python 3, `io.open` is an alias for `open`. Prefer using `open` directly,
/// as it is more idiomatic.
///
/// ## Example
/// ```python
/// import io
///
/// with io.open("file.txt") as f:
///     ...
/// ```
///
/// Use instead:
/// ```python
/// with open("file.txt") as f:
///     ...
/// ```
///
/// ## References
/// - [Python documentation: `io.open`](https://docs.python.org/3/library/io.html#io.open)
#[violation]
pub struct OpenAlias;

impl Violation for OpenAlias {
    const AUTOFIX: AutofixKind = AutofixKind::Sometimes;

    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Use builtin `open`")
    }

    fn autofix_title(&self) -> Option<String> {
        Some("Replace with builtin `open`".to_string())
    }
}

/// UP020
pub(crate) fn open_alias(checker: &mut Checker, expr: &Expr, func: &Expr) {
    if checker
        .semantic()
        .resolve_call_path(func)
        .is_some_and(|call_path| matches!(call_path.as_slice(), ["io", "open"]))
    {
        let mut diagnostic = Diagnostic::new(OpenAlias, expr.range());
        if checker.patch(diagnostic.kind.rule()) {
            if checker.semantic().is_builtin("open") {
                diagnostic.set_fix(Fix::suggested(Edit::range_replacement(
                    "open".to_string(),
                    func.range(),
                )));
            }
        }
        checker.diagnostics.push(diagnostic);
    }
}
