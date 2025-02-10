use ruff_diagnostics::{AlwaysFixableViolation, Diagnostic, Edit, Fix};
use ruff_macros::{derive_message_formats, ViolationMetadata};
use ruff_python_ast::Decorator;
use ruff_python_trivia::is_python_whitespace;
use ruff_text_size::{Ranged, TextRange, TextSize};

use crate::checkers::ast::Checker;

/// ## What it does
/// Checks for trailing whitespace after a decorator's opening `@`.
///
/// ## Why is this bad?
/// Including whitespace after the `@` symbol is not compliant with
/// [PEP 8].
///
/// ## Example
///
/// ```python
/// @ decorator
/// def func():
///    pass
/// ```
///
/// Use instead:
/// ```python
/// @decorator
/// def func():
///   pass
/// ```
///
/// [PEP 8]: https://peps.python.org/pep-0008/#maximum-line-length

#[derive(ViolationMetadata)]
pub(crate) struct WhitespaceAfterDecorator;

impl AlwaysFixableViolation for WhitespaceAfterDecorator {
    #[derive_message_formats]
    fn message(&self) -> String {
        "Whitespace after decorator".to_string()
    }

    fn fix_title(&self) -> String {
        "Remove whitespace".to_string()
    }
}

/// E204
pub(crate) fn whitespace_after_decorator(checker: &Checker, decorator_list: &[Decorator]) {
    for decorator in decorator_list {
        let decorator_text = checker.locator().slice(decorator);

        // Determine whether the `@` is followed by whitespace.
        if let Some(trailing) = decorator_text.strip_prefix('@') {
            // Collect the whitespace characters after the `@`.
            if trailing.chars().next().is_some_and(is_python_whitespace) {
                let end = trailing
                    .chars()
                    .position(|c| !(is_python_whitespace(c) || matches!(c, '\n' | '\r' | '\\')))
                    .unwrap_or(trailing.len());

                let start = decorator.start() + TextSize::from(1);
                let end = start + TextSize::try_from(end).unwrap();
                let range = TextRange::new(start, end);

                let mut diagnostic = Diagnostic::new(WhitespaceAfterDecorator, range);
                diagnostic.set_fix(Fix::safe_edit(Edit::range_deletion(range)));
                checker.report_diagnostic(diagnostic);
            }
        }
    }
}
