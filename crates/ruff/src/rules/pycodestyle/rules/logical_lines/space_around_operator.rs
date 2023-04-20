use ruff_text_size::TextRange;
use rustpython_parser::Tok;

use super::{LogicalLine, Whitespace};
use crate::checkers::logical_lines::LogicalLinesContext;
use ruff_diagnostics::Violation;
use ruff_macros::{derive_message_formats, violation};

/// ## What it does
/// Checks for extraneous tabs before an operator.
///
/// ## Why is this bad?
/// Per PEP 8, operators should be surrounded by at most a single space on either
/// side.
///
/// ## Example
/// ```python
/// a = 4\t+ 5
/// ```
///
/// Use instead:
/// ```python
/// a = 12 + 3
/// ```
///
/// ## References
/// - [PEP 8](https://peps.python.org/pep-0008/#whitespace-in-expressions-and-statements)
#[violation]
pub struct TabBeforeOperator;

impl Violation for TabBeforeOperator {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Tab before operator")
    }
}

/// ## What it does
/// Checks for extraneous whitespace before an operator.
///
/// ## Why is this bad?
/// Per PEP 8, operators should be surrounded by at most a single space on either
/// side.
///
/// ## Example
/// ```python
/// a = 4  + 5
/// ```
///
/// Use instead:
/// ```python
/// a = 12 + 3
/// ```
///
/// ## References
/// - [PEP 8](https://peps.python.org/pep-0008/#whitespace-in-expressions-and-statements)
#[violation]
pub struct MultipleSpacesBeforeOperator;

impl Violation for MultipleSpacesBeforeOperator {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Multiple spaces before operator")
    }
}

/// ## What it does
/// Checks for extraneous tabs after an operator.
///
/// ## Why is this bad?
/// Per PEP 8, operators should be surrounded by at most a single space on either
/// side.
///
/// ## Example
/// ```python
/// a = 4 +\t5
/// ```
///
/// Use instead:
/// ```python
/// a = 12 + 3
/// ```
///
/// ## References
/// - [PEP 8](https://peps.python.org/pep-0008/#whitespace-in-expressions-and-statements)
#[violation]
pub struct TabAfterOperator;

impl Violation for TabAfterOperator {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Tab after operator")
    }
}

/// ## What it does
/// Checks for extraneous whitespace after an operator.
///
/// ## Why is this bad?
/// Per PEP 8, operators should be surrounded by at most a single space on either
/// side.
///
/// ## Example
/// ```python
/// a = 4 +  5
/// ```
///
/// Use instead:
/// ```python
/// a = 12 + 3
/// ```
///
/// ## References
/// - [PEP 8](https://peps.python.org/pep-0008/#whitespace-in-expressions-and-statements)
#[violation]
pub struct MultipleSpacesAfterOperator;

impl Violation for MultipleSpacesAfterOperator {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Multiple spaces after operator")
    }
}

/// E221, E222, E223, E224
pub(crate) fn space_around_operator(line: &LogicalLine, context: &mut LogicalLinesContext) {
    let mut after_operator = false;

    for token in line.tokens() {
        let is_operator = is_operator_token(&token);

        if is_operator {
            if !after_operator {
                match line.leading_whitespace(&token) {
                    (Whitespace::Tab, offset) => {
                        let start = token.start();
                        context.push(TabBeforeOperator, TextRange::empty(start - offset));
                    }
                    (Whitespace::Many, offset) => {
                        let start = token.start();
                        context.push(
                            MultipleSpacesBeforeOperator,
                            TextRange::empty(start - offset),
                        );
                    }
                    _ => {}
                }
            }

            match line.trailing_whitespace(&token) {
                Whitespace::Tab => {
                    let end = token.end();
                    context.push(TabAfterOperator, TextRange::empty(end));
                }
                Whitespace::Many => {
                    let end = token.end();
                    context.push(MultipleSpacesAfterOperator, TextRange::empty(end));
                }
                _ => {}
            }
        }

        after_operator = is_operator;
    }
}

const fn is_operator_token(token: &Tok) -> bool {
    matches!(
        token,
        Tok::Plus
            | Tok::Minus
            | Tok::Star
            | Tok::Slash
            | Tok::Vbar
            | Tok::Amper
            | Tok::Less
            | Tok::Greater
            | Tok::Equal
            | Tok::Percent
            | Tok::NotEqual
            | Tok::LessEqual
            | Tok::GreaterEqual
            | Tok::CircumFlex
            | Tok::LeftShift
            | Tok::RightShift
            | Tok::DoubleStar
            | Tok::PlusEqual
            | Tok::MinusEqual
            | Tok::StarEqual
            | Tok::SlashEqual
            | Tok::PercentEqual
            | Tok::AmperEqual
            | Tok::VbarEqual
            | Tok::CircumflexEqual
            | Tok::LeftShiftEqual
            | Tok::RightShiftEqual
            | Tok::DoubleStarEqual
            | Tok::DoubleSlash
            | Tok::DoubleSlashEqual
            | Tok::ColonEqual
    )
}
