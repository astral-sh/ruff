use ruff_diagnostics::DiagnosticKind;
use ruff_diagnostics::Violation;
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::token_kind::TokenKind;
use rustpython_parser::ast::Location;

use super::LogicalLine;

/// ## What it does
/// Checks for indentation with a non-multiple of 4 spaces.
///
/// ## Why is this bad?
/// Per PEP 8, 4 spaces per indentation level should be preferred.
///
/// ## Example
/// ```python
/// if True:
///    a = 1
/// ```
///
/// Use instead:
/// ```python
/// if True:
///     a = 1
/// ```
///
/// ## References
/// - [PEP 8](https://peps.python.org/pep-0008/#indentation)
#[violation]
pub struct IndentationWithInvalidMultiple {
    pub indent_size: usize,
}

impl Violation for IndentationWithInvalidMultiple {
    #[derive_message_formats]
    fn message(&self) -> String {
        let Self { indent_size } = self;
        format!("Indentation is not a multiple of {indent_size}")
    }
}

/// ## What it does
/// Checks for indentation of comments with a non-multiple of 4 spaces.
///
/// ## Why is this bad?
/// Per PEP 8, 4 spaces per indentation level should be preferred.
///
/// ## Example
/// ```python
/// if True:
///    # a = 1
/// ```
///
/// Use instead:
/// ```python
/// if True:
///     # a = 1
/// ```
///
/// ## References
/// - [PEP 8](https://peps.python.org/pep-0008/#indentation)
#[violation]
pub struct IndentationWithInvalidMultipleComment {
    pub indent_size: usize,
}

impl Violation for IndentationWithInvalidMultipleComment {
    #[derive_message_formats]
    fn message(&self) -> String {
        let Self { indent_size } = self;
        format!("Indentation is not a multiple of {indent_size} (comment)")
    }
}

/// ## What it does
/// Checks for indented blocks that are lacking indentation.
///
/// ## Why is this bad?
/// All indented blocks should be indented; otherwise, they are not valid
/// Python syntax.
///
/// ## Example
/// ```python
/// for item in items:
/// pass
///
/// ```
///
/// Use instead:
/// ```python
/// for item in items:
///     pass
/// ```
///
/// ## References
/// - [PEP 8](https://peps.python.org/pep-0008/#indentation)
#[violation]
pub struct NoIndentedBlock;

impl Violation for NoIndentedBlock {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Expected an indented block")
    }
}

/// ## What it does
/// Checks for comments in a code blocks that are lacking indentation.
///
/// ## Why is this bad?
/// Comments within an indented block should themselves be indented, to
/// indicate that they are part of the block.
///
/// ## Example
/// ```python
/// for item in items:
/// # Hi
///     pass
/// ```
///
/// Use instead:
/// ```python
/// for item in items:
///     # Hi
///     pass
/// ```
///
/// ## References
/// - [PEP 8](https://peps.python.org/pep-0008/#indentation)
#[violation]
pub struct NoIndentedBlockComment;

impl Violation for NoIndentedBlockComment {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Expected an indented block (comment)")
    }
}

/// ## What it does
/// Checks for unexpected indentation.
///
/// ## Why is this bad?
/// Indentation outside of a code block is not valid Python syntax.
///
/// ## Example
/// ```python
/// a = 1
///     b = 2
/// ```
///
/// Use instead:
/// ```python
/// a = 1
/// b = 2
/// ```
///
/// ## References
/// - [PEP 8](https://peps.python.org/pep-0008/#indentation)
#[violation]
pub struct UnexpectedIndentation;

impl Violation for UnexpectedIndentation {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Unexpected indentation")
    }
}

/// ## What it does
/// Checks for unexpected indentation of comment.
///
/// ## Why is this bad?
/// Comments should match the indentation of the containing code block.
///
/// ## Example
/// ```python
/// a = 1
///     # b = 2
/// ```
///
/// Use instead:
/// ```python
/// a = 1
/// # b = 2
/// ```
///
/// ## References
/// - [PEP 8](https://peps.python.org/pep-0008/#indentation)
#[violation]
pub struct UnexpectedIndentationComment;

impl Violation for UnexpectedIndentationComment {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Unexpected indentation (comment)")
    }
}

/// ## What it does
/// Checks for over-indented code.
///
/// ## Why is this bad?
/// Per PEP 8, 4 spaces per indentation level should be preferred. Increased
/// indentation can lead to inconsistent formatting, which can hurt
/// readability.
///
/// ## Example
/// ```python
/// for item in items:
///       pass
/// ```
///
/// Use instead:
/// ```python
/// for item in items:
///     pass
/// ```
///
/// ## References
/// - [PEP 8](https://peps.python.org/pep-0008/#indentation)
#[violation]
pub struct OverIndented;

impl Violation for OverIndented {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Over-indented")
    }
}

/// E111, E114, E112, E113, E115, E116, E117
pub(crate) fn indentation(
    logical_line: &LogicalLine,
    prev_logical_line: Option<&LogicalLine>,
    indent_char: char,
    indent_level: usize,
    prev_indent_level: Option<usize>,
    indent_size: usize,
) -> Vec<(Location, DiagnosticKind)> {
    let mut diagnostics = vec![];

    let location = logical_line.first_token_location().unwrap();

    if indent_level % indent_size != 0 {
        diagnostics.push((
            location,
            if logical_line.is_comment_only() {
                IndentationWithInvalidMultipleComment { indent_size }.into()
            } else {
                IndentationWithInvalidMultiple { indent_size }.into()
            },
        ));
    }
    let indent_expect = prev_logical_line
        .and_then(|prev_logical_line| prev_logical_line.tokens_trimmed().last())
        .map_or(false, |t| t.kind() == TokenKind::Colon);

    if indent_expect && indent_level <= prev_indent_level.unwrap_or(0) {
        diagnostics.push((
            location,
            if logical_line.is_comment_only() {
                NoIndentedBlockComment.into()
            } else {
                NoIndentedBlock.into()
            },
        ));
    } else if !indent_expect
        && prev_indent_level.map_or(false, |prev_indent_level| indent_level > prev_indent_level)
    {
        diagnostics.push((
            location,
            if logical_line.is_comment_only() {
                UnexpectedIndentationComment.into()
            } else {
                UnexpectedIndentation.into()
            },
        ));
    }
    if indent_expect {
        let expected_indent_amount = if indent_char == '\t' { 8 } else { 4 };
        let expected_indent_level = prev_indent_level.unwrap_or(0) + expected_indent_amount;
        if indent_level > expected_indent_level {
            diagnostics.push((location, OverIndented.into()));
        }
    }

    diagnostics
}
