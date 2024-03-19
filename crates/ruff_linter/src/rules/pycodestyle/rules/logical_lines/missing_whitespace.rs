use ruff_diagnostics::Edit;
use ruff_diagnostics::{AlwaysFixableViolation, Diagnostic, Fix};
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

impl AlwaysFixableViolation for MissingWhitespace {
    #[derive_message_formats]
    fn message(&self) -> String {
        let token = self.token_text();
        format!("Missing whitespace after '{token}'")
    }

    fn fix_title(&self) -> String {
        format!("Add missing whitespace")
    }
}

/// E231
pub(crate) fn missing_whitespace(line: &LogicalLine, context: &mut LogicalLinesContext) {
    let mut fstrings = 0u32;
    let mut lsqb_stack = vec![TextSize::default()];
    let mut lbrace_stack = vec![TextSize::default()];
    let mut iter = line.tokens().iter().peekable();

    while let Some(token) = iter.next() {
        let kind = token.kind();
        match kind {
            TokenKind::FStringStart => fstrings += 1,
            TokenKind::FStringEnd => fstrings = fstrings.saturating_sub(1),
            TokenKind::Lsqb if fstrings == 0 => {
                lsqb_stack.push(token.start());
            }
            TokenKind::Rsqb if fstrings == 0 => {
                lsqb_stack.pop();
            }
            TokenKind::Lbrace if fstrings == 0 => {
                lbrace_stack.push(token.start());
            }
            TokenKind::Rbrace if fstrings == 0 => {
                lbrace_stack.pop();
            }
            TokenKind::Colon if fstrings > 0 => {
                // Colon in f-string, no space required. This will yield false
                // negatives for cases like the following as it's hard to
                // differentiate between the usage of a colon in a f-string.
                //
                // ```python
                // f'{ {'x':1} }'
                // f'{(lambda x:x)}'
                // ```
                continue;
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
                            (TokenKind::Colon, _) if lsqb_stack.last() > lbrace_stack.last() => {
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

                    let mut diagnostic =
                        Diagnostic::new(MissingWhitespace { token: kind }, token.range());
                    diagnostic.set_fix(Fix::safe_edit(Edit::insertion(
                        " ".to_string(),
                        token.end(),
                    )));
                    context.push_diagnostic(diagnostic);
                }
            }
            _ => {}
        }
    }
}
