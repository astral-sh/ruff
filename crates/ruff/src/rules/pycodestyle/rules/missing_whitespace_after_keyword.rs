#![allow(dead_code)]

use std::clone;

use once_cell::sync::Lazy;
use regex::Regex;

use ruff_macros::{define_violation, derive_message_formats};

use crate::registry::DiagnosticKind;
use crate::violation::Violation;

use crate::rules::pycodestyle::helpers::{is_keyword_token, is_singleton_token};

use rustpython_parser::ast::Location;
use rustpython_parser::Tok;

define_violation!(
    pub struct MissingWhitespaceAfterKeyword;
);
impl Violation for MissingWhitespaceAfterKeyword {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Missing whitespace after keyword")
    }
}

/// E275
pub fn missing_whitespace_after_keyword(
    tokens: &Vec<(Location, &Tok, Location)>,
) -> Vec<(usize, DiagnosticKind)> {
    let mut diagnostics = vec![];
    let mut tokens_clone = tokens.clone();
    tokens_clone.remove(0);

    for (tok0, tok1) in tokens.iter().zip(tokens_clone.iter()) {
        if tok0.2 == tok1.0
            && is_keyword_token(tok0.1)
            && !is_singleton_token(tok0.1)
            && *tok0.1 != Tok::Async
            && *tok0.1 != Tok::Await
            && !(*tok0.1 == Tok::Except && *tok1.1 == Tok::Star)
            && !(*tok0.1 == Tok::Yield && *tok1.1 == Tok::Rpar)
            && *tok1.1 != Tok::Colon
            && *tok1.1 != Tok::Newline
        {
            diagnostics.push((tok0.2.column(), MissingWhitespaceAfterKeyword.into()));
        }
    }
    diagnostics
}

#[cfg(not(feature = "logical_lines"))]
pub fn missing_whitespace_after_keyword(_line: &str) -> Vec<(usize, DiagnosticKind)> {
    vec![]
}
