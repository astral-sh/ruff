use ruff_diagnostics::Edit;
use ruff_diagnostics::{AlwaysAutofixableViolation, Diagnostic, Fix};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_parser::TokenKind;
use ruff_text_size::{Ranged, TextSize};

use crate::checkers::logical_lines::LogicalLinesContext;

use super::LogicalLine;

/// ## What it does
/// Checks for missing whitespace after `,`, `;`, and `:`.
///
/// ## Why is this bad?
/// Missing whitespace after `,`, `;`, and `:` makes the code harder to read.
///
/// ## Example
/// ```python
/// a = (1,2)
/// ```
///
/// Use instead:
/// ```python
/// a = (1, 2)
/// ```
#[violation]
pub struct MissingWhitespace {
    token: TokenKind,
}

impl MissingWhitespace {
    fn token_text(&self) -> char {
        match self.token {
            TokenKind::Colon => ':',
            TokenKind::Semi => ';',
            TokenKind::Comma => ',',
            _ => unreachable!(),
        }
    }
}

impl AlwaysAutofixableViolation for MissingWhitespace {
    #[derive_message_formats]
    fn message(&self) -> String {
        let token = self.token_text();
        format!("Missing whitespace after '{token}'")
    }

    fn autofix_title(&self) -> String {
        let token = self.token_text();
        format!("Added missing whitespace after '{token}'")
    }
}

/// E231
pub(crate) fn missing_whitespace(
    line: &LogicalLine,
    autofix: bool,
    context: &mut LogicalLinesContext,
) {
    let mut open_parentheses = 0u32;
    let mut prev_lsqb = TextSize::default();
    let mut prev_lbrace = TextSize::default();
    let mut iter = line.tokens().iter().peekable();

    while let Some(token) = iter.next() {
        let kind = token.kind();
        match kind {
            TokenKind::Lsqb => {
                open_parentheses = open_parentheses.saturating_add(1);
                prev_lsqb = token.start();
            }
            TokenKind::Rsqb => {
                open_parentheses = open_parentheses.saturating_sub(1);
            }
            TokenKind::Lbrace => {
                prev_lbrace = token.start();
            }

            TokenKind::Comma | TokenKind::Semi | TokenKind::Colon => {
                let after = line.text_after(token);

                if after
                    .chars()
                    .next()
                    .is_some_and(|c| !(char::is_whitespace(c) || c == '\\'))
                {
                    if let Some(next_token) = iter.peek() {
                        match (kind, next_token.kind()) {
                            (TokenKind::Colon, _)
                                if open_parentheses > 0 && prev_lsqb > prev_lbrace =>
                            {
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
                    }

                    let kind = MissingWhitespace { token: kind };
                    let mut diagnostic = Diagnostic::new(kind, token.range());

                    if autofix {
                        diagnostic.set_fix(Fix::automatic(Edit::insertion(
                            " ".to_string(),
                            token.end(),
                        )));
                    }
                    context.push_diagnostic(diagnostic);
                }
            }
            _ => {}
        }
    }
}
