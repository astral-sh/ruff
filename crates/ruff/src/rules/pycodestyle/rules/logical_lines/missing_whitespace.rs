use itertools::Itertools;
use rustpython_parser::Tok;

use super::LogicalLine;
use ruff_diagnostics::Edit;
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
        format!("Missing whitespace after {token}")
    }

    fn autofix_title(&self) -> String {
        let MissingWhitespace { token } = self;
        format!("Added missing whitespace after {token}")
    }
}

/// E231
pub(crate) fn missing_whitespace(line: &LogicalLine, autofix: bool) -> Vec<Diagnostic> {
    let mut diagnostics = vec![];

    let mut num_lsqb = 0u32;
    let mut num_rsqb = 0u32;
    let mut prev_lsqb = None;
    let mut prev_lbrace = None;

    for (token, next_token) in line.tokens().iter().tuple_windows() {
        let kind = token.kind();
        match kind {
            Tok::Lsqb => {
                num_lsqb += 1;
                prev_lsqb = Some(token.start());
            }
            Tok::Rsqb => {
                num_rsqb += 1;
            }
            Tok::Lbrace => {
                prev_lbrace = Some(token.start());
            }

            Tok::Comma | Tok::Semi | Tok::Colon => {
                let after = line.text_after(&token);

                if !after.chars().next().map_or(false, char::is_whitespace) {
                    match (kind, next_token.kind()) {
                        (Tok::Colon, _) if num_lsqb > num_rsqb && prev_lsqb > prev_lbrace => {
                            continue; // Slice syntax, no space required
                        }
                        (Tok::Comma, Tok::Rpar | Tok::Rsqb) => {
                            continue; // Allow tuple with only one element: (3,)
                        }
                        (Tok::Colon, Tok::Equal) => {
                            continue; // Allow assignment expression
                        }
                        _ => {}
                    }

                    let kind = MissingWhitespace {
                        token: kind.to_string(),
                    };

                    let (start, end) = token.range();
                    let mut diagnostic = Diagnostic::new(kind, Range::new(start, start));

                    if autofix {
                        diagnostic.set_fix(Edit::insertion(" ".to_string(), end));
                    }
                    diagnostics.push(diagnostic);
                }
            }
            _ => {}
        }
    }
    diagnostics
}
