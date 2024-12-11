use ruff_diagnostics::{AlwaysFixableViolation, Diagnostic, Edit, Fix};
use ruff_macros::{derive_message_formats, ViolationMetadata};
use ruff_python_index::Indexer;
use ruff_python_parser::TokenKind;
use ruff_source_file::LineRanges;
use ruff_text_size::{Ranged, TextRange, TextSize};

use crate::checkers::logical_lines::LogicalLinesContext;
use crate::Locator;

use super::LogicalLine;

/// ## What it does
/// Checks for redundant backslashes between brackets.
///
/// ## Why is this bad?
/// Explicit line joins using a backslash are redundant between brackets.
///
/// ## Example
/// ```python
/// x = (2 + \
///     2)
/// ```
///
/// Use instead:
/// ```python
/// x = (2 +
///     2)
/// ```
///
/// [PEP 8]: https://peps.python.org/pep-0008/#maximum-line-length
#[derive(ViolationMetadata)]
pub(crate) struct RedundantBackslash;

impl AlwaysFixableViolation for RedundantBackslash {
    #[derive_message_formats]
    fn message(&self) -> String {
        "Redundant backslash".to_string()
    }

    fn fix_title(&self) -> String {
        "Remove redundant backslash".to_string()
    }
}

/// E502
pub(crate) fn redundant_backslash(
    line: &LogicalLine,
    locator: &Locator,
    indexer: &Indexer,
    context: &mut LogicalLinesContext,
) {
    let mut parens = 0;
    let continuation_lines = indexer.continuation_line_starts();
    let mut start_index = 0;

    for token in line.tokens() {
        match token.kind() {
            TokenKind::Lpar | TokenKind::Lsqb | TokenKind::Lbrace => {
                if parens == 0 {
                    let start = locator.line_start(token.start());
                    start_index = continuation_lines
                        .binary_search(&start)
                        .unwrap_or_else(|err_index| err_index);
                }
                parens += 1;
            }
            TokenKind::Rpar | TokenKind::Rsqb | TokenKind::Rbrace => {
                parens -= 1;
                if parens == 0 {
                    let end = locator.line_start(token.start());
                    let end_index = continuation_lines
                        .binary_search(&end)
                        .unwrap_or_else(|err_index| err_index);
                    for continuation_line in &continuation_lines[start_index..end_index] {
                        let backslash_end = locator.line_end(*continuation_line);
                        let backslash_start = backslash_end - TextSize::new(1);
                        let mut diagnostic = Diagnostic::new(
                            RedundantBackslash,
                            TextRange::new(backslash_start, backslash_end),
                        );
                        diagnostic.set_fix(Fix::safe_edit(Edit::deletion(
                            backslash_start,
                            backslash_end,
                        )));
                        context.push_diagnostic(diagnostic);
                    }
                }
            }
            _ => continue,
        }
    }
}
