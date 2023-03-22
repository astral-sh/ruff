#![allow(dead_code, unused_imports, unused_variables)]

use rustpython_parser::ast::Location;
use rustpython_parser::Tok;

use ruff_diagnostics::DiagnosticKind;
use ruff_diagnostics::Fix;
use ruff_diagnostics::Violation;
use ruff_diagnostics::{AlwaysAutofixableViolation, Diagnostic};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::types::Range;

use crate::registry::AsRule;
use crate::rules::pycodestyle::helpers::{is_keyword_token, is_singleton_token};

#[violation]
pub struct MissingWhitespace {
    pub token: String,
}

impl AlwaysAutofixableViolation for MissingWhitespace {
    #[derive_message_formats]
    fn message(&self) -> String {
        let MissingWhitespace { token } = self;
        format!("Missing whitespace after '{token}'")
    }

    fn autofix_title(&self) -> String {
        let MissingWhitespace { token } = self;
        format!("Added missing whitespace after '{token}'")
    }
}

/// E231
#[cfg(debug_assertions)]
pub fn missing_whitespace(
    line: &str,
    row: usize,
    autofix: bool,
    indent_level: usize,
) -> Vec<Diagnostic> {
    let mut diagnostics = vec![];
    for (idx, char) in line.chars().enumerate() {
        if idx + 1 == line.len() {
            break;
        }
        let next_char = line.chars().nth(idx + 1).unwrap();

        if ",;:".contains(char) && !char::is_whitespace(next_char) {
            let before = &line[..idx];
            if char == ':'
                && before.matches('[').count() > before.matches(']').count()
                && before.rfind('{') < before.rfind('[')
            {
                continue; // Slice syntax, no space required
            }
            if char == ',' && ")]".contains(next_char) {
                continue; // Allow tuple with only one element: (3,)
            }
            if char == ':' && next_char == '=' {
                continue; // Allow assignment expression
            }

            let kind: MissingWhitespace = MissingWhitespace {
                token: char.to_string(),
            };

            let mut diagnostic = Diagnostic::new(
                kind,
                Range::new(
                    Location::new(row, indent_level + idx),
                    Location::new(row, indent_level + idx),
                ),
            );

            if autofix {
                diagnostic.amend(Fix::insertion(
                    " ".to_string(),
                    Location::new(row, indent_level + idx + 1),
                ));
            }
            diagnostics.push(diagnostic);
        }
    }
    diagnostics
}

#[cfg(not(debug_assertions))]
pub fn missing_whitespace(
    _line: &str,
    _row: usize,
    _autofix: bool,
    indent_level: usize,
) -> Vec<Diagnostic> {
    vec![]
}
