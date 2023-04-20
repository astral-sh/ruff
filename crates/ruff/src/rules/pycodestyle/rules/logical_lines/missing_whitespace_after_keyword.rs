use itertools::Itertools;
use ruff_text_size::TextRange;
use rustpython_parser::Tok;

use crate::checkers::logical_lines::LogicalLinesContext;
use crate::rules::pycodestyle::rules::logical_lines::LogicalLine;
use ruff_diagnostics::Violation;
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::token_kind::{is_keyword_token, is_singleton_token};

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
    line: &LogicalLine,
    context: &mut LogicalLinesContext,
) {
    for (tok0, tok1) in line.tokens().tuple_windows() {
        let tok0_kind = tok0.token();
        let tok1_kind = tok1.token();

        if is_keyword_token(tok0_kind)
            && !(is_singleton_token(tok0_kind)
                || matches!(tok0_kind, Tok::Async | Tok::Await)
                || tok0.is_except() && tok1.is_star()
                || tok0.is_yield() && tok1.is_rpar()
                || matches!(tok1_kind, Tok::Colon | Tok::Newline))
            && tok0.end() == tok1.start()
        {
            context.push(MissingWhitespaceAfterKeyword, TextRange::empty(tok0.end()));
        }
    }
}
