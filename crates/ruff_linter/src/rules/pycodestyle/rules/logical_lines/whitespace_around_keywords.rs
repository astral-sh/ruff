use ruff_diagnostics::{AlwaysFixableViolation, Diagnostic, Edit, Fix};
use ruff_macros::{derive_message_formats, ViolationMetadata};
use ruff_text_size::{Ranged, TextRange};

use crate::checkers::logical_lines::LogicalLinesContext;

use super::{LogicalLine, Whitespace};

/// ## What it does
/// Checks for extraneous whitespace after keywords.
///
/// ## Why is this bad?
///
///
/// ## Example
/// ```python
/// True and  False
/// ```
///
/// Use instead:
/// ```python
/// True and False
/// ```
#[derive(ViolationMetadata)]
pub(crate) struct MultipleSpacesAfterKeyword;

impl AlwaysFixableViolation for MultipleSpacesAfterKeyword {
    #[derive_message_formats]
    fn message(&self) -> String {
        "Multiple spaces after keyword".to_string()
    }

    fn fix_title(&self) -> String {
        "Replace with single space".to_string()
    }
}

/// ## What it does
/// Checks for extraneous whitespace before keywords.
///
/// ## Why is this bad?
///
///
/// ## Example
/// ```python
/// True  and False
/// ```
///
/// Use instead:
/// ```python
/// True and False
/// ```
#[derive(ViolationMetadata)]
pub(crate) struct MultipleSpacesBeforeKeyword;

impl AlwaysFixableViolation for MultipleSpacesBeforeKeyword {
    #[derive_message_formats]
    fn message(&self) -> String {
        "Multiple spaces before keyword".to_string()
    }

    fn fix_title(&self) -> String {
        "Replace with single space".to_string()
    }
}

/// ## What it does
/// Checks for extraneous tabs after keywords.
///
/// ## Why is this bad?
///
///
/// ## Example
/// ```python
/// True and\tFalse
/// ```
///
/// Use instead:
/// ```python
/// True and False
/// ```
#[derive(ViolationMetadata)]
pub(crate) struct TabAfterKeyword;

impl AlwaysFixableViolation for TabAfterKeyword {
    #[derive_message_formats]
    fn message(&self) -> String {
        "Tab after keyword".to_string()
    }

    fn fix_title(&self) -> String {
        "Replace with single space".to_string()
    }
}

/// ## What it does
/// Checks for extraneous tabs before keywords.
///
/// ## Why is this bad?
///
///
/// ## Example
/// ```python
/// True\tand False
/// ```
///
/// Use instead:
/// ```python
/// True and False
/// ```
#[derive(ViolationMetadata)]
pub(crate) struct TabBeforeKeyword;

impl AlwaysFixableViolation for TabBeforeKeyword {
    #[derive_message_formats]
    fn message(&self) -> String {
        "Tab before keyword".to_string()
    }

    fn fix_title(&self) -> String {
        "Replace with single space".to_string()
    }
}

/// E271, E272, E273, E274
pub(crate) fn whitespace_around_keywords(line: &LogicalLine, context: &mut LogicalLinesContext) {
    let mut after_keyword = false;

    for token in line.tokens() {
        let is_keyword = token.kind().is_keyword();
        if is_keyword {
            if !after_keyword {
                match line.leading_whitespace(token) {
                    (Whitespace::Tab, offset) => {
                        let start = token.start();
                        let mut diagnostic = Diagnostic::new(
                            TabBeforeKeyword,
                            TextRange::at(start - offset, offset),
                        );
                        diagnostic.set_fix(Fix::safe_edit(Edit::range_replacement(
                            " ".to_string(),
                            TextRange::at(start - offset, offset),
                        )));
                        context.push_diagnostic(diagnostic);
                    }
                    (Whitespace::Many, offset) => {
                        let start = token.start();
                        let mut diagnostic = Diagnostic::new(
                            MultipleSpacesBeforeKeyword,
                            TextRange::at(start - offset, offset),
                        );
                        diagnostic.set_fix(Fix::safe_edit(Edit::range_replacement(
                            " ".to_string(),
                            TextRange::at(start - offset, offset),
                        )));
                        context.push_diagnostic(diagnostic);
                    }
                    _ => {}
                }
            }

            match line.trailing_whitespace(token) {
                (Whitespace::Tab, len) => {
                    let mut diagnostic =
                        Diagnostic::new(TabAfterKeyword, TextRange::at(token.end(), len));
                    diagnostic.set_fix(Fix::safe_edit(Edit::range_replacement(
                        " ".to_string(),
                        TextRange::at(token.end(), len),
                    )));
                    context.push_diagnostic(diagnostic);
                }
                (Whitespace::Many, len) => {
                    let mut diagnostic = Diagnostic::new(
                        MultipleSpacesAfterKeyword,
                        TextRange::at(token.end(), len),
                    );
                    diagnostic.set_fix(Fix::safe_edit(Edit::range_replacement(
                        " ".to_string(),
                        TextRange::at(token.end(), len),
                    )));
                    context.push_diagnostic(diagnostic);
                }
                _ => {}
            }
        }

        after_keyword = is_keyword;
    }
}
