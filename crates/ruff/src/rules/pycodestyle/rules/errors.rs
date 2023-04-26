use ruff_text_size::{TextLen, TextRange, TextSize};
use rustpython_parser::ParseError;

use crate::logging::DisplayParseErrorType;
use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::source_code::Locator;

#[violation]
pub struct IOError {
    pub message: String,
}

/// E902
impl Violation for IOError {
    #[derive_message_formats]
    fn message(&self) -> String {
        let IOError { message } = self;
        format!("{message}")
    }
}

#[violation]
pub struct SyntaxError {
    pub message: String,
}

impl Violation for SyntaxError {
    #[derive_message_formats]
    fn message(&self) -> String {
        let SyntaxError { message } = self;
        format!("SyntaxError: {message}")
    }
}

/// E901
pub fn syntax_error(
    diagnostics: &mut Vec<Diagnostic>,
    parse_error: &ParseError,
    locator: &Locator,
) {
    let rest = locator.after(parse_error.location);

    // Try to create a non-empty range so that the diagnostic can print a caret at the
    // right position. This requires that we retrieve the next character, if any, and take its length
    // to maintain char-boundaries.
    let len = rest
        .chars()
        .next()
        .map_or(TextSize::new(0), TextLen::text_len);

    diagnostics.push(Diagnostic::new(
        SyntaxError {
            message: format!("{}", DisplayParseErrorType::new(&parse_error.error)),
        },
        TextRange::at(parse_error.location, len),
    ));
}
