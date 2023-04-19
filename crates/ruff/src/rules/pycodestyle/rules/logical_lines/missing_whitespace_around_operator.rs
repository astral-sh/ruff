use crate::checkers::logical_lines::LogicalLinesContext;
use ruff_diagnostics::Violation;
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::token_kind::TokenKind;
use ruff_text_size::{TextRange, TextSize};

use crate::rules::pycodestyle::rules::logical_lines::LogicalLineTokens;

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
    tokens: &LogicalLineTokens,
    context: &mut LogicalLinesContext,
) {
    #[derive(Copy, Clone, Eq, PartialEq)]
    enum NeedsSpace {
        Yes,
        No,
        Unset,
    }

    let mut needs_space_main = NeedsSpace::No;
    let mut needs_space_aux = NeedsSpace::Unset;
    let mut prev_end_aux = TextSize::default();
    let mut parens = 0u32;
    let mut prev_type: TokenKind = TokenKind::EndOfFile;
    let mut prev_end = TextSize::default();

    for token in tokens {
        let kind = token.kind();

        if kind.is_skip_comment() {
            continue;
        }

        match kind {
            TokenKind::Lpar | TokenKind::Lambda => parens += 1,
            TokenKind::Rpar => parens -= 1,
            _ => {}
        };

        let needs_space = needs_space_main == NeedsSpace::Yes
            || needs_space_aux != NeedsSpace::Unset
            || prev_end_aux != TextSize::new(0);
        if needs_space {
            if token.start() > prev_end {
                if needs_space_main != NeedsSpace::Yes && needs_space_aux != NeedsSpace::Yes {
                    context.push(
                        MissingWhitespaceAroundOperator,
                        TextRange::empty(prev_end_aux),
                    );
                }
                needs_space_main = NeedsSpace::No;
                needs_space_aux = NeedsSpace::Unset;
                prev_end_aux = TextSize::new(0);
            } else if kind == TokenKind::Greater
                && matches!(prev_type, TokenKind::Less | TokenKind::Minus)
            {
                // Tolerate the "<>" operator, even if running Python 3
                // Deal with Python 3's annotated return value "->"
            } else if prev_type == TokenKind::Slash
                && matches!(kind, TokenKind::Comma | TokenKind::Rpar | TokenKind::Colon)
                || (prev_type == TokenKind::Rpar && kind == TokenKind::Colon)
            {
                // Tolerate the "/" operator in function definition
                // For more info see PEP570
            } else {
                if needs_space_main == NeedsSpace::Yes || needs_space_aux == NeedsSpace::Yes {
                    context.push(MissingWhitespaceAroundOperator, TextRange::empty(prev_end));
                } else if prev_type != TokenKind::DoubleStar {
                    if prev_type == TokenKind::Percent {
                        context.push(
                            MissingWhitespaceAroundModuloOperator,
                            TextRange::empty(prev_end_aux),
                        );
                    } else if !prev_type.is_arithmetic() {
                        context.push(
                            MissingWhitespaceAroundBitwiseOrShiftOperator,
                            TextRange::empty(prev_end_aux),
                        );
                    } else {
                        context.push(
                            MissingWhitespaceAroundArithmeticOperator,
                            TextRange::empty(prev_end_aux),
                        );
                    }
                }
                needs_space_main = NeedsSpace::No;
                needs_space_aux = NeedsSpace::Unset;
                prev_end_aux = TextSize::new(0);
            }
        } else if (kind.is_operator() || matches!(kind, TokenKind::Name))
            && prev_end != TextSize::default()
        {
            if kind == TokenKind::Equal && parens > 0 {
                // Allow keyword args or defaults: foo(bar=None).
            } else if kind.is_whitespace_needed() {
                needs_space_main = NeedsSpace::Yes;
                needs_space_aux = NeedsSpace::Unset;
                prev_end_aux = TextSize::new(0);
            } else if kind.is_unary() {
                // Check if the operator is used as a binary operator
                // Allow unary operators: -123, -x, +1.
                // Allow argument unpacking: foo(*args, **kwargs)
                if (matches!(
                    prev_type,
                    TokenKind::Rpar | TokenKind::Rsqb | TokenKind::Rbrace
                )) || (!prev_type.is_operator()
                    && !prev_type.is_keyword()
                    && !prev_type.is_soft_keyword())
                {
                    needs_space_main = NeedsSpace::Unset;
                    needs_space_aux = NeedsSpace::Unset;
                    prev_end_aux = TextSize::new(0);
                }
            } else if kind.is_whitespace_optional() {
                needs_space_main = NeedsSpace::Unset;
                needs_space_aux = NeedsSpace::Unset;
                prev_end_aux = TextSize::new(0);
            }

            if needs_space_main == NeedsSpace::Unset {
                // Surrounding space is optional, but ensure that
                // trailing space matches opening space
                prev_end_aux = prev_end;
                needs_space_aux = if token.start() == prev_end {
                    NeedsSpace::No
                } else {
                    NeedsSpace::Yes
                };
            } else if needs_space_main == NeedsSpace::Yes && token.start() == prev_end_aux {
                // A needed opening space was not found
                context.push(MissingWhitespaceAroundOperator, TextRange::empty(prev_end));
                needs_space_main = NeedsSpace::No;
                needs_space_aux = NeedsSpace::Unset;
                prev_end_aux = TextSize::new(0);
            }
        }
        prev_type = kind;
        prev_end = token.end();
    }
}
