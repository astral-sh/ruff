use ruff_text_size::{TextLen, TextRange, TextSize};
use rustpython_parser::ast::Ranged;

use ruff_formatter::{format_args, write, FormatError, SourceCode};
use ruff_python_ast::node::{AnyNodeRef, AstNode};

use crate::comments::SourceComment;
use crate::context::NodeLevel;
use crate::prelude::*;
use crate::trivia::{lines_after, lines_before, skip_trailing_trivia};

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
            FormatLeadingComments::Node(node) => comments.leading_comments(*node),
            FormatLeadingComments::Comments(comments) => comments,
        };

        for comment in leading_comments
            .iter()
            .filter(|comment| comment.is_unformatted())
        {
            let slice = comment.slice();

            let lines_after_comment = lines_after(slice.end(), f.context().contents());
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
    fn fmt(&self, f: &mut Formatter<PyFormatContext<'_>>) -> FormatResult<()> {
        if let Some(first_leading) = self.comments.first() {
            // Leading comments only preserves the lines after the comment but not before.
            // Insert the necessary lines.
            if lines_before(first_leading.slice().start(), f.context().contents()) > 1 {
                write!(f, [empty_line()])?;
            }

            write!(f, [leading_comments(self.comments)])?;
        } else if let Some(last_preceding) = self.last_node {
            let full_end = skip_trailing_trivia(last_preceding.end(), f.context().contents());
            // The leading comments formatting ensures that it preserves the right amount of lines after
            // We need to take care of this ourselves, if there's no leading `else` comment.
            if lines_after(full_end, f.context().contents()) > 1 {
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
    fn fmt(&self, f: &mut Formatter<PyFormatContext<'_>>) -> FormatResult<()> {
        let comments = f.context().comments().clone();

        let trailing_comments = match self {
            FormatTrailingComments::Node(node) => comments.trailing_comments(*node),
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
                let lines_before_comment = lines_before(slice.start(), f.context().contents());

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
                        line_suffix(&format_with(|f| {
                            write!(
                                f,
                                [empty_lines(lines_before_comment), format_comment(trailing)]
                            )
                        })),
                        expand_parent()
                    ]
                )?;
            } else {
                write!(
                    f,
                    [
                        line_suffix(&format_args![space(), space(), format_comment(trailing)]),
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
            Self::Node(node) => comments.dangling_comments(*node),
        };

        let mut first = true;
        for comment in dangling_comments
            .iter()
            .filter(|comment| comment.is_unformatted())
        {
            if first && comment.line_position().is_end_of_line() {
                write!(f, [space(), space()])?;
            }

            write!(
                f,
                [
                    format_comment(comment),
                    empty_lines(lines_after(comment.slice().end(), f.context().contents()))
                ]
            )?;

            comment.mark_formatted();

            first = false;
        }

        Ok(())
    }
}

/// Formats the content of the passed comment.
///
/// * Adds a whitespace between `#` and the comment text except if the first character is a `#`, `:`, `'`, or `!`
/// * Replaces non breaking whitespaces with regular whitespaces except if in front of a `types:` comment
const fn format_comment(comment: &SourceComment) -> FormatComment {
    FormatComment { comment }
}

struct FormatComment<'a> {
    comment: &'a SourceComment,
}

impl Format<PyFormatContext<'_>> for FormatComment<'_> {
    fn fmt(&self, f: &mut Formatter<PyFormatContext<'_>>) -> FormatResult<()> {
        let slice = self.comment.slice();
        let comment_text = slice.text(SourceCode::new(f.context().contents()));

        let trimmed = comment_text.trim_end();
        let trailing_whitespace_len = comment_text.text_len() - trimmed.text_len();

        let Some(content) = trimmed.strip_prefix('#') else {
            return Err(FormatError::SyntaxError);
        };

        // Fast path for correctly formatted comments:
        // * Start with a `#` and are followed by a space
        // * Have no trailing whitespace.
        if trailing_whitespace_len == TextSize::new(0) && content.starts_with(' ') {
            return source_text_slice(slice.range(), ContainsNewlines::No).fmt(f);
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
        }

        let start = slice.start() + start_offset;
        let end = slice.range().end() - trailing_whitespace_len;

        write!(
            f,
            [
                source_text_slice(TextRange::new(start, end), ContainsNewlines::No),
                source_position(slice.end())
            ]
        )
    }
}

// Helper that inserts the appropriate number of empty lines before a comment, depending on the node level.
// Top level: Up to two empty lines
// parenthesized: A single empty line
// other: Up to a single empty line
const fn empty_lines(lines: u32) -> FormatEmptyLines {
    FormatEmptyLines { lines }
}

#[derive(Copy, Clone, Debug)]
struct FormatEmptyLines {
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
            NodeLevel::Expression => write!(f, [hard_line_break()]),
        }
    }
}
