use ruff_diagnostics::Violation;
use ruff_macros::{derive_message_formats, violation};
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
                        context.push(TabBeforeKeyword, TextRange::at(start - offset, offset));
                    }
                    (Whitespace::Many, offset) => {
                        let start = token.start();
                        context.push(
                            MultipleSpacesBeforeKeyword,
                            TextRange::at(start - offset, offset),
                        );
                    }
                    _ => {}
                }
            }

            match line.trailing_whitespace(token) {
                (Whitespace::Tab, len) => {
                    context.push(TabAfterKeyword, TextRange::at(token.end(), len));
                }
                (Whitespace::Many, len) => {
                    context.push(MultipleSpacesAfterKeyword, TextRange::at(token.end(), len));
                }
                _ => {}
            }
        }

        after_keyword = is_keyword;
    }
}
