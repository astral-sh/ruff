#![allow(dead_code)]

use ruff_macros::{define_violation, derive_message_formats};

use crate::registry::DiagnosticKind;
use crate::rules::pycodestyle::logical_lines::LogicalLine;
use crate::violation::Violation;

define_violation!(
    pub struct IndentationWithInvalidMultiple {
        pub indent_size: usize,
    }
);
impl Violation for IndentationWithInvalidMultiple {
    #[derive_message_formats]
    fn message(&self) -> String {
        let Self { indent_size } = self;
        format!("Indentation is not a multiple of {indent_size}")
    }
}

define_violation!(
    pub struct IndentationWithInvalidMultipleComment {
        pub indent_size: usize,
    }
);
impl Violation for IndentationWithInvalidMultipleComment {
    #[derive_message_formats]
    fn message(&self) -> String {
        let Self { indent_size } = self;
        format!("Indentation is not a multiple of {indent_size} (comment)")
    }
}

define_violation!(
    pub struct NoIndentedBlock;
);
impl Violation for NoIndentedBlock {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Expected an indented block")
    }
}

define_violation!(
    pub struct NoIndentedBlockComment;
);
impl Violation for NoIndentedBlockComment {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Expected an indented block (comment)")
    }
}

define_violation!(
    pub struct UnexpectedIndentation;
);
impl Violation for UnexpectedIndentation {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Unexpected indentation")
    }
}

define_violation!(
    pub struct UnexpectedIndentationComment;
);
impl Violation for UnexpectedIndentationComment {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Unexpected indentation (comment)")
    }
}

define_violation!(
    pub struct OverIndented;
);
impl Violation for OverIndented {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Over-indented")
    }
}

/// E111
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
