use ruff_diagnostics::{AlwaysFixableViolation, Diagnostic};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::Decorator;
use ruff_python_trivia::is_python_whitespace;
use ruff_text_size::{TextLen, TextRange};

use crate::checkers::ast::Checker;

/// ## What it does
/// Checks for whitespace after a decorator.
///
/// ## Why is this bad?
///
///
///
/// ## Formatter compatibility
///
///
///
///
///
///
///
///

#[violation]
pub struct WhitespaceAfterDecorator;

impl AlwaysFixableViolation for WhitespaceAfterDecorator {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Whitespace after decorator")
    }

    fn fix_title(&self) -> String {
        "Remove whitespace after decorator".to_string()
    }
}

// Function that checks weather there is a whitespace after a decorator using
// the provided `Decorator` list and the functions
// You can use locator.slice(decorator) to get the text of the decorator.
// The last step is to find the @ and then test if whatever comes after is_python_whitespace.
pub(crate) fn whitespace_after_decorator(checker: &mut Checker, decorator_list: &[Decorator]) {
    // Get the locator from the checker
    let locator = checker.locator();

    // Iterate over the list of decorators
    for decorator in decorator_list {
        // Obtain the text of the decorator using lactor.slice(decorator)
        let decorator_text = locator.slice(decorator);

        // Get the text after the @ symbol
        let after_at = &decorator_text[1..];

        // Check if there is a whitespace after the @ symbol by using is_python_whitespace
        if is_python_whitespace(after_at.chars().next().unwrap()) {
            let range = TextRange::empty(locator.contents().text_len());
            checker
                .diagnostics
                .push(Diagnostic::new(WhitespaceAfterDecorator, range));
        }
    }
}
