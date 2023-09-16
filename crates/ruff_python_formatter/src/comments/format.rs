use std::borrow::Cow;

use ruff_formatter::{format_args, write, FormatError, FormatOptions, SourceCode};
use ruff_python_ast::node::{AnyNodeRef, AstNode};
use ruff_python_ast::PySourceType;
use ruff_python_trivia::{lines_after, lines_after_ignoring_trivia, lines_before};
use ruff_text_size::{Ranged, TextLen, TextRange};

use crate::comments::{CommentLinePosition, SourceComment};
use crate::context::NodeLevel;
use crate::prelude::*;

/// Formats the leading comments of a node.
pub(crate) fn leading_node_comments<T>(node: &T) -> FormatLeadingComments
where
    T: AstNode,
{
    FormatLeadingComments::Node(node.as_any_node_ref())
}

/// Formats the passed comments as leading comments
pub(crate) const fn leading_comments(comments: &[SourceComment]) -> FormatLeadingComments {
    FormatLeadingComments::Comments(comments)
}

#[derive(Copy, Clone, Debug)]
pub(crate) enum FormatLeadingComments<'a> {
    Node(AnyNodeRef<'a>),
    Comments(&'a [SourceComment]),
}

impl Format<PyFormatContext<'_>> for FormatLeadingComments<'_> {
    fn fmt(&self, f: &mut PyFormatter) -> FormatResult<()> {
        fn write_leading_comments(
            comments: &[SourceComment],
            f: &mut PyFormatter,
        ) -> FormatResult<()> {
            for comment in comments.iter().filter(|comment| comment.is_unformatted()) {
                let lines_after_comment = lines_after(comment.end(), f.context().source());
                write!(
                    f,
                    [format_comment(comment), empty_lines(lines_after_comment)]
                )?;

                comment.mark_formatted();
            }

            Ok(())
        }

        match self {
            FormatLeadingComments::Node(node) => {
                let comments = f.context().comments().clone();
                write_leading_comments(comments.leading(*node), f)
            }
            FormatLeadingComments::Comments(comments) => write_leading_comments(comments, f),
        }
    }
}

/// Formats the leading `comments` of an alternate branch and ensures that it preserves the right
/// number of empty lines before. The `last_node` is the last node of the preceding body.
///
/// For example, `last_node` is the last statement in the if body when formatting the leading
/// comments of the `else` branch.
pub(crate) fn leading_alternate_branch_comments<'a, T>(
    comments: &'a [SourceComment],
    last_node: Option<T>,
) -> FormatLeadingAlternateBranchComments<'a>
where
    T: Into<AnyNodeRef<'a>>,
{
    FormatLeadingAlternateBranchComments {
        comments,
        last_node: last_node.map(Into::into),
    }
}

pub(crate) struct FormatLeadingAlternateBranchComments<'a> {
    comments: &'a [SourceComment],
    last_node: Option<AnyNodeRef<'a>>,
}

impl Format<PyFormatContext<'_>> for FormatLeadingAlternateBranchComments<'_> {
    fn fmt(&self, f: &mut PyFormatter) -> FormatResult<()> {
        if let Some(first_leading) = self.comments.first() {
            // Leading comments only preserves the lines after the comment but not before.
            // Insert the necessary lines.
            if lines_before(first_leading.start(), f.context().source()) > 1 {
                write!(f, [empty_line()])?;
            }

            write!(f, [leading_comments(self.comments)])?;
        } else if let Some(last_preceding) = self.last_node {
            // The leading comments formatting ensures that it preserves the right amount of lines after
            // We need to take care of this ourselves, if there's no leading `else` comment.
            if lines_after_ignoring_trivia(last_preceding.end(), f.context().source()) > 1 {
                write!(f, [empty_line()])?;
            }
        }

        Ok(())
    }
}

/// Formats the passed comments as trailing comments
pub(crate) fn trailing_comments(comments: &[SourceComment]) -> FormatTrailingComments {
    FormatTrailingComments(comments)
}

pub(crate) struct FormatTrailingComments<'a>(&'a [SourceComment]);

