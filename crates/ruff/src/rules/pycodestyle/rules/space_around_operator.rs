#![allow(dead_code, unused_imports, unused_variables)]

use once_cell::sync::Lazy;
use regex::Regex;

use ruff_diagnostics::DiagnosticKind;
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

static OPERATOR_REGEX: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"[^,\s](\s*)(?:[-+*/|!<=>%&^]+|:=)(\s*)").unwrap());

/// E221, E222, E223, E224
#[cfg(debug_assertions)]
pub fn space_around_operator(line: &str) -> Vec<(usize, DiagnosticKind)> {
    let mut diagnostics = vec![];
    for line_match in OPERATOR_REGEX.captures_iter(line) {
        let before = line_match.get(1).unwrap();
        let after = line_match.get(2).unwrap();

        if before.as_str().contains('\t') {
            diagnostics.push((before.start(), TabBeforeOperator.into()));
        } else if before.as_str().len() > 1 {
            diagnostics.push((before.start(), MultipleSpacesBeforeOperator.into()));
        }

        if after.as_str().contains('\t') {
            diagnostics.push((after.start(), TabAfterOperator.into()));
        } else if after.as_str().len() > 1 {
            diagnostics.push((after.start(), MultipleSpacesAfterOperator.into()));
        }
    }
    diagnostics
}

#[cfg(not(debug_assertions))]
pub fn space_around_operator(_line: &str) -> Vec<(usize, DiagnosticKind)> {
    vec![]
}
