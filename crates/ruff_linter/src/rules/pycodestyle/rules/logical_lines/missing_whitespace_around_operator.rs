use ruff_diagnostics::{DiagnosticKind, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_parser::TokenKind;
use ruff_text_size::Ranged;

use crate::checkers::logical_lines::LogicalLinesContext;
use crate::rules::pycodestyle::rules::logical_lines::LogicalLine;

/// ## What it does
/// Checks for missing whitespace around all operators.
///
/// ## Why is this bad?
/// According to [PEP 8], there should be one space before and after all
/// operators.
///
/// ## Example
/// ```python
/// if number==42:
///     print('you have found the meaning of life')
/// ```
///
/// Use instead:
/// ```python
/// if number == 42:
///     print('you have found the meaning of life')
/// ```
///
/// [PEP 8]: https://peps.python.org/pep-0008/#pet-peeves
// E225
#[violation]
pub struct MissingWhitespaceAroundOperator;

impl Violation for MissingWhitespaceAroundOperator {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Missing whitespace around operator")
    }
}

/// ## What it does
/// Checks for missing whitespace arithmetic operators.
///
/// ## Why is this bad?
/// According to [PEP 8], there should be one space before and after an
/// arithmetic operator (+, -, /, and *).
///
/// ## Example
/// ```python
/// number = 40+2
/// ```
///
/// Use instead:
/// ```python
/// number = 40 + 2
/// ```
///
/// [PEP 8]: https://peps.python.org/pep-0008/#pet-peeves
// E226
#[violation]
pub struct MissingWhitespaceAroundArithmeticOperator;

impl Violation for MissingWhitespaceAroundArithmeticOperator {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Missing whitespace around arithmetic operator")
    }
}

/// ## What it does
/// Checks for missing whitespace around bitwise and shift operators.
///
/// ## Why is this bad?
/// According to [PEP 8], there should be one space before and after bitwise and
/// shift operators (<<, >>, &, |, ^).
///
/// ## Example
/// ```python
/// x = 128<<1
/// ```
///
/// Use instead:
/// ```python
/// x = 128 << 1
/// ```
///
/// [PEP 8]: https://peps.python.org/pep-0008/#pet-peeves
// E227
#[violation]
pub struct MissingWhitespaceAroundBitwiseOrShiftOperator;

impl Violation for MissingWhitespaceAroundBitwiseOrShiftOperator {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Missing whitespace around bitwise or shift operator")
    }
}

/// ## What it does
/// Checks for missing whitespace around the modulo operator.
///
/// ## Why is this bad?
/// According to [PEP 8], the modulo operator (%) should have whitespace on
/// either side of it.
///
/// ## Example
/// ```python
/// remainder = 10%2
/// ```
///
/// Use instead:
/// ```python
/// remainder = 10 % 2
/// ```
///
/// [PEP 8]: https://peps.python.org/pep-0008/#other-recommendations
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
pub(crate) fn missing_whitespace_around_operator(
    line: &LogicalLine,
    context: &mut LogicalLinesContext,
) {
    let mut tokens = line.tokens().iter().peekable();
    let first_token = tokens.by_ref().find_map(|token| {
        let kind = token.kind();
        (!kind.is_trivia()).then_some(token)
    });
    let Some(mut prev_token) = first_token else {
        return;
    };
    let mut parens = u32::from(matches!(
        prev_token.kind(),
        TokenKind::Lpar | TokenKind::Lambda
    ));

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
        } else if kind.is_unary() || matches!(kind, TokenKind::Star | TokenKind::DoubleStar) {
            let is_binary = {
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
            };

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
            let has_leading_trivia =
                prev_token.end() < token.start() || prev_token.kind().is_trivia();

            let has_trailing_trivia = tokens.peek().map_or(true, |next| {
                token.end() < next.start() || next.kind().is_trivia()
            });

            match (has_leading_trivia, has_trailing_trivia) {
                // Operator with trailing but no leading space, enforce consistent spacing.
                (false, true) |
                // Operator with leading but no trailing space, enforce consistent spacing.
                (true, false) => {
                    context.push(MissingWhitespaceAroundOperator, token.range());
                }
                // Operator with no space, require spaces if it is required by the operator.
                (false, false) => {
                    if needs_space == NeedsSpace::Yes {
                        context.push(diagnostic_kind_for_operator(kind), token.range());
                    }
                }
                (true, true) => {
                    // Operator has leading and trailing spaces, all good.
                }
            }
        }

        prev_token = token;
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
        || (kind.is_bitwise_or_shift() &&
            // As a special-case, pycodestyle seems to ignore whitespace around the tilde.
            !matches!(kind, TokenKind::Tilde))
}
