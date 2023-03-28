use itertools::Itertools;
use rustpython_parser::ast::Location;

use ruff_diagnostics::DiagnosticKind;
use ruff_diagnostics::Violation;
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::token_kind::TokenKind;

use super::LogicalLineTokens;

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

        if tok0_kind.is_keyword()
            && !(tok0_kind.is_singleton()
                || matches!(tok0_kind, TokenKind::Async | TokenKind::Await)
                || tok0_kind == TokenKind::Except && tok1_kind == TokenKind::Star
                || tok0_kind == TokenKind::Yield && tok1_kind == TokenKind::Rpar
                || matches!(tok1_kind, TokenKind::Colon | TokenKind::Newline))
            && tok0.end() == tok1.start()
        {
            diagnostics.push((tok0.end(), MissingWhitespaceAfterKeyword.into()));
        }
    }
    diagnostics
}
