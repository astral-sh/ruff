use ruff_diagnostics::Violation;
use ruff_macros::{derive_message_formats, violation};
use ruff_python_parser::TokenKind;
use ruff_text_size::Ranged;

use crate::checkers::logical_lines::LogicalLinesContext;
use crate::rules::pycodestyle::rules::logical_lines::LogicalLine;

/// ## What it does
/// Checks for missing whitespace after keywords.
///
/// ## Why is this bad?
/// Missing whitespace after keywords makes the code harder to read.
///
/// ## Example
/// ```python
/// if(True):
///     pass
/// ```
///
/// Use instead:
/// ```python
/// if (True):
///     pass
/// ```
///
/// ## References
/// - [Python documentation: Keywords](https://docs.python.org/3/reference/lexical_analysis.html#keywords)
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
    for window in line.tokens().windows(2) {
        let tok0 = &window[0];
        let tok1 = &window[1];

        let tok0_kind = tok0.kind();
        let tok1_kind = tok1.kind();

        if tok0_kind.is_keyword()
            && !(tok0_kind.is_singleton()
                || matches!(tok0_kind, TokenKind::Async | TokenKind::Await)
                || tok0_kind == TokenKind::Except && tok1_kind == TokenKind::Star
                || tok0_kind == TokenKind::Yield && tok1_kind == TokenKind::Rpar
                || matches!(
                    tok1_kind,
                    TokenKind::Colon | TokenKind::Newline | TokenKind::NonLogicalNewline
                ))
            && tok0.end() == tok1.start()
        {
            context.push(MissingWhitespaceAfterKeyword, tok0.range());
        }
    }
}