impl Format<PyFormatContext<'_>> for FormatTrailingComments<'_> {
    fn fmt(&self, f: &mut PyFormatter) -> FormatResult<()> {
        let mut has_trailing_own_line_comment = false;

        for trailing in self.0.iter().filter(|comment| comment.is_unformatted()) {
            has_trailing_own_line_comment |= trailing.line_position().is_own_line();

            if has_trailing_own_line_comment {
                let lines_before_comment = lines_before(trailing.start(), f.context().source());

                // A trailing comment at the end of a body or list
                // ```python
                // def test():
                //      pass
                //
                //      # Some comment
                // ```
                write!(
                    f,
                    [
                        line_suffix(
                            &format_args![
                                empty_lines(lines_before_comment),
                                format_comment(trailing)
                            ],
                            // Reserving width isn't necessary because we don't split
                            // comments and the empty lines expand any enclosing group.
                            0
                        ),
                        expand_parent()
                    ]
                )?;
            } else {
                // A trailing comment at the end of a line has a reserved width to
                // consider during line measurement.
                // ```python
                // tup = (
                //     "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
                // )  # Some comment
                // ```
                trailing_end_of_line_comment(trailing).fmt(f)?;
            }

            trailing.mark_formatted();
        }

        Ok(())
    }
}

/// Formats the dangling comments of `node`.
pub(crate) fn dangling_node_comments<T>(node: &T) -> FormatDanglingComments
where
    T: AstNode,
{
    FormatDanglingComments::Node(node.as_any_node_ref())
}

pub(crate) fn dangling_comments(comments: &[SourceComment]) -> FormatDanglingComments {
    FormatDanglingComments::Comments(comments)
}

pub(crate) enum FormatDanglingComments<'a> {
    Node(AnyNodeRef<'a>),
    Comments(&'a [SourceComment]),
}

impl Format<PyFormatContext<'_>> for FormatDanglingComments<'_> {
    fn fmt(&self, f: &mut Formatter<PyFormatContext>) -> FormatResult<()> {
        let comments = f.context().comments().clone();

        let dangling_comments = match self {
            Self::Comments(comments) => comments,
            Self::Node(node) => comments.dangling(*node),
        };

        let mut first = true;
        for comment in dangling_comments
            .iter()
            .filter(|comment| comment.is_unformatted())
        {
            if first {
                match comment.line_position {
                    CommentLinePosition::OwnLine => {
                        write!(f, [hard_line_break()])?;
                    }
                    CommentLinePosition::EndOfLine => {
                        write!(f, [space(), space()])?;
                    }
                }
            }

            write!(
                f,
                [
                    format_comment(comment),
                    empty_lines(lines_after(comment.end(), f.context().source()))
                ]
            )?;

            comment.mark_formatted();

            first = false;
        }

        Ok(())
    }
}

/// Formats the dangling comments within a parenthesized expression, for example:
/// ```python
/// [  # comment
///     1,
///     2,
///     3,
/// ]
/// ```
pub(crate) fn dangling_open_parenthesis_comments(
    comments: &[SourceComment],
) -> FormatDanglingOpenParenthesisComments {
    FormatDanglingOpenParenthesisComments { comments }
}

pub(crate) struct FormatDanglingOpenParenthesisComments<'a> {
    comments: &'a [SourceComment],
}

impl Format<PyFormatContext<'_>> for FormatDanglingOpenParenthesisComments<'_> {
    fn fmt(&self, f: &mut Formatter<PyFormatContext>) -> FormatResult<()> {
        for comment in self
            .comments
            .iter()
            .filter(|comment| comment.is_unformatted())
        {
            debug_assert!(
                comment.line_position().is_end_of_line(),
                "Expected dangling comment to be at the end of the line"
            );

            trailing_end_of_line_comment(comment).fmt(f)?;
            comment.mark_formatted();
        }

        Ok(())
    }
}

/// Formats the content of the passed comment.
///
/// * Adds a whitespace between `#` and the comment text except if the first character is a `#`, `:`, `'`, or `!`
/// * Replaces non breaking whitespaces with regular whitespaces except if in front of a `types:` comment
pub(crate) const fn format_comment(comment: &SourceComment) -> FormatComment {
    FormatComment { comment }
}

pub(crate) struct FormatComment<'a> {
    comment: &'a SourceComment,
}

