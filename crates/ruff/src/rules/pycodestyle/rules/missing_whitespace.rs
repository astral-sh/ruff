#![allow(dead_code, unused_imports, unused_variables)]

use itertools::Itertools;
use rustpython_parser::ast::Location;

use ruff_diagnostics::Edit;
use ruff_diagnostics::Violation;
use ruff_diagnostics::{AlwaysAutofixableViolation, Diagnostic};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::types::Range;

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
#[cfg(feature = "logical_lines")]
pub fn missing_whitespace(
    line: &str,
    row: usize,
    autofix: bool,
    indent_level: usize,
) -> Vec<Diagnostic> {
    let mut diagnostics = vec![];

    let mut num_lsqb = 0u32;
    let mut num_rsqb = 0u32;
    let mut prev_lsqb = None;
    let mut prev_lbrace = None;
    for (idx, (char, next_char)) in line.chars().tuple_windows().enumerate() {
        match char {
            '[' => {
                num_lsqb += 1;
                prev_lsqb = Some(idx);
            }
            ']' => {
                num_rsqb += 1;
            }
            '{' => {
                prev_lbrace = Some(idx);
            }

            ',' | ';' | ':' if !next_char.is_whitespace() => {
                if char == ':' && num_lsqb > num_rsqb && prev_lsqb > prev_lbrace {
                    continue; // Slice syntax, no space required
                }
                if char == ',' && matches!(next_char, ')' | ']') {
                    continue; // Allow tuple with only one element: (3,)
                }
                if char == ':' && next_char == '=' {
                    continue; // Allow assignment expression
                }

                let kind = MissingWhitespace {
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
                    diagnostic.set_fix(Edit::insertion(
                        " ".to_string(),
                        Location::new(row, indent_level + idx + 1),
                    ));
                }
                diagnostics.push(diagnostic);
            }
            _ => {}
        }
    }
    diagnostics
}

#[cfg(not(feature = "logical_lines"))]
pub fn missing_whitespace(
    _line: &str,
    _row: usize,
    _autofix: bool,
    indent_level: usize,
) -> Vec<Diagnostic> {
    vec![]
}
