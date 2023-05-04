use super::{LogicalLine, Whitespace};
use crate::checkers::logical_lines::LogicalLinesContext;
use ruff_diagnostics::Violation;
use ruff_macros::{derive_message_formats, violation};
use ruff_text_size::TextRange;

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
#[violation]
pub struct MultipleSpacesAfterKeyword;

impl Violation for MultipleSpacesAfterKeyword {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Multiple spaces after keyword")
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
///
/// ```
///
/// Use instead:
/// ```python
/// True and False
/// ```
#[violation]
pub struct MultipleSpacesBeforeKeyword;

impl Violation for MultipleSpacesBeforeKeyword {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Multiple spaces before keyword")
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
///
/// ```
///
/// Use instead:
/// ```python
/// True and False
/// ```
#[violation]
pub struct TabAfterKeyword;

impl Violation for TabAfterKeyword {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Tab after keyword")
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
///
/// ```
///
/// Use instead:
/// ```python
/// True and False
/// ```
#[violation]
pub struct TabBeforeKeyword;

impl Violation for TabBeforeKeyword {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Tab before keyword")
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
                        context.push(TabBeforeKeyword, TextRange::empty(start - offset));
                    }
                    (Whitespace::Many, offset) => {
                        let start = token.start();
                        context.push(
                            MultipleSpacesBeforeKeyword,
                            TextRange::empty(start - offset),
                        );
                    }
                    _ => {}
                }
            }

            match line.trailing_whitespace(token) {
                Whitespace::Tab => {
                    let end = token.end();
                    context.push(TabAfterKeyword, TextRange::empty(end));
                }
                Whitespace::Many => {
                    let end = token.end();
                    context.push(MultipleSpacesAfterKeyword, TextRange::empty(end));
                }
                _ => {}
            }
        }

        after_keyword = is_keyword;
    }
}