impl Format<PyFormatContext<'_>> for FormatComment<'_> {
    fn fmt(&self, f: &mut PyFormatter) -> FormatResult<()> {
        let slice = self.comment.slice();
        let source = SourceCode::new(f.context().source());

        let normalized_comment = normalize_comment(self.comment, source)?;

        format_normalized_comment(normalized_comment, slice.range()).fmt(f)
    }
}

/// Helper that inserts the appropriate number of empty lines before a comment, depending on the node level:
/// - Top-level: Up to two empty lines.
/// - Parenthesized: A single empty line.
/// - Otherwise: Up to a single empty line.
pub(crate) const fn empty_lines(lines: u32) -> FormatEmptyLines {
    FormatEmptyLines { lines }
}

#[derive(Copy, Clone, Debug)]
pub(crate) struct FormatEmptyLines {
    lines: u32,
}

impl Format<PyFormatContext<'_>> for FormatEmptyLines {
    fn fmt(&self, f: &mut Formatter<PyFormatContext>) -> FormatResult<()> {
        match f.context().node_level() {
            NodeLevel::TopLevel => match self.lines {
                0 | 1 => write!(f, [hard_line_break()]),
                2 => write!(f, [empty_line()]),
                _ => write!(f, [empty_line(), empty_line()]),
            },

            NodeLevel::CompoundStatement => match self.lines {
                0 | 1 => write!(f, [hard_line_break()]),
                _ => write!(f, [empty_line()]),
            },

            // Remove all whitespace in parenthesized expressions
            NodeLevel::Expression(_) | NodeLevel::ParenthesizedExpression => {
                write!(f, [hard_line_break()])
            }
        }
    }
}

/// A helper that constructs a formattable element using a reserved-width line-suffix
/// for normalized comments.
///
/// * Black normalization of `SourceComment`.
/// * Line suffix with reserved width for the final, normalized content.
/// * Expands parent node.
pub(crate) const fn trailing_end_of_line_comment(
    comment: &SourceComment,
) -> FormatTrailingEndOfLineComment {
    FormatTrailingEndOfLineComment { comment }
}

pub(crate) struct FormatTrailingEndOfLineComment<'a> {
    comment: &'a SourceComment,
}

impl Format<PyFormatContext<'_>> for FormatTrailingEndOfLineComment<'_> {
    fn fmt(&self, f: &mut PyFormatter) -> FormatResult<()> {
        let slice = self.comment.slice();
        let source = SourceCode::new(f.context().source());

        let normalized_comment = normalize_comment(self.comment, source)?;

        // Trim the normalized comment to detect excluded pragmas (strips NBSP).
        let trimmed = strip_comment_prefix(&normalized_comment)?.trim_start();

        let is_pragma = if let Some((maybe_pragma, _)) = trimmed.split_once(':') {
            matches!(maybe_pragma, "noqa" | "type" | "pyright" | "pylint")
        } else {
            trimmed.starts_with("noqa")
        };

        // Don't reserve width for excluded pragma comments.
        let reserved_width = if is_pragma {
            0
        } else {
            // Start with 2 because of the two leading spaces.
            let width = 2u32.saturating_add(
                TextWidth::from_text(&normalized_comment, f.options().indent_width())
                    .width()
                    .expect("Expected comment not to contain any newlines")
                    .value(),
            );

            width
        };

        write!(
            f,
            [
                line_suffix(
                    &format_args![
                        space(),
                        space(),
                        format_normalized_comment(normalized_comment, slice.range())
                    ],
                    reserved_width
                ),
                expand_parent()
            ]
        )
    }
}

/// A helper that constructs formattable normalized comment text as efficiently as
/// possible.
///
/// * If the content is unaltered then format with source text slice strategy and no
///   unnecessary allocations.
/// * If the content is modified then make as few allocations as possible and use
///   a dynamic text element at the original slice's start position.
pub(crate) const fn format_normalized_comment(
    comment: Cow<'_, str>,
    range: TextRange,
) -> FormatNormalizedComment<'_> {
    FormatNormalizedComment { comment, range }
}

pub(crate) struct FormatNormalizedComment<'a> {
    comment: Cow<'a, str>,
    range: TextRange,
}

