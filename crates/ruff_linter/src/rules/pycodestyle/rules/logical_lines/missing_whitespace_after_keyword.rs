use ruff_diagnostics::{AlwaysFixableViolation, Diagnostic, Edit, Fix};
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

impl AlwaysFixableViolation for MissingWhitespaceAfterKeyword {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Missing whitespace after keyword")
    }

    fn fix_title(&self) -> String {
        format!("Added missing whitespace after keyword")
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
            let mut diagnostic = Diagnostic::new(MissingWhitespaceAfterKeyword, tok0.range());
            diagnostic.set_fix(Fix::safe_edit(Edit::insertion(" ".to_string(), tok0.end())));
            context.push_diagnostic(diagnostic);
        }
    }
}
