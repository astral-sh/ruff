#![allow(dead_code, unused_imports, unused_variables)]

use once_cell::sync::Lazy;
use regex::Regex;
use rustpython_parser::ast::Location;
use rustpython_parser::Tok;

use crate::rules::pycodestyle::helpers::is_keyword_token;
use crate::rules::pycodestyle::logical_lines::{LogicalLine, LogicalLineTokens};
use crate::rules::pycodestyle::rules::Whitespace;
use ruff_diagnostics::DiagnosticKind;
use ruff_diagnostics::Violation;
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::source_code::Locator;

/// ## What it does
/// Checks for extraneous whitespace after keywords.
///
/// ## Why is this bad?
///
///
/// ## Example
/// ```python
/// True and  False
/// ```
///
/// Use instead:
/// ```python
/// True and False
/// ```
#[violation]
pub struct MultipleSpacesAfterKeyword;

impl Violation for MultipleSpacesAfterKeyword {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Multiple spaces after keyword")
    }
}

/// ## What it does
/// Checks for extraneous whitespace before keywords.
///
/// ## Why is this bad?
///
///
/// ## Example
/// ```python
/// True  and False
///
/// ```
///
/// Use instead:
/// ```python
/// True and False
/// ```
#[violation]
pub struct MultipleSpacesBeforeKeyword;

impl Violation for MultipleSpacesBeforeKeyword {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Multiple spaces before keyword")
    }
}

/// ## What it does
/// Checks for extraneous tabs after keywords.
///
/// ## Why is this bad?
///
///
/// ## Example
/// ```python
/// True and\tFalse
///
/// ```
///
/// Use instead:
/// ```python
/// True and False
/// ```
#[violation]
pub struct TabAfterKeyword;

impl Violation for TabAfterKeyword {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Tab after keyword")
    }
}

/// ## What it does
/// Checks for extraneous tabs before keywords.
///
/// ## Why is this bad?
///
///
/// ## Example
/// ```python
/// True\tand False
///
/// ```
///
/// Use instead:
/// ```python
/// True and False
/// ```
#[violation]
pub struct TabBeforeKeyword;

impl Violation for TabBeforeKeyword {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Tab before keyword")
    }
}

/// E271, E272, E273, E274
#[cfg(feature = "logical_lines")]
pub fn whitespace_around_keywords(line: &LogicalLine) -> Vec<(Location, DiagnosticKind)> {
    let mut diagnostics = vec![];
    let mut after_keyword = false;

    for token in line.tokens() {
        let is_keyword = is_keyword_token(token.kind());

        if is_keyword {
            let (start, end) = token.range();
            let before = line.text_before(&token);

            if !after_keyword {
                match Whitespace::trailing(before) {
                    (Whitespace::Tab, offset) => diagnostics.push((
                        Location::new(start.row(), start.column() - offset),
                        TabBeforeKeyword.into(),
                    )),
                    (Whitespace::Many, offset) => diagnostics.push((
                        Location::new(start.row(), start.column() - offset),
                        MultipleSpacesBeforeKeyword.into(),
                    )),
                    _ => {}
                }
            }

            let after = line.text_after(&token);
            match Whitespace::leading(after) {
                Whitespace::Tab => diagnostics.push((end, TabAfterKeyword.into())),
                Whitespace::Many => diagnostics.push((end, MultipleSpacesAfterKeyword.into())),
                _ => {}
            }
        }

        after_keyword = is_keyword;
    }

    diagnostics
}

#[cfg(not(feature = "logical_lines"))]
pub fn whitespace_around_keywords(_line: &LogicalLine) -> Vec<(Location, DiagnosticKind)> {
    vec![]
}