impl Format<PyFormatContext<'_>> for FormatNormalizedComment<'_> {
    fn fmt(&self, f: &mut Formatter<PyFormatContext>) -> FormatResult<()> {
        match self.comment {
            Cow::Borrowed(borrowed) => {
                source_text_slice(TextRange::at(self.range.start(), borrowed.text_len())).fmt(f)
            }

            Cow::Owned(ref owned) => {
                write!(
                    f,
                    [
                        text(owned, Some(self.range.start())),
                        source_position(self.range.end())
                    ]
                )
            }
        }
    }
}

/// A helper for normalizing comments efficiently.
///
/// * Return as fast as possible without making unnecessary allocations.
/// * Trim any trailing whitespace.
/// * Normalize for a leading '# '.
/// * Retain non-breaking spaces for 'type:' pragmas by leading with '# \u{A0}'.
fn normalize_comment<'a>(
    comment: &'a SourceComment,
    source: SourceCode<'a>,
) -> FormatResult<Cow<'a, str>> {
    let slice = comment.slice();
    let comment_text = slice.text(source);

    let trimmed = comment_text.trim_end();

    let content = strip_comment_prefix(trimmed)?;

    if content.is_empty() {
        return Ok(Cow::Borrowed("#"));
    }

    // Fast path for correctly formatted comments:
    // * Start with a `# '.
    // * Have no trailing whitespace.
    if content.starts_with([' ', '!', ':', '#', '\'']) {
        return Ok(Cow::Borrowed(trimmed));
    }

    if content.starts_with('\u{A0}') {
        let trimmed = content.trim_start_matches('\u{A0}');

        if trimmed.trim_start().starts_with("type:") {
            // Black adds a space before the non-breaking space if part of a type pragma.
            Ok(Cow::Owned(std::format!("# {content}")))
        } else if trimmed.starts_with(' ') {
            // Black replaces the non-breaking space with a space if followed by a space.
            Ok(Cow::Owned(std::format!("# {trimmed}")))
        } else {
            // Otherwise we replace the first non-breaking space with a regular space.
            Ok(Cow::Owned(std::format!("# {}", &content["\u{A0}".len()..])))
        }
    } else {
        Ok(Cow::Owned(std::format!("# {}", content.trim_start())))
    }
}

/// A helper for stripping '#' from comments.
fn strip_comment_prefix(comment_text: &str) -> FormatResult<&str> {
    let Some(content) = comment_text.strip_prefix('#') else {
        return Err(FormatError::syntax_error(
            "Didn't find expected comment token `#`",
        ));
    };

    Ok(content)
}

/// Format the empty lines between a node and its trailing comments.
///
/// For example, given:
/// ```python
/// def func():
///     ...
/// # comment
/// ```
///
/// This builder will insert two empty lines before the comment.
/// ```
pub(crate) fn empty_lines_before_trailing_comments<'a>(
    f: &PyFormatter,
    comments: &'a [SourceComment],
) -> FormatEmptyLinesBeforeTrailingComments<'a> {
    // Black has different rules for stub vs. non-stub and top level vs. indented
    let empty_lines = match (f.options().source_type(), f.context().node_level()) {
        (PySourceType::Stub, NodeLevel::TopLevel) => 1,
        (PySourceType::Stub, _) => 0,
        (_, NodeLevel::TopLevel) => 2,
        (_, _) => 1,
    };

    FormatEmptyLinesBeforeTrailingComments {
        comments,
        empty_lines,
    }
}

#[derive(Copy, Clone, Debug)]
pub(crate) struct FormatEmptyLinesBeforeTrailingComments<'a> {
    /// The trailing comments of the node.
    comments: &'a [SourceComment],
    /// The expected number of empty lines before the trailing comments.
    empty_lines: u32,
}

impl Format<PyFormatContext<'_>> for FormatEmptyLinesBeforeTrailingComments<'_> {
    fn fmt(&self, f: &mut Formatter<PyFormatContext>) -> FormatResult<()> {
        if let Some(comment) = self
            .comments
            .iter()
            .find(|comment| comment.line_position().is_own_line())
        {
            let actual = lines_before(comment.start(), f.context().source()).saturating_sub(1);
            for _ in actual..self.empty_lines {
                write!(f, [empty_line()])?;
            }
        }
        Ok(())
    }
}
