use crate::checkers::logical_lines::LogicalLinesContext;
use ruff_diagnostics::Violation;
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::token_kind::{
    is_arithmetic_token, is_keyword_token, is_operator_token, is_soft_keyword_token,
};
use ruff_text_size::{TextRange, TextSize};
use rustpython_parser::Tok;

use crate::rules::pycodestyle::rules::logical_lines::LogicalLine;

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
    let mut prev_type = &Tok::EndOfFile;
    let mut prev_end = TextSize::default();

    for token in line.tokens() {
        let kind = token.token();

        if token.is_skip_comment() {
            continue;
        }

        match kind {
            Tok::Lpar | Tok::Lambda => parens += 1,
            Tok::Rpar => parens -= 1,
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
            } else if token.is_greater() && matches!(prev_type, Tok::Less | Tok::Minus) {
                // Tolerate the "<>" operator, even if running Python 3
                // Deal with Python 3's annotated return value "->"
            } else if matches!(prev_type, Tok::Slash)
                && matches!(kind, Tok::Comma | Tok::Rpar | Tok::Colon)
                || (matches!(prev_type, Tok::Rpar) && token.is_colon())
            {
                // Tolerate the "/" operator in function definition
                // For more info see PEP570
            } else {
                if needs_space_main == NeedsSpace::Yes || needs_space_aux == NeedsSpace::Yes {
                    context.push(MissingWhitespaceAroundOperator, TextRange::empty(prev_end));
                } else if !matches!(prev_type, Tok::DoubleStar) {
                    if matches!(prev_type, Tok::Percent) {
                        context.push(
                            MissingWhitespaceAroundModuloOperator,
                            TextRange::empty(prev_end_aux),
                        );
                    } else if !is_arithmetic_token(prev_type) {
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
        } else if (token.is_operator() || token.is_name()) && prev_end != TextSize::default() {
            if token.is_equal() && parens > 0 {
                // Allow keyword args or defaults: foo(bar=None).
            } else if token.is_whitespace_needed() {
                needs_space_main = NeedsSpace::Yes;
                needs_space_aux = NeedsSpace::Unset;
                prev_end_aux = TextSize::new(0);
            } else if token.is_unary() {
                // Check if the operator is used as a binary operator
                // Allow unary operators: -123, -x, +1.
                // Allow argument unpacking: foo(*args, **kwargs)
                if (matches!(prev_type, Tok::Rpar | Tok::Rsqb | Tok::Rbrace))
                    || (!is_operator_token(prev_type)
                        && !is_keyword_token(prev_type)
                        && !is_soft_keyword_token(prev_type))
                {
                    needs_space_main = NeedsSpace::Unset;
                    needs_space_aux = NeedsSpace::Unset;
                    prev_end_aux = TextSize::new(0);
                }
            } else if token.is_whitespace_optional() {
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
