use crate::checkers::logical_lines::LogicalLinesContext;
use itertools::PeekingNext;
use ruff_diagnostics::{DiagnosticKind, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::token_kind::TokenKind;
use ruff_text_size::{TextRange, TextSize};

use crate::rules::pycodestyle::rules::logical_lines::{LogicalLine, LogicalLineToken};

// E225
#[violation]
pub struct MissingWhitespaceAroundOperator;

impl Violation for MissingWhitespaceAroundOperator {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Missing whitespace around operator")
    }
}

// E226
#[violation]
pub struct MissingWhitespaceAroundArithmeticOperator;

impl Violation for MissingWhitespaceAroundArithmeticOperator {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Missing whitespace around arithmetic operator")
    }
}

// E227
#[violation]
pub struct MissingWhitespaceAroundBitwiseOrShiftOperator;

impl Violation for MissingWhitespaceAroundBitwiseOrShiftOperator {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Missing whitespace around bitwise or shift operator")
    }
}

// E228
#[violation]
pub struct MissingWhitespaceAroundModuloOperator;

impl Violation for MissingWhitespaceAroundModuloOperator {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Missing whitespace around modulo operator")
    }
}

/// E225, E226, E227, E228
#[allow(clippy::if_same_then_else)]
pub(crate) fn missing_whitespace_around_operator(
    line: &LogicalLine,
    context: &mut LogicalLinesContext,
) {
    #[derive(Copy, Clone, Eq, PartialEq, Debug)]
    enum NeedsSpace {
        /// Needs a leading and trailing space
        Yes,
        /// Doesn't need a leading or trailing space
        No,
        /// Needs a trailing space if it has a leading space.
        Optional,
    }

    let mut parens = 0u32;
    let mut prev_token: Option<&LogicalLineToken> = None;
    let mut tokens = line.tokens().iter().peekable();

    while let Some(token) = tokens.next() {
        let kind = token.kind();

        if kind.is_trivia() {
            continue;
        }

        match kind {
            TokenKind::Lpar | TokenKind::Lambda => parens += 1,
            TokenKind::Rpar => parens -= 1,
            _ => {}
        };

        let needs_space = if kind == TokenKind::Equal && parens > 0 {
            // Allow keyword args or defaults: foo(bar=None).
            NeedsSpace::No
        } else if kind.is_whitespace_needed() {
            NeedsSpace::Yes
        } else if kind.is_unary() {
            prev_token.map_or(NeedsSpace::No, |prev_token| {
                let prev_kind = dbg!(prev_token.kind());

                // Check if the operator is used as a binary operator
                // Allow unary operators: -123, -x, +1.
                // Allow argument unpacking: foo(*args, **kwargs)
                if matches!(
                    prev_kind,
                    TokenKind::Rpar | TokenKind::Rsqb | TokenKind::Rbrace
                ) || !(prev_kind.is_operator()
                    || prev_kind.is_keyword()
                    || prev_kind.is_soft_keyword())
                {
                    NeedsSpace::Optional
                } else {
                    NeedsSpace::No
                }
            })
        } else if kind.is_whitespace_optional() {
            NeedsSpace::Optional
        } else {
            NeedsSpace::No
        };

        dbg!(needs_space, kind);

        match needs_space {
            NeedsSpace::Yes => {
                // Assert leading whitespace
                if prev_token.map_or(false, |prev| prev.end() == token.start()) {
                    // A needed opening space was not found
                    context.push(
                        diagnostic_kind_for_operator(kind),
                        TextRange::empty(token.start()),
                    );
                }
                // Assert trailing whitespace
                else if let Some(next_token) = tokens.peek() {
                    let next_kind = next_token.kind();

                    // Tolerate the "<>" operator, even if running Python 3
                    // Deal with Python 3's annotated return value "->"
                    let not_equal_or_arrow = next_kind == TokenKind::Greater
                        && matches!(kind, TokenKind::Less | TokenKind::Minus);

                    // Tolerate the "/" operator in function definition
                    // For more info see PEP570
                    let is_slash_in_function_definition = matches!(
                        (kind, next_kind),
                        (
                            TokenKind::Slash,
                            TokenKind::Comma | TokenKind::Rpar | TokenKind::Colon
                        ) | (TokenKind::Rpar, TokenKind::Colon)
                    );

                    let has_trailing_trivia =
                        next_token.start() > token.end() || next_kind.is_trivia();

                    if !has_trailing_trivia
                        && !not_equal_or_arrow
                        && !is_slash_in_function_definition
                    {
                        context.push(
                            diagnostic_kind_for_operator(kind),
                            TextRange::empty(token.end()),
                        );
                    }
                }
            }

            NeedsSpace::Optional => {
                // Surrounding space is optional, but ensure that
                // leading & trailing space matches opening space
                let has_leading = prev_token.map_or(false, |prev| prev.end() < token.start());
                let has_trailing = tokens.peek().map_or(false, |next| {
                    token.end() < next.start() || next.kind().is_trivia()
                });

                // TODO why does this use MissingWhitespaceAroundOperator...always
                match (has_leading, has_trailing) {
                    (true, false) => {
                        context.push(
                            MissingWhitespaceAroundOperator,
                            TextRange::empty(token.end()),
                        );
                    }
                    (false, true) => {
                        context.push(
                            MissingWhitespaceAroundOperator,
                            TextRange::empty(token.start()),
                        );
                    }
                    (false, false) | (true, true) => {}
                }
            }

            NeedsSpace::No => {}
        };

        prev_token = Some(token);
    }
}

fn diagnostic_kind_for_operator(operator: TokenKind) -> DiagnosticKind {
    if operator == TokenKind::Percent {
        DiagnosticKind::from(MissingWhitespaceAroundModuloOperator)
    } else if operator.is_bitwise_or_shift() {
        DiagnosticKind::from(MissingWhitespaceAroundBitwiseOrShiftOperator)
    } else if operator.is_arithmetic() {
        DiagnosticKind::from(MissingWhitespaceAroundArithmeticOperator)
    } else {
        DiagnosticKind::from(MissingWhitespaceAroundOperator)
    }
}
