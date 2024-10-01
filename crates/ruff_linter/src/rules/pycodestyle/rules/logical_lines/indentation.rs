use ruff_diagnostics::DiagnosticKind;
use ruff_diagnostics::Violation;
use ruff_macros::{derive_message_formats, violation};
use ruff_python_parser::TokenKind;

use super::LogicalLine;

/// ## What it does
/// Checks for indentation with a non-multiple of 4 spaces.
///
/// ## Why is this bad?
/// According to [PEP 8], 4 spaces per indentation level should be preferred.
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
///
/// ## Formatter compatibility
/// We recommend against using this rule alongside the [formatter]. The
/// formatter enforces consistent indentation, making the rule redundant.
///
/// The rule is also incompatible with the [formatter] when using
/// `indent-width` with a value other than `4`.
///
/// ## Options
/// - `indent-width`
///
/// [PEP 8]: https://peps.python.org/pep-0008/#indentation
/// [formatter]:https://docs.astral.sh/ruff/formatter/
#[violation]
pub struct IndentationWithInvalidMultiple {
    indent_width: usize,
}

impl Violation for IndentationWithInvalidMultiple {
    #[derive_message_formats]
    fn message(&self) -> String {
        let Self { indent_width } = self;
        format!("Indentation is not a multiple of {indent_width}")
    }
}

/// ## What it does
/// Checks for indentation of comments with a non-multiple of 4 spaces.
///
/// ## Why is this bad?
/// According to [PEP 8], 4 spaces per indentation level should be preferred.
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
/// ## Formatter compatibility
/// We recommend against using this rule alongside the [formatter]. The
/// formatter enforces consistent indentation, making the rule redundant.
///
/// The rule is also incompatible with the [formatter] when using
/// `indent-width` with a value other than `4`.
///
/// ## Options
/// - `indent-width`
///
/// [PEP 8]: https://peps.python.org/pep-0008/#indentation
/// [formatter]:https://docs.astral.sh/ruff/formatter/
#[violation]
pub struct IndentationWithInvalidMultipleComment {
    indent_width: usize,
}

impl Violation for IndentationWithInvalidMultipleComment {
    #[derive_message_formats]
    fn message(&self) -> String {
        let Self { indent_width } = self;
        format!("Indentation is not a multiple of {indent_width} (comment)")
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
/// ```
///
/// Use instead:
/// ```python
/// for item in items:
///     pass
/// ```
///
/// [PEP 8]: https://peps.python.org/pep-0008/#indentation
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
/// [PEP 8]: https://peps.python.org/pep-0008/#indentation
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
/// [PEP 8]: https://peps.python.org/pep-0008/#indentation
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
/// [PEP 8]: https://peps.python.org/pep-0008/#indentation
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
/// According to [PEP 8], 4 spaces per indentation level should be preferred. Increased
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
/// [PEP 8]: https://peps.python.org/pep-0008/#indentation
#[violation]
pub struct OverIndented {
    is_comment: bool,
}

impl Violation for OverIndented {
    #[derive_message_formats]
    fn message(&self) -> String {
        let OverIndented { is_comment } = self;
        if *is_comment {
            format!("Over-indented (comment)")
        } else {
            format!("Over-indented")
        }
    }
}

/// E111, E112, E113, E114, E115, E116, E117
pub(crate) fn indentation(
    logical_line: &LogicalLine,
    prev_logical_line: Option<&LogicalLine>,
    indent_char: char,
    indent_level: usize,
    prev_indent_level: Option<usize>,
    indent_size: usize,
) -> Vec<DiagnosticKind> {
    let mut diagnostics = vec![];

    if indent_level % indent_size != 0 {
        diagnostics.push(if logical_line.is_comment_only() {
            DiagnosticKind::from(IndentationWithInvalidMultipleComment {
                indent_width: indent_size,
            })
        } else {
            DiagnosticKind::from(IndentationWithInvalidMultiple {
                indent_width: indent_size,
            })
        });
    }
    let indent_expect = prev_logical_line
        .and_then(|prev_logical_line| prev_logical_line.tokens_trimmed().last())
        .is_some_and(|t| t.kind() == TokenKind::Colon);

    if indent_expect && indent_level <= prev_indent_level.unwrap_or(0) {
        diagnostics.push(if logical_line.is_comment_only() {
            DiagnosticKind::from(NoIndentedBlockComment)
        } else {
            DiagnosticKind::from(NoIndentedBlock)
        });
    } else if !indent_expect
        && prev_indent_level.is_some_and(|prev_indent_level| indent_level > prev_indent_level)
    {
        diagnostics.push(if logical_line.is_comment_only() {
            DiagnosticKind::from(UnexpectedIndentationComment)
        } else {
            DiagnosticKind::from(UnexpectedIndentation)
        });
    }
    if indent_expect {
        let expected_indent_amount = if indent_char == '\t' { 8 } else { 4 };
        let expected_indent_level = prev_indent_level.unwrap_or(0) + expected_indent_amount;
        if indent_level > expected_indent_level {
            diagnostics.push(
                OverIndented {
                    is_comment: logical_line.is_comment_only(),
                }
                .into(),
            );
        }
    }

    diagnostics
}
