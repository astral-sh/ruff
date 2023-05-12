use crate::checkers::logical_lines::LogicalLinesContext;
use crate::rules::pycodestyle::rules::logical_lines::{LogicalLine, LogicalLineToken};
use ruff_diagnostics::{DiagnosticKind, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::token_kind::TokenKind;

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
            TokenKind::Rpar => parens = parens.saturating_sub(1),
            _ => {}
        };

        let needs_space = if kind == TokenKind::Equal && parens > 0 {
            // Allow keyword args or defaults: foo(bar=None).
            NeedsSpace::No
        } else if kind == TokenKind::Slash {
            // Tolerate the "/" operator in function definition
            // For more info see PEP570

            // `def f(a, /, b):` or `def f(a, b, /):` or `f = lambda a, /:`
            //            ^                       ^                      ^
            let slash_in_func = matches!(
                tokens.peek().map(|t| t.kind()),
                Some(TokenKind::Comma | TokenKind::Rpar | TokenKind::Colon)
            );

            NeedsSpace::from(!slash_in_func)
        } else if kind.is_unary() || kind == TokenKind::DoubleStar {
            let is_binary = prev_token.map_or(false, |prev_token| {
                let prev_kind = prev_token.kind();

                // Check if the operator is used as a binary operator.
                // Allow unary operators: -123, -x, +1.
                // Allow argument unpacking: foo(*args, **kwargs)
                matches!(
                    prev_kind,
                    TokenKind::Rpar | TokenKind::Rsqb | TokenKind::Rbrace
                ) || !(prev_kind.is_operator()
                    || prev_kind.is_keyword()
                    || prev_kind.is_soft_keyword())
            });

            if is_binary {
                if kind == TokenKind::DoubleStar {
                    // Enforce consistent spacing, but don't enforce whitespaces.
                    NeedsSpace::Optional
                } else {
                    NeedsSpace::Yes
                }
            } else {
                NeedsSpace::No
            }
        } else if is_whitespace_needed(kind) {
            NeedsSpace::Yes
        } else {
            NeedsSpace::No
        };

        if needs_space != NeedsSpace::No {
            let has_leading_trivia = prev_token.map_or(true, |prev| {
                prev.end() < token.start() || prev.kind().is_trivia()
            });

            let has_trailing_trivia = tokens.peek().map_or(true, |next| {
                token.end() < next.start() || next.kind().is_trivia()
            });

            match (has_leading_trivia, has_trailing_trivia) {
                // Operator with trailing but no leading space, enforce consistent spacing
                (false, true) |
                // Operator with leading but no trailing space, enforce consistent spacing.
                (true, false)
                => {
                    context.push(MissingWhitespaceAroundOperator, token.range());
                }
                // Operator with no space, require spaces if it is required by the operator.
                (false, false) => {
                    if needs_space == NeedsSpace::Yes {
                        context.push(diagnostic_kind_for_operator(kind), token.range());
                    }
                }
                (true, true) => {
                    // Operator has leading and trailing spaces, all good
                }
            }
        }

        prev_token = Some(token);
    }
}

#[derive(Copy, Clone, Eq, PartialEq, Debug)]
enum NeedsSpace {
    /// Needs a leading and trailing space.
    Yes,

    /// Doesn't need a leading or trailing space. Or in other words, we don't care how many
    /// leading or trailing spaces that token has.
    No,

    /// Needs consistent leading and trailing spacing. The operator needs spacing if
    /// * it has a leading space
    /// * it has a trailing space
    Optional,
}

impl From<bool> for NeedsSpace {
    fn from(value: bool) -> Self {
        if value {
            NeedsSpace::Yes
        } else {
            NeedsSpace::No
        }
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

fn is_whitespace_needed(kind: TokenKind) -> bool {
    matches!(
        kind,
        TokenKind::DoubleStarEqual
            | TokenKind::StarEqual
            | TokenKind::SlashEqual
            | TokenKind::DoubleSlashEqual
            | TokenKind::PlusEqual
            | TokenKind::MinusEqual
            | TokenKind::NotEqual
            | TokenKind::Less
            | TokenKind::Greater
            | TokenKind::PercentEqual
            | TokenKind::CircumflexEqual
            | TokenKind::AmperEqual
            | TokenKind::VbarEqual
            | TokenKind::EqEqual
            | TokenKind::LessEqual
            | TokenKind::GreaterEqual
            | TokenKind::LeftShiftEqual
            | TokenKind::RightShiftEqual
            | TokenKind::Equal
            | TokenKind::And
            | TokenKind::Or
            | TokenKind::In
            | TokenKind::Is
            | TokenKind::Rarrow
            | TokenKind::ColonEqual
            | TokenKind::Slash
            | TokenKind::Percent
    ) || kind.is_arithmetic()
        || kind.is_bitwise_or_shift()
}
