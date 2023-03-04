#![allow(dead_code, unused_imports, unused_variables)]

use rustpython_parser::ast::Location;
use rustpython_parser::Tok;

use ruff_macros::{define_violation, derive_message_formats};

use crate::registry::DiagnosticKind;
use crate::rules::pycodestyle::helpers::{is_keyword_token, is_singleton_token};
use crate::violation::AlwaysAutofixableViolation;
use crate::violation::Violation;

use crate::ast::types::Range;
use crate::fix::Fix;
use crate::registry::Diagnostic;

define_violation!(
    pub struct MissingWhitespace {
        pub token: String,
    }
);
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

fn rfind(line: &str, char: char) -> i16 {
    // emulate python's rfind
    match line.find(char) {
        Some(idx) => idx as i16,
        None => -1,
    }
}

/// E231
#[cfg(feature = "logical_lines")]
pub fn missing_whitespace(line: &str, row: usize, autofix: bool) -> Vec<Diagnostic> {
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
                && rfind(before, '{') < rfind(before, '[')
            {
                continue; // Slice syntax, no space required
            }
            if char == ',' && ")]".contains(char) {
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
                Range::new(Location::new(row, idx), Location::new(row, idx)),
            );

            if autofix {
                diagnostic.amend(Fix::insertion(" ".to_string(), Location::new(row, idx + 1)));
            }
            diagnostics.push(diagnostic);
        }
    }
    diagnostics
}

#[cfg(not(feature = "logical_lines"))]
pub fn missing_whitespace_after_keyword(_tokens: &str) -> Vec<(Location, DiagnosticKind)> {
    vec![]
}
