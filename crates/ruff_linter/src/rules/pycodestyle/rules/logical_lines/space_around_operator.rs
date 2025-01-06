use ruff_diagnostics::{AlwaysFixableViolation, Diagnostic, Edit, Fix};
use ruff_macros::{derive_message_formats, ViolationMetadata};
use ruff_python_parser::TokenKind;
use ruff_text_size::{Ranged, TextRange};

use crate::checkers::logical_lines::LogicalLinesContext;

use super::{LogicalLine, Whitespace};

/// ## What it does
/// Checks for extraneous tabs before an operator.
///
/// ## Why is this bad?
/// According to [PEP 8], operators should be surrounded by at most a single space on either
/// side.
///
/// ## Example
/// ```python
/// a = 4\t+ 5
/// ```
///
/// Use instead:
/// ```python
/// a = 4 + 5
/// ```
///
/// [PEP 8]: https://peps.python.org/pep-0008/#whitespace-in-expressions-and-statements
#[derive(ViolationMetadata)]
pub(crate) struct TabBeforeOperator;

impl AlwaysFixableViolation for TabBeforeOperator {
    #[derive_message_formats]
    fn message(&self) -> String {
        "Tab before operator".to_string()
    }

    fn fix_title(&self) -> String {
        "Replace with single space".to_string()
    }
}

/// ## What it does
/// Checks for extraneous whitespace before an operator.
///
/// ## Why is this bad?
/// According to [PEP 8], operators should be surrounded by at most a single space on either
/// side.
///
/// ## Example
/// ```python
/// a = 4  + 5
/// ```
///
/// Use instead:
/// ```python
/// a = 4 + 5
/// ```
///
/// [PEP 8]: https://peps.python.org/pep-0008/#whitespace-in-expressions-and-statements
#[derive(ViolationMetadata)]
pub(crate) struct MultipleSpacesBeforeOperator;

impl AlwaysFixableViolation for MultipleSpacesBeforeOperator {
    #[derive_message_formats]
    fn message(&self) -> String {
        "Multiple spaces before operator".to_string()
    }

    fn fix_title(&self) -> String {
        "Replace with single space".to_string()
    }
}

/// ## What it does
/// Checks for extraneous tabs after an operator.
///
/// ## Why is this bad?
/// According to [PEP 8], operators should be surrounded by at most a single space on either
/// side.
///
/// ## Example
/// ```python
/// a = 4 +\t5
/// ```
///
/// Use instead:
/// ```python
/// a = 4 + 5
/// ```
///
/// [PEP 8]: https://peps.python.org/pep-0008/#whitespace-in-expressions-and-statements
#[derive(ViolationMetadata)]
pub(crate) struct TabAfterOperator;

impl AlwaysFixableViolation for TabAfterOperator {
    #[derive_message_formats]
    fn message(&self) -> String {
        "Tab after operator".to_string()
    }

    fn fix_title(&self) -> String {
        "Replace with single space".to_string()
    }
}

/// ## What it does
/// Checks for extraneous whitespace after an operator.
///
/// ## Why is this bad?
/// According to [PEP 8], operators should be surrounded by at most a single space on either
/// side.
///
/// ## Example
/// ```python
/// a = 4 +  5
/// ```
///
/// Use instead:
/// ```python
/// a = 4 + 5
/// ```
///
/// [PEP 8]: https://peps.python.org/pep-0008/#whitespace-in-expressions-and-statements
#[derive(ViolationMetadata)]
pub(crate) struct MultipleSpacesAfterOperator;

impl AlwaysFixableViolation for MultipleSpacesAfterOperator {
    #[derive_message_formats]
    fn message(&self) -> String {
        "Multiple spaces after operator".to_string()
    }

    fn fix_title(&self) -> String {
        "Replace with single space".to_string()
    }
}

/// ## What it does
/// Checks for extraneous tabs after a comma.
///
/// ## Why is this bad?
/// Commas should be followed by one space, never tabs.
///
/// ## Example
/// ```python
/// a = 4,\t5
/// ```
///
/// Use instead:
/// ```python
/// a = 4, 5
/// ```
///
#[derive(ViolationMetadata)]
pub(crate) struct TabAfterComma;

impl AlwaysFixableViolation for TabAfterComma {
    #[derive_message_formats]
    fn message(&self) -> String {
        "Tab after comma".to_string()
    }

    fn fix_title(&self) -> String {
        "Replace with single space".to_string()
    }
}

/// ## What it does
/// Checks for extraneous whitespace after a comma.
///
/// ## Why is this bad?
/// Consistency is good. This rule helps ensure you have a consistent
/// formatting style across your project.
///
/// ## Example
/// ```python
/// a = 4,    5
/// ```
///
/// Use instead:
/// ```python
/// a = 4, 5
/// ```
#[derive(ViolationMetadata)]
pub(crate) struct MultipleSpacesAfterComma;

