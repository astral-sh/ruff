use rustpython_parser::ParseError;

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::types::Range;

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
pub fn syntax_error(diagnostics: &mut Vec<Diagnostic>, parse_error: &ParseError) {
    diagnostics.push(Diagnostic::new(
        SyntaxError {
            message: parse_error.error.to_string(),
        },
        Range::new(parse_error.location, parse_error.location),
    ));
}
