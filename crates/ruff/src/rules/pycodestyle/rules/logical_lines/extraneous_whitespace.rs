use ruff_text_size::TextRange;

use super::{LogicalLine, Whitespace};
use crate::checkers::logical_lines::LogicalLinesContext;
use ruff_diagnostics::DiagnosticKind;
use ruff_diagnostics::Violation;
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::token_kind::TokenKind;

/// ## What it does
/// Checks for the use of extraneous whitespace after "(".
///
/// ## Why is this bad?
/// PEP 8 recommends the omission of whitespace in the following cases:
/// - "Immediately inside parentheses, brackets or braces."
/// - "Immediately before a comma, semicolon, or colon."
///
/// ## Example
/// ```python
/// spam( ham[1], {eggs: 2})
/// spam(ham[ 1], {eggs: 2})
/// spam(ham[1], { eggs: 2})
/// ```
///
/// Use instead:
/// ```python
/// spam(ham[1], {eggs: 2})
/// ```
///
/// ## References
/// - [PEP 8](https://peps.python.org/pep-0008/#pet-peeves)
#[violation]
pub struct WhitespaceAfterOpenBracket;

impl Violation for WhitespaceAfterOpenBracket {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Whitespace after '('")
    }
}

/// ## What it does
/// Checks for the use of extraneous whitespace before ")".
///
/// ## Why is this bad?
/// PEP 8 recommends the omission of whitespace in the following cases:
/// - "Immediately inside parentheses, brackets or braces."
/// - "Immediately before a comma, semicolon, or colon."
///
/// ## Example
/// ```python
/// spam(ham[1], {eggs: 2} )
/// spam(ham[1 ], {eggs: 2})
/// spam(ham[1], {eggs: 2 })
/// ```
///
/// Use instead:
/// ```python
/// spam(ham[1], {eggs: 2})
/// ```
///
/// ## References
/// - [PEP 8](https://peps.python.org/pep-0008/#pet-peeves)
#[violation]
pub struct WhitespaceBeforeCloseBracket;

impl Violation for WhitespaceBeforeCloseBracket {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Whitespace before ')'")
    }
}

/// ## What it does
/// Checks for the use of extraneous whitespace before ",", ";" or ":".
///
/// ## Why is this bad?
/// PEP 8 recommends the omission of whitespace in the following cases:
/// - "Immediately inside parentheses, brackets or braces."
/// - "Immediately before a comma, semicolon, or colon."
///
/// ## Example
/// ```python
/// if x == 4: print(x, y); x, y = y , x
/// ```
///
/// Use instead:
/// ```python
/// if x == 4: print(x, y); x, y = y, x
/// ```
///
/// ## References
/// - [PEP 8](https://peps.python.org/pep-0008/#pet-peeves)
#[violation]
pub struct WhitespaceBeforePunctuation;

impl Violation for WhitespaceBeforePunctuation {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Whitespace before ',', ';', or ':'")
    }
}

/// E201, E202, E203
pub(crate) fn extraneous_whitespace(line: &LogicalLine, context: &mut LogicalLinesContext) {
    let mut last_token = TokenKind::EndOfFile;

    for token in line.tokens() {
        let kind = token.kind();
        match kind {
            TokenKind::Lbrace | TokenKind::Lpar | TokenKind::Lsqb => {
                if !matches!(line.trailing_whitespace(token), Whitespace::None) {
                    context.push(WhitespaceAfterOpenBracket, TextRange::empty(token.end()));
                }
            }
            TokenKind::Rbrace
            | TokenKind::Rpar
            | TokenKind::Rsqb
            | TokenKind::Comma
            | TokenKind::Semi
            | TokenKind::Colon => {
                if let (Whitespace::Single | Whitespace::Many | Whitespace::Tab, offset) =
                    line.leading_whitespace(token)
                {
                    if !matches!(last_token, TokenKind::Comma | TokenKind::EndOfFile) {
                        let diagnostic_kind = if matches!(
                            kind,
                            TokenKind::Comma | TokenKind::Semi | TokenKind::Colon
                        ) {
                            DiagnosticKind::from(WhitespaceBeforePunctuation)
                        } else {
                            DiagnosticKind::from(WhitespaceBeforeCloseBracket)
                        };

                        context.push(diagnostic_kind, TextRange::empty(token.start() - offset));
                    }
                }
            }

            _ => {}
        }

        last_token = kind;
    }
}
