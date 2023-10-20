use ruff_diagnostics::AlwaysFixableViolation;
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

impl AlwaysFixableViolation for WhitespaceAfterOpenBracket {
    #[derive_message_formats]
    fn message(&self) -> String {
        let WhitespaceAfterOpenBracket { symbol } = self;
        format!("Whitespace after '{symbol}'")
    }

    fn fix_title(&self) -> String {
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

impl AlwaysFixableViolation for WhitespaceBeforeCloseBracket {
    #[derive_message_formats]
    fn message(&self) -> String {
        let WhitespaceBeforeCloseBracket { symbol } = self;
        format!("Whitespace before '{symbol}'")
    }

    fn fix_title(&self) -> String {
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

impl AlwaysFixableViolation for WhitespaceBeforePunctuation {
    #[derive_message_formats]
    fn message(&self) -> String {
        let WhitespaceBeforePunctuation { symbol } = self;
        format!("Whitespace before '{symbol}'")
    }

    fn fix_title(&self) -> String {
        let WhitespaceBeforePunctuation { symbol } = self;
        format!("Remove whitespace before '{symbol}'")
    }
}

/// E201, E202, E203
pub(crate) fn extraneous_whitespace(line: &LogicalLine, context: &mut LogicalLinesContext) {
    let mut prev_token = None;
    let mut fstrings = 0u32;

    for token in line.tokens() {
        let kind = token.kind();
        match kind {
            TokenKind::FStringStart => fstrings += 1,
            TokenKind::FStringEnd => fstrings = fstrings.saturating_sub(1),
            _ => {}
        }
        if let Some(symbol) = BracketOrPunctuation::from_kind(kind) {
            // Whitespace before "{" or after "}" might be required in f-strings.
            // For example,
            //
            // ```python
            // f"{ {'a': 1} }"
            // ```
            //
            // Here, `{{` / `}} would be interpreted as a single raw `{` / `}`
            // character.
            match symbol {
                BracketOrPunctuation::OpenBracket(symbol) if symbol != '{' || fstrings == 0 => {
                    let (trailing, trailing_len) = line.trailing_whitespace(token);
                    if !matches!(trailing, Whitespace::None) {
                        let mut diagnostic = Diagnostic::new(
                            WhitespaceAfterOpenBracket { symbol },
                            TextRange::at(token.end(), trailing_len),
                        );
                        diagnostic
                            .set_fix(Fix::safe_edit(Edit::range_deletion(diagnostic.range())));
                        context.push_diagnostic(diagnostic);
                    }
                }
                BracketOrPunctuation::CloseBracket(symbol) if symbol != '}' || fstrings == 0 => {
                    if !matches!(prev_token, Some(TokenKind::Comma)) {
                        if let (Whitespace::Single | Whitespace::Many | Whitespace::Tab, offset) =
                            line.leading_whitespace(token)
                        {
                            let mut diagnostic = Diagnostic::new(
                                WhitespaceBeforeCloseBracket { symbol },
                                TextRange::at(token.start() - offset, offset),
                            );
                            diagnostic
                                .set_fix(Fix::safe_edit(Edit::range_deletion(diagnostic.range())));
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
                            diagnostic
                                .set_fix(Fix::safe_edit(Edit::range_deletion(diagnostic.range())));
                            context.push_diagnostic(diagnostic);
                        }
                    }
                }
                _ => {}
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
