use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_python_ast::token::TokenKind;
use ruff_text_size::Ranged;

use crate::checkers::ast::LintContext;
use crate::rules::pycodestyle::rules::logical_lines::LogicalLine;
use crate::{AlwaysFixableViolation, Edit, Fix};

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
#[derive(ViolationMetadata)]
#[violation_metadata(preview_since = "v0.0.269")]
pub(crate) struct MissingWhitespaceAfterKeyword;

impl AlwaysFixableViolation for MissingWhitespaceAfterKeyword {
    #[derive_message_formats]
    fn message(&self) -> String {
        "Missing whitespace after keyword".to_string()
    }

    fn fix_title(&self) -> String {
        "Added missing whitespace after keyword".to_string()
    }
}

/// E275
pub(crate) fn missing_whitespace_after_keyword(line: &LogicalLine, context: &LintContext) {
    for window in line.tokens().windows(2) {
        let tok0 = &window[0];
        let tok1 = &window[1];

        let tok0_kind = tok0.kind();
        let tok1_kind = tok1.kind();

        if tok0_kind.is_keyword()
            && !(tok0_kind.is_singleton()
                || matches!(tok0_kind, TokenKind::Async | TokenKind::Await)
                || tok0_kind == TokenKind::Except && tok1_kind == TokenKind::Star
                || tok0_kind == TokenKind::Yield
                    && matches!(tok1_kind, TokenKind::Rpar | TokenKind::Comma)
                || matches!(
                    tok1_kind,
                    TokenKind::Colon
                        | TokenKind::Semi
                        | TokenKind::Newline
                        | TokenKind::NonLogicalNewline
                        // In the event of a syntax error, do not attempt to add a whitespace.
                        | TokenKind::Rpar
                        | TokenKind::Rsqb
                        | TokenKind::Rbrace
                ))
            && tok0.end() == tok1.start()
        {
            if let Some(mut diagnostic) =
                context.report_diagnostic_if_enabled(MissingWhitespaceAfterKeyword, tok0.range())
            {
                diagnostic.set_fix(Fix::safe_edit(Edit::insertion(" ".to_string(), tok0.end())));
            }
        }
    }
}
