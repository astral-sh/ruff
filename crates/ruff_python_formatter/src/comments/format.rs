use ruff_text_size::{Ranged, TextLen, TextRange, TextSize};

use ruff_formatter::{format_args, write, FormatError, FormatState, SourceCode, VecBuffer};
use ruff_python_ast::node::{AnyNodeRef, AstNode};
use ruff_python_trivia::{lines_after, lines_after_ignoring_trivia, lines_before};

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
        let comments = f.context().comments().clone();

        let leading_comments = match self {
            FormatLeadingComments::Node(node) => comments.leading(*node),
            FormatLeadingComments::Comments(comments) => comments,
        };

        for comment in leading_comments
            .iter()
            .filter(|comment| comment.is_unformatted())
        {
            let slice = comment.slice();

            let lines_after_comment = lines_after(slice.end(), f.context().source());
            write!(
                f,
                [format_comment(comment), empty_lines(lines_after_comment)]
            )?;

            comment.mark_formatted();
        }

        Ok(())
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
            if lines_before(first_leading.slice().start(), f.context().source()) > 1 {
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

/// Formats the trailing comments of `node`
pub(crate) fn trailing_node_comments<T>(node: &T) -> FormatTrailingComments
where
    T: AstNode,
{
    FormatTrailingComments::Node(node.as_any_node_ref())
}

/// Formats the passed comments as trailing comments
pub(crate) fn trailing_comments(comments: &[SourceComment]) -> FormatTrailingComments {
    FormatTrailingComments::Comments(comments)
}

pub(crate) enum FormatTrailingComments<'a> {
    Node(AnyNodeRef<'a>),
    Comments(&'a [SourceComment]),
}

impl Format<PyFormatContext<'_>> for FormatTrailingComments<'_> {
    fn fmt(&self, f: &mut PyFormatter) -> FormatResult<()> {
        let comments = f.context().comments().clone();

        let trailing_comments = match self {
            FormatTrailingComments::Node(node) => comments.trailing(*node),
            FormatTrailingComments::Comments(comments) => comments,
        };

        let mut has_trailing_own_line_comment = false;

        for trailing in trailing_comments
            .iter()
            .filter(|comment| comment.is_unformatted())
        {
            let slice = trailing.slice();

            has_trailing_own_line_comment |= trailing.line_position().is_own_line();

            if has_trailing_own_line_comment {
                let lines_before_comment = lines_before(slice.start(), f.context().source());

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
                            0 // Reserving width isn't necessary because we don't split comments and the empty lines expand any enclosing group.
                        ),
                        expand_parent()
                    ]
                )?;
            } else {
                // A trailing comment at the end of a line has a reserved width to consider during line measurement.
                // ```python
                // tup = (
                //     "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
                // )  # Some comment
                // ```
                write!(
                    f,
                    [
                        line_suffix(
                            &format_args![space(), space(), format_comment(trailing)],
                            measure_comment(trailing, f.context())? + 2 // Account for two added spaces
                        ),
                        expand_parent()
                    ]
                )?;
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
                    empty_lines(lines_after(comment.slice().end(), f.context().source()))
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

            write!(
                f,
                [
                    line_suffix(
                        &format_args!(space(), space(), format_comment(comment)),
                        // Marking the comment as a line suffix with reserved width is safe since we expect the comment to be end of line.
                        measure_comment(comment, f.context())? + 2 // Account for two added spaces
                    ),
                    expand_parent()
                ]
            )?;
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
        // We don't need the formatted comment's width.
        let _ = write_comment(f, self.comment)?;
        Ok(())
    }
}

// Helper that inserts the appropriate number of empty lines before a comment, depending on the node level.
// Top level: Up to two empty lines
// parenthesized: A single empty line
// other: Up to a single empty line
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

/// A helper used to measure formatted comments.
///
/// Use a temporary formatter to write a normalized, formatted comment
/// to in order to compute its width for a reserved-width line suffix element.
fn measure_comment(comment: &SourceComment, context: &PyFormatContext) -> FormatResult<u32> {
    let mut state = FormatState::new(context.clone());
    let mut buffer = VecBuffer::new(&mut state);
    let comment_len = write_comment(&mut Formatter::new(&mut buffer), comment)?;
    Ok(comment_len)
}

/// Write a comment to a formatter and return the normalized comment's width.
fn write_comment(f: &mut PyFormatter, comment: &SourceComment) -> FormatResult<u32> {
    let slice = comment.slice();
    let comment_text = slice.text(SourceCode::new(f.context().source()));

    // Track any additional width the formatted comment will have after normalization.
    let mut added_width = TextSize::new(0);

    let trimmed = comment_text.trim_end();
    let trailing_whitespace_len = comment_text.text_len() - trimmed.text_len();

    let Some(content) = trimmed.strip_prefix('#') else {
        return Err(FormatError::syntax_error(
            "Didn't find expected comment token `#`",
        ));
    };

    // Fast path for correctly formatted comments:
    // * Start with a `#` and are followed by a space
    // * Have no trailing whitespace.
    if trailing_whitespace_len == TextSize::new(0) && content.starts_with(' ') {
        source_text_slice(slice.range(), ContainsNewlines::No).fmt(f)?;
        return Ok(slice.range().len().into());
    }

    write!(f, [source_position(slice.start()), text("#")])?;

    // Starts with a non breaking space
    let start_offset =
        if content.starts_with('\u{A0}') && !content.trim_start().starts_with("type:") {
            // Replace non-breaking space with a space (if not followed by a normal space)
            "#\u{A0}".text_len()
        } else {
            '#'.text_len()
        };

    // Add a space between the `#` and the text if the source contains none.
    if !content.is_empty() && !content.starts_with([' ', '!', ':', '#', '\'']) {
        write!(f, [space()])?;
        added_width += TextSize::new(1);
    }

    let start = slice.start() + start_offset;
    let end = slice.end() - trailing_whitespace_len;

    write!(
        f,
        [
            source_text_slice(TextRange::new(start, end), ContainsNewlines::No),
            source_position(slice.end())
        ]
    )?;

    Ok((end - slice.start() + added_width).into())
}
