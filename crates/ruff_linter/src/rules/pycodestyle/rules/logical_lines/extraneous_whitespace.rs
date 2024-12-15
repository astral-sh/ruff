use ruff_diagnostics::AlwaysFixableViolation;
use ruff_diagnostics::Diagnostic;
use ruff_diagnostics::Edit;
use ruff_diagnostics::Fix;
use ruff_macros::{derive_message_formats, ViolationMetadata};
use ruff_python_parser::TokenKind;
use ruff_text_size::{Ranged, TextRange};

use crate::checkers::logical_lines::LogicalLinesContext;

use super::{LogicalLine, Whitespace};

/// ## What it does
/// Checks for the use of extraneous whitespace after "(", "[" or "{".
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
#[derive(ViolationMetadata)]
pub(crate) struct WhitespaceAfterOpenBracket {
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
/// Checks for the use of extraneous whitespace before ")", "]" or "}".
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
#[derive(ViolationMetadata)]
pub(crate) struct WhitespaceBeforeCloseBracket {
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
#[derive(ViolationMetadata)]
pub(crate) struct WhitespaceBeforePunctuation {
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
    let mut fstrings = 0u32;
    let mut brackets = vec![];
    let mut prev_token = None;
    let mut iter = line.tokens().iter().peekable();

    while let Some(token) = iter.next() {
        let kind = token.kind();
        match kind {
            TokenKind::FStringStart => fstrings += 1,
            TokenKind::FStringEnd => fstrings = fstrings.saturating_sub(1),
            TokenKind::Lsqb => {
                brackets.push(kind);
            }
            TokenKind::Rsqb => {
                brackets.pop();
            }
            TokenKind::Lbrace => {
                brackets.push(kind);
            }
            TokenKind::Rbrace => {
                brackets.pop();
            }
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
                        let whitespace = line.leading_whitespace(token);
                        if let (Whitespace::Single | Whitespace::Many | Whitespace::Tab, offset) =
                            whitespace
                        {
                            // If we're in a slice, and the token is a colon, and it has
                            // equivalent spacing on both sides, allow it.
                            if symbol == ':'
                                && brackets
                                    .last()
                                    .is_some_and(|kind| matches!(kind, TokenKind::Lsqb))
                            {
                                // If we're in the second half of a double colon, disallow
                                // any whitespace (e.g., `foo[1: :2]` or `foo[1 : : 2]`).
                                if matches!(prev_token, Some(TokenKind::Colon)) {
                                    let mut diagnostic = Diagnostic::new(
                                        WhitespaceBeforePunctuation { symbol },
                                        TextRange::at(token.start() - offset, offset),
                                    );
                                    diagnostic.set_fix(Fix::safe_edit(Edit::range_deletion(
                                        diagnostic.range(),
                                    )));
                                    context.push_diagnostic(diagnostic);
                                } else if iter.peek().is_some_and(|token| {
                                    matches!(token.kind(), TokenKind::Rsqb | TokenKind::Comma)
                                }) {
                                    // Allow `foo[1 :]`, but not `foo[1  :]`.
                                    // Or `foo[index :, 2]`, but not `foo[index  :, 2]`.
                                    if let (Whitespace::Many | Whitespace::Tab, offset) = whitespace
                                    {
                                        let mut diagnostic = Diagnostic::new(
                                            WhitespaceBeforePunctuation { symbol },
                                            TextRange::at(token.start() - offset, offset),
                                        );
                                        diagnostic.set_fix(Fix::safe_edit(Edit::range_deletion(
                                            diagnostic.range(),
                                        )));
                                        context.push_diagnostic(diagnostic);
                                    }
                                } else if iter.peek().is_some_and(|token| {
                                    matches!(
                                        token.kind(),
                                        TokenKind::NonLogicalNewline | TokenKind::Comment
                                    )
                                }) {
                                    // Allow [
                                    //      long_expression_calculating_the_index() :
                                    // ]
                                    // But not [
                                    //      long_expression_calculating_the_index()  :
                                    // ]
                                    // distinct from the above case, because ruff format produces a
                                    // whitespace before the colon and so should the fix
                                    if let (Whitespace::Many | Whitespace::Tab, offset) = whitespace
                                    {
                                        let mut diagnostic = Diagnostic::new(
                                            WhitespaceBeforePunctuation { symbol },
                                            TextRange::at(token.start() - offset, offset),
                                        );
                                        diagnostic.set_fix(Fix::safe_edits(
                                            Edit::range_deletion(diagnostic.range()),
                                            [Edit::insertion(" ".into(), token.start() - offset)],
                                        ));
                                        context.push_diagnostic(diagnostic);
                                    }
                                } else {
                                    // Allow, e.g., `foo[1:2]` or `foo[1 : 2]` or `foo[1 :: 2]`.
                                    let token = iter
                                        .peek()
                                        .filter(|next| matches!(next.kind(), TokenKind::Colon))
                                        .unwrap_or(&token);
                                    if line.trailing_whitespace(token) != whitespace {
                                        let mut diagnostic = Diagnostic::new(
                                            WhitespaceBeforePunctuation { symbol },
                                            TextRange::at(token.start() - offset, offset),
                                        );
                                        diagnostic.set_fix(Fix::safe_edit(Edit::range_deletion(
                                            diagnostic.range(),
                                        )));
                                        context.push_diagnostic(diagnostic);
                                    }
                                }
                            } else {
                                if fstrings > 0
                                    && symbol == ':'
                                    && matches!(prev_token, Some(TokenKind::Equal))
                                {
                                    // Avoid removing any whitespace for f-string debug expressions.
                                    continue;
                                }
                                let mut diagnostic = Diagnostic::new(
                                    WhitespaceBeforePunctuation { symbol },
                                    TextRange::at(token.start() - offset, offset),
                                );
                                diagnostic.set_fix(Fix::safe_edit(Edit::range_deletion(
                                    diagnostic.range(),
                                )));
                                context.push_diagnostic(diagnostic);
                            }
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
