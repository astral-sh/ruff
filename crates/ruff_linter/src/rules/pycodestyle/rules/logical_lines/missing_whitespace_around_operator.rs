use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_python_ast::token::TokenKind;
use ruff_text_size::{Ranged, TextRange};

use crate::checkers::ast::LintContext;
use crate::rules::pycodestyle::helpers::is_non_logical_token;
use crate::rules::pycodestyle::rules::logical_lines::{DefinitionState, LogicalLine};
use crate::{AlwaysFixableViolation, Edit, Fix};

/// ## What it does
/// Checks for missing whitespace around all operators.
///
/// ## Why is this bad?
/// According to [PEP 8], there should be one space before and after all
/// assignment (`=`), augmented assignment (`+=`, `-=`, etc.), comparison,
/// and Booleans operators.
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
#[derive(ViolationMetadata)]
#[violation_metadata(preview_since = "v0.0.269")]
pub(crate) struct MissingWhitespaceAroundOperator;

impl AlwaysFixableViolation for MissingWhitespaceAroundOperator {
    #[derive_message_formats]
    fn message(&self) -> String {
        "Missing whitespace around operator".to_string()
    }

    fn fix_title(&self) -> String {
        "Add missing whitespace".to_string()
    }
}

/// ## What it does
/// Checks for missing whitespace arithmetic operators.
///
/// ## Why is this bad?
/// [PEP 8] recommends never using more than one space, and always having the
/// same amount of whitespace on both sides of a binary operator.
///
/// For consistency, this rule enforces one space before and after an
/// arithmetic operator (`+`, `-`, `/`, and `*`).
///
/// (Note that [PEP 8] suggests only adding whitespace around the operator with
/// the lowest precedence, but that authors should "use [their] own judgment".)
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
/// [PEP 8]: https://peps.python.org/pep-0008/#other-recommendations
// E226
#[derive(ViolationMetadata)]
#[violation_metadata(preview_since = "v0.0.269")]
pub(crate) struct MissingWhitespaceAroundArithmeticOperator;

impl AlwaysFixableViolation for MissingWhitespaceAroundArithmeticOperator {
    #[derive_message_formats]
    fn message(&self) -> String {
        "Missing whitespace around arithmetic operator".to_string()
    }

    fn fix_title(&self) -> String {
        "Add missing whitespace".to_string()
    }
}

/// ## What it does
/// Checks for missing whitespace around bitwise and shift operators.
///
/// ## Why is this bad?
/// [PEP 8] recommends never using more than one space, and always having the
/// same amount of whitespace on both sides of a binary operator.
///
/// For consistency, this rule enforces one space before and after bitwise and
/// shift operators (`<<`, `>>`, `&`, `|`, `^`).
///
/// (Note that [PEP 8] suggests only adding whitespace around the operator with
/// the lowest precedence, but that authors should "use [their] own judgment".)
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
/// [PEP 8]: https://peps.python.org/pep-0008/#other-recommendations
// E227
#[derive(ViolationMetadata)]
#[violation_metadata(preview_since = "v0.0.269")]
pub(crate) struct MissingWhitespaceAroundBitwiseOrShiftOperator;

impl AlwaysFixableViolation for MissingWhitespaceAroundBitwiseOrShiftOperator {
    #[derive_message_formats]
    fn message(&self) -> String {
        "Missing whitespace around bitwise or shift operator".to_string()
    }

    fn fix_title(&self) -> String {
        "Add missing whitespace".to_string()
    }
}

/// ## What it does
/// Checks for missing whitespace around the modulo operator.
///
/// ## Why is this bad?
/// [PEP 8] recommends never using more than one space, and always having the
/// same amount of whitespace on both sides of a binary operator.
///
/// For consistency, this rule enforces one space before and after a modulo
/// operator (`%`).
///
/// (Note that [PEP 8] suggests only adding whitespace around the operator with
/// the lowest precedence, but that authors should "use [their] own judgment".)
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
#[derive(ViolationMetadata)]
#[violation_metadata(preview_since = "v0.0.269")]
pub(crate) struct MissingWhitespaceAroundModuloOperator;

impl AlwaysFixableViolation for MissingWhitespaceAroundModuloOperator {
    #[derive_message_formats]
    fn message(&self) -> String {
        "Missing whitespace around modulo operator".to_string()
    }

    fn fix_title(&self) -> String {
        "Add missing whitespace".to_string()
    }
}

