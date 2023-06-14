use rustpython_parser::ast::{Expr, Ranged};

use ruff_diagnostics::{AlwaysAutofixableViolation, Diagnostic, Edit, Fix};
use ruff_macros::{derive_message_formats, violation};

use crate::checkers::ast::Checker;
use crate::registry::AsRule;

/// ## What it does
/// Checks for `typing.Text`.
///
/// ## Why is this bad?
/// `typing.Text` is an alias for `str` and exists only for Python 2
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

impl AlwaysAutofixableViolation for TypingTextStrAlias {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("`typing.Text` is deprecated, use `str`")
    }

    fn autofix_title(&self) -> String {
        "Replace with `str`".to_string()
    }
}

/// UP019
pub(crate) fn typing_text_str_alias(checker: &mut Checker, expr: &Expr) {
    if checker
        .semantic_model()
        .resolve_call_path(expr)
        .map_or(false, |call_path| {
            call_path.as_slice() == ["typing", "Text"]
        })
    {
        let mut diagnostic = Diagnostic::new(TypingTextStrAlias, expr.range());
        if checker.patch(diagnostic.kind.rule()) {
            #[allow(deprecated)]
            diagnostic.set_fix(Fix::unspecified(Edit::range_replacement(
                "str".to_string(),
                expr.range(),
            )));
        }
        checker.diagnostics.push(diagnostic);
    }
}
