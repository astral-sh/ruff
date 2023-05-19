use ruff_text_size::TextRange;

use ruff_diagnostics::DiagnosticKind;
use ruff_diagnostics::Violation;
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::token_kind::TokenKind;

use crate::checkers::logical_lines::LogicalLinesContext;

use super::{LogicalLine, Whitespace};

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
pub struct WhitespaceAfterOpenBracket {
    symbol: char,
}

impl Violation for WhitespaceAfterOpenBracket {
    #[derive_message_formats]
    fn message(&self) -> String {
        let WhitespaceAfterOpenBracket { symbol } = self;
        format!("Whitespace after '{symbol}'")
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
pub struct WhitespaceBeforeCloseBracket {
    symbol: char,
}

impl Violation for WhitespaceBeforeCloseBracket {
    #[derive_message_formats]
    fn message(&self) -> String {
        let WhitespaceBeforeCloseBracket { symbol } = self;
        format!("Whitespace before '{symbol}'")
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
pub struct WhitespaceBeforePunctuation {
    symbol: char,
}

impl Violation for WhitespaceBeforePunctuation {
    #[derive_message_formats]
    fn message(&self) -> String {
        let WhitespaceBeforePunctuation { symbol } = self;
        format!("Whitespace before '{symbol}'")
    }
}

/// E201, E202, E203
pub(crate) fn extraneous_whitespace(line: &LogicalLine, context: &mut LogicalLinesContext) {
    let mut prev_token = None;

    for token in line.tokens() {
        let kind = token.kind();
        if let Some(symbol) = BracketOrPunctuation::from_kind(kind) {
            match symbol {
                BracketOrPunctuation::OpenBracket(symbol) => {
                    let (trailing, trailing_len) = line.trailing_whitespace(token);
                    if !matches!(trailing, Whitespace::None) {
                        context.push(
                            WhitespaceAfterOpenBracket { symbol },
                            TextRange::at(token.end(), trailing_len),
                        );
                    }
                }
                BracketOrPunctuation::CloseBracket(symbol) => {
                    if !matches!(prev_token, Some(TokenKind::Comma)) {
                        if let (Whitespace::Single | Whitespace::Many | Whitespace::Tab, offset) =
                            line.leading_whitespace(token)
                        {
                            context.push(
                                DiagnosticKind::from(WhitespaceBeforeCloseBracket { symbol }),
                                TextRange::at(token.start() - offset, offset),
                            );
                        }
                    }
                }
                BracketOrPunctuation::Punctuation(symbol) => {
                    if !matches!(prev_token, Some(TokenKind::Comma)) {
                        if let (Whitespace::Single | Whitespace::Many | Whitespace::Tab, offset) =
                            line.leading_whitespace(token)
                        {
                            context.push(
                                DiagnosticKind::from(WhitespaceBeforePunctuation { symbol }),
                                TextRange::at(token.start() - offset, offset),
                            );
                        }
                    }
                }
            }
        }

        prev_token = Some(kind);
    }
}

#[derive(Debug)]
enum BracketOrPunctuation {
    OpenBracket(char),
    CloseBracket(char),
    Punctuation(char),
}

impl BracketOrPunctuation {
    fn from_kind(kind: TokenKind) -> Option<BracketOrPunctuation> {
        match kind {
            TokenKind::Lbrace => Some(BracketOrPunctuation::OpenBracket('{')),
            TokenKind::Lpar => Some(BracketOrPunctuation::OpenBracket('(')),
            TokenKind::Lsqb => Some(BracketOrPunctuation::OpenBracket('[')),
            TokenKind::Rbrace => Some(BracketOrPunctuation::CloseBracket('}')),
            TokenKind::Rpar => Some(BracketOrPunctuation::CloseBracket(')')),
            TokenKind::Rsqb => Some(BracketOrPunctuation::CloseBracket(']')),
            TokenKind::Comma => Some(BracketOrPunctuation::Punctuation(',')),
            TokenKind::Colon => Some(BracketOrPunctuation::Punctuation(':')),
            TokenKind::Semi => Some(BracketOrPunctuation::Punctuation(';')),
            _ => None,
        }
    }
}