/// E225, E226, E227, E228
pub(crate) fn missing_whitespace_around_operator(line: &LogicalLine, context: &LintContext) {
    let mut definition_state = DefinitionState::from_tokens(line.tokens());
    let mut tokens = line.tokens().iter().peekable();
    let first_token = tokens
        .by_ref()
        .find(|token| !is_non_logical_token(token.kind()));
    let Some(mut prev_token) = first_token else {
        return;
    };
    let mut parens = u32::from(matches!(
        prev_token.kind(),
        TokenKind::Lpar | TokenKind::Lambda
    ));
    let mut fstrings = u32::from(matches!(prev_token.kind(), TokenKind::FStringStart));

    while let Some(token) = tokens.next() {
        let kind = token.kind();

        definition_state.visit_token_kind(kind);

        if is_non_logical_token(kind) {
            continue;
        }

        match kind {
            TokenKind::FStringStart => fstrings += 1,
            TokenKind::FStringEnd => fstrings = fstrings.saturating_sub(1),
            TokenKind::Lpar | TokenKind::Lambda => parens += 1,
            TokenKind::Rpar => parens = parens.saturating_sub(1),
            _ => {}
        }

        let needs_space = if kind == TokenKind::Equal
            && (parens > 0 || fstrings > 0 || definition_state.in_type_params())
        {
            // Allow keyword args, defaults: foo(bar=None) and f-strings: f'{foo=}'
            // Also ignore `foo[T=int]`, which is handled by E251.
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
        } else if kind.is_unary_arithmetic_operator()
            || matches!(kind, TokenKind::Star | TokenKind::DoubleStar)
        {
            let is_binary = {
                let prev_kind = prev_token.kind();

                // Check if the operator is used as a binary operator.
                // Allow unary operators: -123, -x, +1.
                // Allow argument unpacking: foo(*args, **kwargs)
                matches!(
                    prev_kind,
                    TokenKind::Rpar | TokenKind::Rsqb | TokenKind::Rbrace
                ) || !(prev_kind.is_operator() || prev_kind.is_keyword())
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
        } else if tokens.peek().is_some_and(|token| {
            matches!(
                token.kind(),
                TokenKind::Rpar | TokenKind::Rsqb | TokenKind::Rbrace
            )
        }) {
            // There should not be a closing bracket directly after a token, as it is a syntax
            // error. For example:
            // ```
            // 1+)
            // ```
            //
            // However, allow it in order to prevent entering an infinite loop in which E225 adds a
            // space only for E202 to remove it.
            NeedsSpace::No
        } else if is_whitespace_needed(kind) {
            NeedsSpace::Yes
        } else {
            NeedsSpace::No
        };

        if needs_space != NeedsSpace::No {
            let has_leading_trivia =
                prev_token.end() < token.start() || is_non_logical_token(prev_token.kind());

            let has_trailing_trivia = tokens
                .peek()
                .is_none_or(|next| token.end() < next.start() || is_non_logical_token(next.kind()));

            match (has_leading_trivia, has_trailing_trivia) {
                // Operator with trailing but no leading space, enforce consistent spacing.
                (false, true) => {
                    if let Some(mut diagnostic) =
                        diagnostic_kind_for_operator(kind, token.range(), context)
                    {
                        diagnostic.set_fix(Fix::safe_edit(Edit::insertion(
                            " ".to_string(),
                            token.start(),
                        )));
                    }
                }
                // Operator with leading but no trailing space, enforce consistent spacing.
                (true, false) => {
                    if let Some(mut diagnostic) =
                        diagnostic_kind_for_operator(kind, token.range(), context)
                    {
                        diagnostic.set_fix(Fix::safe_edit(Edit::insertion(
                            " ".to_string(),
                            token.end(),
                        )));
                    }
                }
                // Operator with no space, require spaces if it is required by the operator.
                (false, false) => {
                    if needs_space == NeedsSpace::Yes {
                        if let Some(mut diagnostic) =
                            diagnostic_kind_for_operator(kind, token.range(), context)
                        {
                            diagnostic.set_fix(Fix::safe_edits(
                                Edit::insertion(" ".to_string(), token.start()),
                                [Edit::insertion(" ".to_string(), token.end())],
                            ));
                        }
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

fn diagnostic_kind_for_operator<'a>(
    operator: TokenKind,
    range: TextRange,
    context: &'a LintContext<'a>,
) -> Option<crate::checkers::ast::DiagnosticGuard<'a, 'a>> {
    if operator == TokenKind::Percent {
        context.report_diagnostic_if_enabled(MissingWhitespaceAroundModuloOperator, range)
    } else if operator.is_bitwise_or_shift() {
        context.report_diagnostic_if_enabled(MissingWhitespaceAroundBitwiseOrShiftOperator, range)
    } else if operator.is_arithmetic() {
        context.report_diagnostic_if_enabled(MissingWhitespaceAroundArithmeticOperator, range)
    } else {
        context.report_diagnostic_if_enabled(MissingWhitespaceAroundOperator, range)
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
