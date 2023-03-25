#![allow(dead_code, unused_imports, unused_variables)]

use once_cell::sync::Lazy;
use regex::Regex;

use crate::rules::pycodestyle::rules::Whitespace;
use ruff_diagnostics::DiagnosticKind;
use ruff_diagnostics::Violation;
use ruff_macros::{derive_message_formats, violation};

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

static KEYWORD_REGEX: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"\b(False|None|True|and|as|assert|async|await|break|class|continue|def|del|elif|else|except|finally|for|from|global|if|import|in|is|lambda|nonlocal|not|or|pass|raise|return|try|while|with|yield)\b").unwrap()
});

/// E271, E272, E273, E274
#[cfg(feature = "logical_lines")]
pub fn whitespace_around_keywords(line: &str) -> Vec<(usize, DiagnosticKind)> {
    let mut diagnostics = vec![];
    for line_match in KEYWORD_REGEX.find_iter(line) {
        let before = &line[..line_match.start()];
        match Whitespace::trailing(before) {
            (Whitespace::Tab, offset) => {
                diagnostics.push((line_match.start() - offset, TabBeforeKeyword.into()));
            }
            (Whitespace::Many, offset) => diagnostics.push((
                line_match.start() - offset,
                MultipleSpacesBeforeKeyword.into(),
            )),
            _ => {}
        }

        let after = &line[line_match.end()..];
        match Whitespace::leading(after) {
            Whitespace::Tab => diagnostics.push((line_match.end(), TabAfterKeyword.into())),
            Whitespace::Many => {
                diagnostics.push((line_match.end(), MultipleSpacesAfterKeyword.into()));
            }
            _ => {}
        }
    }
    diagnostics
}

#[cfg(not(feature = "logical_lines"))]
pub fn whitespace_around_keywords(_line: &str) -> Vec<(usize, DiagnosticKind)> {
    vec![]
}
