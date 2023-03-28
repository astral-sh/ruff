use itertools::Itertools;
use rustpython_parser::ast::Location;
use rustpython_parser::Tok;

use ruff_diagnostics::DiagnosticKind;
use ruff_diagnostics::Violation;
use ruff_macros::{derive_message_formats, violation};

use super::LogicalLineTokens;
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
pub(crate) fn missing_whitespace_after_keyword(
    tokens: &LogicalLineTokens,
) -> Vec<(Location, DiagnosticKind)> {
    let mut diagnostics = vec![];

    for (tok0, tok1) in tokens.iter().tuple_windows() {
        let tok0_kind = tok0.kind();
        let tok1_kind = tok1.kind();

        if is_keyword_token(tok0_kind)
            && !(is_singleton_token(tok0_kind)
                || matches!(tok0_kind, Tok::Async | Tok::Await)
                || tok0_kind == &Tok::Except && tok1_kind == &Tok::Star
                || tok0_kind == &Tok::Yield && tok1_kind == &Tok::Rpar
                || matches!(tok1_kind, Tok::Colon | Tok::Newline))
            && tok0.end() == tok1.start()
        {
            diagnostics.push((tok0.end(), MissingWhitespaceAfterKeyword.into()));
        }
    }
    diagnostics
}