impl AlwaysFixableViolation for MultipleSpacesAfterComma {
    #[derive_message_formats]
    fn message(&self) -> String {
        "Multiple spaces after comma".to_string()
    }

    fn fix_title(&self) -> String {
        "Replace with single space".to_string()
    }
}

/// E221, E222, E223, E224
pub(crate) fn space_around_operator(line: &LogicalLine, context: &mut LogicalLinesContext) {
    let mut after_operator = false;

    for token in line.tokens() {
        let is_operator = is_operator_token(token.kind());

        if is_operator {
            if !after_operator {
                match line.leading_whitespace(token) {
                    (Whitespace::Tab, offset) => {
                        let mut diagnostic = Diagnostic::new(
                            TabBeforeOperator,
                            TextRange::at(token.start() - offset, offset),
                        );
                        diagnostic.set_fix(Fix::safe_edit(Edit::range_replacement(
                            " ".to_string(),
                            TextRange::at(token.start() - offset, offset),
                        )));
                        context.push_diagnostic(diagnostic);
                    }
                    (Whitespace::Many, offset) => {
                        let mut diagnostic = Diagnostic::new(
                            MultipleSpacesBeforeOperator,
                            TextRange::at(token.start() - offset, offset),
                        );
                        diagnostic.set_fix(Fix::safe_edit(Edit::range_replacement(
                            " ".to_string(),
                            TextRange::at(token.start() - offset, offset),
                        )));
                        context.push_diagnostic(diagnostic);
                    }
                    _ => {}
                }
            }

            match line.trailing_whitespace(token) {
                (Whitespace::Tab, len) => {
                    let mut diagnostic =
                        Diagnostic::new(TabAfterOperator, TextRange::at(token.end(), len));
                    diagnostic.set_fix(Fix::safe_edit(Edit::range_replacement(
                        " ".to_string(),
                        TextRange::at(token.end(), len),
                    )));
                    context.push_diagnostic(diagnostic);
                }
                (Whitespace::Many, len) => {
                    let mut diagnostic = Diagnostic::new(
                        MultipleSpacesAfterOperator,
                        TextRange::at(token.end(), len),
                    );
                    diagnostic.set_fix(Fix::safe_edit(Edit::range_replacement(
                        " ".to_string(),
                        TextRange::at(token.end(), len),
                    )));
                    context.push_diagnostic(diagnostic);
                }
                _ => {}
            }
        }

        after_operator = is_operator;
    }
}

/// E241, E242
pub(crate) fn space_after_comma(line: &LogicalLine, context: &mut LogicalLinesContext) {
    for token in line.tokens() {
        if matches!(token.kind(), TokenKind::Comma) {
            match line.trailing_whitespace(token) {
                (Whitespace::Tab, len) => {
                    let mut diagnostic =
                        Diagnostic::new(TabAfterComma, TextRange::at(token.end(), len));
                    diagnostic.set_fix(Fix::safe_edit(Edit::range_replacement(
                        " ".to_string(),
                        TextRange::at(token.end(), len),
                    )));
                    context.push_diagnostic(diagnostic);
                }
                (Whitespace::Many, len) => {
                    let mut diagnostic =
                        Diagnostic::new(MultipleSpacesAfterComma, TextRange::at(token.end(), len));
                    diagnostic.set_fix(Fix::safe_edit(Edit::range_replacement(
                        " ".to_string(),
                        TextRange::at(token.end(), len),
                    )));
                    context.push_diagnostic(diagnostic);
                }
                _ => {}
            }
        }
    }
}

const fn is_operator_token(token: TokenKind) -> bool {
    matches!(
        token,
        TokenKind::Plus
            | TokenKind::Minus
            | TokenKind::Star
            | TokenKind::Slash
            | TokenKind::Vbar
            | TokenKind::Amper
            | TokenKind::Less
            | TokenKind::Greater
            | TokenKind::Equal
            | TokenKind::Percent
            | TokenKind::NotEqual
            | TokenKind::EqEqual
            | TokenKind::LessEqual
            | TokenKind::GreaterEqual
            | TokenKind::CircumFlex
            | TokenKind::LeftShift
            | TokenKind::RightShift
            | TokenKind::DoubleStar
            | TokenKind::PlusEqual
            | TokenKind::MinusEqual
            | TokenKind::StarEqual
            | TokenKind::SlashEqual
            | TokenKind::PercentEqual
            | TokenKind::AmperEqual
            | TokenKind::VbarEqual
            | TokenKind::CircumflexEqual
            | TokenKind::LeftShiftEqual
            | TokenKind::RightShiftEqual
            | TokenKind::DoubleStarEqual
            | TokenKind::DoubleSlash
            | TokenKind::DoubleSlashEqual
            | TokenKind::ColonEqual
    )
}
