#![allow(dead_code)]

use once_cell::sync::Lazy;
use regex::Regex;
use ruff_macros::{define_violation, derive_message_formats};

use crate::registry::DiagnosticKind;
use crate::violation::Violation;

define_violation!(
    pub struct WhitespaceAfterOpenBracket;
);
impl Violation for WhitespaceAfterOpenBracket {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Whitespace after '('")
    }
}

define_violation!(
    pub struct WhitespaceBeforeCloseBracket;
);
impl Violation for WhitespaceBeforeCloseBracket {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Whitespace before ')'")
    }
}

define_violation!(
    pub struct WhitespaceBeforePunctuation;
);
impl Violation for WhitespaceBeforePunctuation {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Whitespace before ',', ';', or ':'")
    }
}

// TODO(charlie): Pycodestyle has a negative lookahead on the end.
static EXTRANEOUS_WHITESPACE_REGEX: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"([\[({][ \t]|[ \t][]}),;:])").unwrap());

/// E201, E202, E203
#[cfg(feature = "logical_lines")]
pub fn extraneous_whitespace(line: &str) -> Vec<(usize, DiagnosticKind)> {
    let mut diagnostics = vec![];
    for line_match in EXTRANEOUS_WHITESPACE_REGEX.captures_iter(line) {
        let match_ = line_match.get(1).unwrap();
        let text = match_.as_str();
        let char = text.trim();
        let found = match_.start();
        if text.chars().last().unwrap().is_ascii_whitespace() {
            diagnostics.push((found + 1, WhitespaceAfterOpenBracket.into()));
        } else if line.chars().nth(found - 1).map_or(false, |c| c != ',') {
            if char == "}" || char == "]" || char == ")" {
                diagnostics.push((found, WhitespaceBeforeCloseBracket.into()));
            } else {
                diagnostics.push((found, WhitespaceBeforePunctuation.into()));
            }
        }
    }
    diagnostics
}

#[cfg(not(feature = "logical_lines"))]
pub fn extraneous_whitespace(_line: &str) -> Vec<(usize, DiagnosticKind)> {
    vec![]
}
