#![allow(dead_code, unused_imports, unused_variables)]

use ruff_diagnostics::DiagnosticKind;
use ruff_diagnostics::Violation;
use ruff_macros::{derive_message_formats, violation};

use crate::rules::pycodestyle::logical_lines::LogicalLine;

/// ## What it does
/// Checks for indentation with invalid multiple of 4 spaces.
///
/// ## Why is this bad?
/// Use indent_size (PEP8 says 4) spaces per indentation level.
/// For really old code that you don't want to mess up, you can continue
/// to use 8-space tabs.
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
/// Checks for indentation of comments with invalid multiple of 4 spaces.
///
/// ## Why is this bad?
/// Use indent_size (PEP8 says 4) spaces per indentation level.
/// For really old code that you don't want to mess up, you can continue
/// to use 8-space tabs.
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
/// Checks for missing indented block.
///
/// ## Why is this bad?
/// Use indent_size (PEP8 says 4) spaces per indentation level.
/// For really old code that you don't want to mess up, you can continue
/// to use 8-space tabs.
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
#[violation]
pub struct NoIndentedBlock;

impl Violation for NoIndentedBlock {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Expected an indented block")
    }
}

/// ## What it does
/// Checks for missing indented comment in a block.
///
/// ## Why is this bad?
/// Use indent_size (PEP8 says 4) spaces per indentation level.
/// For really old code that you don't want to mess up, you can continue
/// to use 8-space tabs.
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
/// Use indent_size (PEP8 says 4) spaces per indentation level.
/// For really old code that you don't want to mess up, you can continue
/// to use 8-space tabs.
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
/// Use indent_size (PEP8 says 4) spaces per indentation level.
/// For really old code that you don't want to mess up, you can continue
/// to use 8-space tabs.
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
#[violation]
pub struct UnexpectedIndentationComment;

impl Violation for UnexpectedIndentationComment {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Unexpected indentation (comment)")
    }
}

/// ## What it does
/// Checks for over indented code.
///
/// ## Why is this bad?
/// Use indent_size (PEP8 says 4) spaces per indentation level.
/// For really old code that you don't want to mess up, you can continue
/// to use 8-space tabs.
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
#[violation]
pub struct OverIndented;

impl Violation for OverIndented {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Over-indented")
    }
}

/// E111, E114, E112, E113, E115, E116, E117
#[cfg(feature = "logical_lines")]
pub fn indentation(
    logical_line: &LogicalLine,
    prev_logical_line: Option<&LogicalLine>,
    indent_char: char,
    indent_level: usize,
    prev_indent_level: Option<usize>,
    indent_size: usize,
) -> Vec<(usize, DiagnosticKind)> {
    let mut diagnostics = vec![];
    if indent_level % indent_size != 0 {
        diagnostics.push((
            0,
            if logical_line.is_comment() {
                IndentationWithInvalidMultipleComment { indent_size }.into()
            } else {
                IndentationWithInvalidMultiple { indent_size }.into()
            },
        ));
    }
    let indent_expect = prev_logical_line.map_or(false, |prev_logical_line| {
        prev_logical_line.text.ends_with(':')
    });
    if indent_expect && indent_level <= prev_indent_level.unwrap_or(0) {
        diagnostics.push((
            0,
            if logical_line.is_comment() {
                NoIndentedBlockComment.into()
            } else {
                NoIndentedBlock.into()
            },
        ));
    } else if !indent_expect
        && prev_indent_level.map_or(false, |prev_indent_level| indent_level > prev_indent_level)
    {
        diagnostics.push((
            0,
            if logical_line.is_comment() {
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
            diagnostics.push((0, OverIndented.into()));
        }
    }
    diagnostics
}

#[cfg(not(feature = "logical_lines"))]
pub fn indentation(
    _logical_line: &LogicalLine,
    _prev_logical_line: Option<&LogicalLine>,
    _indent_char: char,
    _indent_level: usize,
    _prev_indent_level: Option<usize>,
    _indent_size: usize,
) -> Vec<(usize, DiagnosticKind)> {
    vec![]
}
