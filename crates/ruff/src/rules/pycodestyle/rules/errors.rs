use crate::ast::types::Range;
use crate::define_violation;
use crate::registry::Diagnostic;
use crate::violation::Violation;
use ruff_macros::derive_message_formats;
use rustpython_parser::error::ParseError;

define_violation!(
    pub struct IOError {
        pub message: String,
    }
);
impl Violation for IOError {
    #[derive_message_formats]
    fn message(&self) -> String {
        let IOError { message } = self;
        format!("{message}")
    }
}

define_violation!(
    pub struct SyntaxError {
        pub message: String,
    }
);
impl Violation for SyntaxError {
    #[derive_message_formats]
    fn message(&self) -> String {
        let SyntaxError { message } = self;
        format!("SyntaxError: {message}")
    }
}

pub fn syntax_error(diagnostics: &mut Vec<Diagnostic>, parse_error: &ParseError) {
    diagnostics.push(Diagnostic::new(
        SyntaxError {
            message: parse_error.error.to_string(),
        },
        Range::new(parse_error.location, parse_error.location),
    ));
}
