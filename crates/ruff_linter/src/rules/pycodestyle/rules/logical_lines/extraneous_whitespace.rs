use ruff_diagnostics::AlwaysAutofixableViolation;
use ruff_diagnostics::Diagnostic;
use ruff_diagnostics::Edit;
use ruff_diagnostics::Fix;
use ruff_macros::{derive_message_formats, violation};
use ruff_python_parser::TokenKind;
use ruff_text_size::{Ranged, TextRange};

use crate::checkers::logical_lines::LogicalLinesContext;

use super::{LogicalLine, Whitespace};

/// ## What it does
/// Checks for the use of extraneous whitespace after "(".
///
/// ## Why is this bad?
/// [PEP 8] recommends the omission of whitespace in the following cases:
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
/// [PEP 8]: https://peps.python.org/pep-0008/#pet-peeves
#[violation]
pub struct WhitespaceAfterOpenBracket {
    symbol: char,
}

impl AlwaysAutofixableViolation for WhitespaceAfterOpenBracket {
    #[derive_message_formats]
    fn message(&self) -> String {
        let WhitespaceAfterOpenBracket { symbol } = self;
        format!("Whitespace after '{symbol}'")
    }

    fn autofix_title(&self) -> String {
        let WhitespaceAfterOpenBracket { symbol } = self;
        format!("Remove whitespace before '{symbol}'")
    }
}

/// ## What it does
/// Checks for the use of extraneous whitespace before ")".
///
/// ## Why is this bad?
/// [PEP 8] recommends the omission of whitespace in the following cases:
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
/// [PEP 8]: https://peps.python.org/pep-0008/#pet-peeves
#[violation]
pub struct WhitespaceBeforeCloseBracket {
    symbol: char,
}

impl AlwaysAutofixableViolation for WhitespaceBeforeCloseBracket {
    #[derive_message_formats]
    fn message(&self) -> String {
        let WhitespaceBeforeCloseBracket { symbol } = self;
        format!("Whitespace before '{symbol}'")
    }

    fn autofix_title(&self) -> String {
        let WhitespaceBeforeCloseBracket { symbol } = self;
        format!("Remove whitespace before '{symbol}'")
    }
}

/// ## What it does
/// Checks for the use of extraneous whitespace before ",", ";" or ":".
///
/// ## Why is this bad?
/// [PEP 8] recommends the omission of whitespace in the following cases:
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
/// [PEP 8]: https://peps.python.org/pep-0008/#pet-peeves
#[violation]
pub struct WhitespaceBeforePunctuation {
    symbol: char,
}

impl AlwaysAutofixableViolation for WhitespaceBeforePunctuation {
    #[derive_message_formats]
    fn message(&self) -> String {
        let WhitespaceBeforePunctuation { symbol } = self;
        format!("Whitespace before '{symbol}'")
    }

    fn autofix_title(&self) -> String {
        let WhitespaceBeforePunctuation { symbol } = self;
        format!("Remove whitespace before '{symbol}'")
    }
}

/// E201, E202, E203
pub(crate) fn extraneous_whitespace(
    line: &LogicalLine,
    context: &mut LogicalLinesContext,
    autofix_after_open_bracket: bool,
    autofix_before_close_bracket: bool,
    autofix_before_punctuation: bool,
) {
    let mut prev_token = None;

    for token in line.tokens() {
        let kind = token.kind();
        if let Some(symbol) = BracketOrPunctuation::from_kind(kind) {
            match symbol {
                BracketOrPunctuation::OpenBracket(symbol) => {
                    let (trailing, trailing_len) = line.trailing_whitespace(token);
                    if !matches!(trailing, Whitespace::None) {
                        let mut diagnostic = Diagnostic::new(
                            WhitespaceAfterOpenBracket { symbol },
                            TextRange::at(token.end(), trailing_len),
                        );
                        if autofix_after_open_bracket {
                            diagnostic
                                .set_fix(Fix::automatic(Edit::range_deletion(diagnostic.range())));
                        }
                        context.push_diagnostic(diagnostic);
                    }
                }
                BracketOrPunctuation::CloseBracket(symbol) => {
                    if !matches!(prev_token, Some(TokenKind::Comma)) {
                        if let (Whitespace::Single | Whitespace::Many | Whitespace::Tab, offset) =
                            line.leading_whitespace(token)
                        {
                            let mut diagnostic = Diagnostic::new(
                                WhitespaceBeforeCloseBracket { symbol },
                                TextRange::at(token.start() - offset, offset),
                            );
                            if autofix_before_close_bracket {
                                diagnostic.set_fix(Fix::automatic(Edit::range_deletion(
                                    diagnostic.range(),
                                )));
                            }
                            context.push_diagnostic(diagnostic);
                        }
                    }
                }
                BracketOrPunctuation::Punctuation(symbol) => {
                    if !matches!(prev_token, Some(TokenKind::Comma)) {
                        if let (Whitespace::Single | Whitespace::Many | Whitespace::Tab, offset) =
                            line.leading_whitespace(token)
                        {
                            let mut diagnostic = Diagnostic::new(
                                WhitespaceBeforePunctuation { symbol },
                                TextRange::at(token.start() - offset, offset),
                            );
                            if autofix_before_punctuation {
                                diagnostic.set_fix(Fix::automatic(Edit::range_deletion(
                                    diagnostic.range(),
                                )));
                            }
                            context.push_diagnostic(diagnostic);
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
