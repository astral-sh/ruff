use ruff_python_ast::Expr;

use ruff_diagnostics::{AutofixKind, Diagnostic, Edit, Fix, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;
use crate::registry::AsRule;

/// ## What it does
/// Checks for uses of `typing.Text`.
///
/// ## Why is this bad?
/// `typing.Text` is an alias for `str`, and only exists for Python 2
/// compatibility. As of Python 3.11, `typing.Text` is deprecated. Use `str`
/// instead.
///
/// ## Example
/// ```python
/// from typing import Text
///
/// foo: Text = "bar"
/// ```
///
/// Use instead:
/// ```python
/// foo: str = "bar"
/// ```
///
/// ## References
/// - [Python documentation: `typing.Text`](https://docs.python.org/3/library/typing.html#typing.Text)
#[violation]
pub struct TypingTextStrAlias;

impl Violation for TypingTextStrAlias {
    const AUTOFIX: AutofixKind = AutofixKind::Sometimes;

    #[derive_message_formats]
    fn message(&self) -> String {
        format!("`typing.Text` is deprecated, use `str`")
    }

    fn autofix_title(&self) -> Option<String> {
        Some("Replace with `str`".to_string())
    }
}

/// UP019
pub(crate) fn typing_text_str_alias(checker: &mut Checker, expr: &Expr) {
    if checker
        .semantic()
        .resolve_call_path(expr)
        .is_some_and(|call_path| matches!(call_path.as_slice(), ["typing", "Text"]))
    {
        let mut diagnostic = Diagnostic::new(TypingTextStrAlias, expr.range());
        if checker.patch(diagnostic.kind.rule()) {
            if checker.semantic().is_builtin("str") {
                diagnostic.set_fix(Fix::automatic(Edit::range_replacement(
                    "str".to_string(),
                    expr.range(),
                )));
            }
        }
        checker.diagnostics.push(diagnostic);
    }
}
