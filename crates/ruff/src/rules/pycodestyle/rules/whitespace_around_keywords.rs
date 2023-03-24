#![allow(dead_code, unused_imports, unused_variables)]

use once_cell::sync::Lazy;
use regex::Regex;

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
    Regex::new(r"(\s*)\b(?:False|None|True|and|as|assert|async|await|break|class|continue|def|del|elif|else|except|finally|for|from|global|if|import|in|is|lambda|nonlocal|not|or|pass|raise|return|try|while|with|yield)\b(\s*)").unwrap()
});

/// E271, E272, E273, E274
#[cfg(debug_assertions)]
pub fn whitespace_around_keywords(line: &str) -> Vec<(usize, DiagnosticKind)> {
    let mut diagnostics = vec![];
    for line_match in KEYWORD_REGEX.captures_iter(line) {
        let before = line_match.get(1).unwrap();
        let after = line_match.get(2).unwrap();

        if before.as_str().contains('\t') {
            diagnostics.push((before.start(), TabBeforeKeyword.into()));
        } else if before.as_str().len() > 1 {
            diagnostics.push((before.start(), MultipleSpacesBeforeKeyword.into()));
        }

        if after.as_str().contains('\t') {
            diagnostics.push((after.start(), TabAfterKeyword.into()));
        } else if after.as_str().len() > 1 {
            diagnostics.push((after.start(), MultipleSpacesAfterKeyword.into()));
        }
    }
    diagnostics
}

#[cfg(not(debug_assertions))]
pub fn whitespace_around_keywords(_line: &str) -> Vec<(usize, DiagnosticKind)> {
    vec![]
}
