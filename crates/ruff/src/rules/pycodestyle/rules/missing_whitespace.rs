#![allow(dead_code, unused_imports, unused_variables)]

use itertools::Itertools;
use rustpython_parser::ast::Location;

use crate::rules::pycodestyle::logical_lines::{LogicalLine, LogicalLineTokens};
use crate::rules::pycodestyle::rules::Whitespace;
use ruff_diagnostics::Edit;
use ruff_diagnostics::Violation;
use ruff_diagnostics::{AlwaysAutofixableViolation, Diagnostic};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::source_code::Locator;
use ruff_python_ast::token_kind::TokenKind;
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
pub fn missing_whitespace(line: &LogicalLine, autofix: bool) -> Vec<Diagnostic> {
    let mut diagnostics = vec![];

    let mut num_lsqb = 0u32;
    let mut num_rsqb = 0u32;
    let mut prev_lsqb = None;
    let mut prev_lbrace = None;

    for (token, next_token) in line.tokens().iter().tuple_windows() {
        let kind = token.kind();
        match kind {
            TokenKind::Lsqb => {
                num_lsqb += 1;
                prev_lsqb = Some(token.start());
            }
            TokenKind::Rsqb => {
                num_rsqb += 1;
            }
            TokenKind::Lbrace => {
                prev_lbrace = Some(token.start());
            }

            TokenKind::Comma | TokenKind::Semi | TokenKind::Colon => {
                let after = line.text_after(&token);

                if !after.chars().next().map_or(false, char::is_whitespace) {
                    match (kind, next_token.kind()) {
                        (TokenKind::Colon, _) if num_lsqb > num_rsqb && prev_lsqb > prev_lbrace => {
                            continue; // Slice syntax, no space required
                        }
                        (TokenKind::Comma, TokenKind::Rpar | TokenKind::Rsqb) => {
                            continue; // Allow tuple with only one element: (3,)
                        }
                        (TokenKind::Colon, TokenKind::Equal) => {
                            continue; // Allow assignment expression
                        }
                        _ => {}
                    }

                    let kind = MissingWhitespace {
                        token: match kind {
                            TokenKind::Comma => ",",
                            TokenKind::Semi => ";",
                            TokenKind::Colon => ":",
                            _ => unreachable!(),
                        }
                        .to_string(),
                    };

                    let (start, end) = token.range();
                    let mut diagnostic = Diagnostic::new(kind, Range::new(start, start));

                    if autofix {
                        diagnostic.amend(Edit::insertion(" ".to_string(), end));
                    }
                    diagnostics.push(diagnostic);
                }
            }
            _ => {}
        }
    }
    diagnostics
}

#[cfg(not(feature = "logical_lines"))]
pub fn missing_whitespace(_line: &LogicalLine, _autofix: bool) -> Vec<Diagnostic> {
    vec![]
}
