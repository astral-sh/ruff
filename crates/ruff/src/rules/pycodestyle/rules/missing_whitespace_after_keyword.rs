#![allow(dead_code, unused_imports)]

use rustpython_parser::ast::Location;
use rustpython_parser::Tok;

use ruff_macros::{define_violation, derive_message_formats};

use crate::registry::DiagnosticKind;
use crate::rules::pycodestyle::helpers::{is_keyword_token, is_singleton_token};
use crate::violation::Violation;

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
#[cfg(feature = "logical_lines")]
pub fn missing_whitespace_after_keyword(
    tokens: &[(Location, &Tok, Location)],
) -> Vec<(Location, DiagnosticKind)> {
    let mut diagnostics = vec![];

    for (tok0, tok1) in tokens.iter().zip(&tokens[1..]) {
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
            diagnostics.push((tok0.2, MissingWhitespaceAfterKeyword.into()));
        }
    }
    diagnostics
}

#[cfg(not(feature = "logical_lines"))]
pub fn missing_whitespace_after_keyword(
    _tokens: &[(Location, &Tok, Location)],
) -> Vec<(Location, DiagnosticKind)> {
    vec![]
}
