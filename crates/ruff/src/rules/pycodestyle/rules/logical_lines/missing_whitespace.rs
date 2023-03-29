use super::LogicalLine;
use ruff_diagnostics::Edit;
use ruff_diagnostics::{AlwaysAutofixableViolation, Diagnostic};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::token_kind::TokenKind;
use ruff_python_ast::types::Range;

#[violation]
pub struct MissingWhitespace {
    pub token: TokenKind,
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
pub(crate) fn missing_whitespace(line: &LogicalLine, autofix: bool) -> Vec<Diagnostic> {
    let mut diagnostics = vec![];

    let mut open_parentheses = 0u32;
    let mut prev_lsqb = None;
    let mut prev_lbrace = None;
    let mut iter = line.tokens().iter().peekable();

    while let Some(token) = iter.next() {
        let kind = token.kind();
        match kind {
            TokenKind::Lsqb => {
                open_parentheses += 1;
                prev_lsqb = Some(token.start());
            }
            TokenKind::Rsqb => {
                open_parentheses += 1;
            }
            TokenKind::Lbrace => {
                prev_lbrace = Some(token.start());
            }

            TokenKind::Comma | TokenKind::Semi | TokenKind::Colon => {
                let after = line.text_after(&token);

                if !after.chars().next().map_or(false, char::is_whitespace) {
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
