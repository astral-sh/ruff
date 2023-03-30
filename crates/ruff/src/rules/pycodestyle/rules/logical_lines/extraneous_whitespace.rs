use rustpython_parser::ast::Location;

use super::{LogicalLine, Whitespace};
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
pub(crate) fn extraneous_whitespace(line: &LogicalLine) -> Vec<(Location, DiagnosticKind)> {
    let mut diagnostics = vec![];
    let mut last_token: Option<TokenKind> = None;

    for token in line.tokens() {
        let kind = token.kind();
        match kind {
            TokenKind::Lbrace | TokenKind::Lpar | TokenKind::Lsqb => {
                if !matches!(line.trailing_whitespace(&token), Whitespace::None) {
                    let end = token.end();
                    diagnostics.push((
                        Location::new(end.row(), end.column()),
                        WhitespaceAfterOpenBracket.into(),
                    ));
                }
            }
            TokenKind::Rbrace
            | TokenKind::Rpar
            | TokenKind::Rsqb
            | TokenKind::Comma
            | TokenKind::Semi
            | TokenKind::Colon => {
                let diagnostic_kind =
                    if matches!(kind, TokenKind::Comma | TokenKind::Semi | TokenKind::Colon) {
                        DiagnosticKind::from(WhitespaceBeforePunctuation)
                    } else {
                        DiagnosticKind::from(WhitespaceBeforeCloseBracket)
                    };

                if let (Whitespace::Single | Whitespace::Many | Whitespace::Tab, offset) =
                    line.leading_whitespace(&token)
                {
                    if !matches!(last_token, Some(TokenKind::Comma)) {
                        let start = token.start();
                        diagnostics.push((
                            Location::new(start.row(), start.column() - offset),
                            diagnostic_kind,
                        ));
                    }
                }
            }

            _ => {}
        }

        last_token = Some(kind);
    }

    diagnostics
}
