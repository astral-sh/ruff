#![allow(dead_code, unused_imports, unused_variables)]

use once_cell::sync::Lazy;
use regex::Regex;
use rustpython_parser::ast::Location;
use rustpython_parser::Tok;

use crate::rules::pycodestyle::helpers::{is_op_token, is_ws_needed_token};
use crate::rules::pycodestyle::logical_lines::{LogicalLine, LogicalLineTokens, Whitespace};
use ruff_diagnostics::DiagnosticKind;
use ruff_diagnostics::Violation;
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::source_code::Locator;
use ruff_python_ast::types::Range;

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
#[cfg(feature = "logical_lines")]
pub(crate) fn space_around_operator(line: &LogicalLine) -> Vec<(Location, DiagnosticKind)> {
    let mut diagnostics = vec![];
    let mut after_operator = false;

    for token in line.tokens() {
        let is_operator = is_operator_token(token.kind());

        if is_operator {
            let (start, end) = token.range();

            if !after_operator {
                match line.leading_whitespace(&token) {
                    (Whitespace::Tab, offset) => diagnostics.push((
                        Location::new(start.row(), start.column() - offset),
                        TabBeforeOperator.into(),
                    )),
                    (Whitespace::Many, offset) => diagnostics.push((
                        Location::new(start.row(), start.column() - offset),
                        MultipleSpacesBeforeOperator.into(),
                    )),
                    _ => {}
                }
            }

            match line.trailing_whitespace(&token) {
                Whitespace::Tab => diagnostics.push((end, TabAfterOperator.into())),
                Whitespace::Many => diagnostics.push((end, MultipleSpacesAfterOperator.into())),
                _ => {}
            }
        }

        after_operator = is_operator;
    }

    diagnostics
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

#[cfg(not(feature = "logical_lines"))]
pub fn space_around_operator(_line: &LogicalLine) -> Vec<(Location, DiagnosticKind)> {
    vec![]
}
