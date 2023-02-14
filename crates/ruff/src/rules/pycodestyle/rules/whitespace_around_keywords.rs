#![allow(dead_code)]

use once_cell::sync::Lazy;
use regex::Regex;

use ruff_macros::{define_violation, derive_message_formats};

use crate::registry::DiagnosticKind;
use crate::violation::Violation;

define_violation!(
    pub struct MultipleSpacesAfterKeyword;
);
impl Violation for MultipleSpacesAfterKeyword {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Multiple spaces after keyword")
    }
}

define_violation!(
    pub struct MultipleSpacesBeforeKeyword;
);
impl Violation for MultipleSpacesBeforeKeyword {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Multiple spaces before keyword")
    }
}

define_violation!(
    pub struct TabAfterKeyword;
);
impl Violation for TabAfterKeyword {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Tab after keyword")
    }
}

define_violation!(
    pub struct TabBeforeKeyword;
);
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
#[cfg(feature = "logical_lines")]
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

#[cfg(not(feature = "logical_lines"))]
pub fn whitespace_around_keywords(_line: &str) -> Vec<(usize, DiagnosticKind)> {
    vec![]
}
