#![allow(dead_code, unused_imports, unused_variables)]

use rustpython_parser::ast::Location;
use rustpython_parser::Tok;

use ruff_diagnostics::DiagnosticKind;
use ruff_diagnostics::Violation;
use ruff_macros::{derive_message_formats, violation};

use crate::rules::pycodestyle::helpers::{is_keyword_token, is_singleton_token};

#[violation]
pub struct MissingWhitespaceAfterKeyword;

impl Violation for MissingWhitespaceAfterKeyword {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Missing whitespace after keyword")
    }
}

/// E275
#[cfg(debug_assertions)]
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

#[cfg(not(debug_assertions))]
pub fn missing_whitespace_after_keyword(
    _tokens: &[(Location, &Tok, Location)],
) -> Vec<(Location, DiagnosticKind)> {
    vec![]
}
