use std::borrow::Cow;

use ruff_formatter::{format_args, write, FormatError, FormatOptions, SourceCode};
use ruff_python_ast::{AnyNodeRef, AstNode, NodeKind, PySourceType};
use ruff_python_trivia::{
    is_pragma_comment, lines_after, lines_after_ignoring_trivia, lines_before, CommentLinePosition,
};
use ruff_text_size::{Ranged, TextLen, TextRange};

use crate::comments::SourceComment;
use crate::context::NodeLevel;
use crate::prelude::*;
use crate::statement::suite::should_insert_blank_line_after_class_in_stub_file;

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
        if self.last_node.map_or(false, |preceding| {
            should_insert_blank_line_after_class_in_stub_file(preceding, None, f.context())
        }) {
            write!(f, [empty_line(), leading_comments(self.comments)])?;
        } else if let Some(first_leading) = self.comments.first() {
            // Leading comments only preserves the lines after the comment but not before.
            // Insert the necessary lines.
            write!(
                f,
                [empty_lines(lines_before(
                    first_leading.start(),
                    f.context().source()
                ))]
            )?;

            write!(f, [leading_comments(self.comments)])?;
        } else if let Some(last_preceding) = self.last_node {
            // The leading comments formatting ensures that it preserves the right amount of lines
            // after We need to take care of this ourselves, if there's no leading `else` comment.
            // Since the `last_node` could be a compound node, we need to skip _all_ trivia.
            //
            // For example, here, when formatting the `if` statement, the `last_node` (the `while`)
            // would end at the end of `pass`, but we want to skip _all_ comments:
            // ```python
            // if True:
            //     while True:
            //         pass
            //         # comment
            //
            //     # comment
            // else:
            //     ...
            // ```
            //
            // `lines_after_ignoring_trivia` is safe here, as we _know_ that the `else` doesn't
            // have any leading comments.
            write!(
                f,
                [empty_lines(lines_after_ignoring_trivia(
                    last_preceding.end(),
                    f.context().source()
                ))]
            )?;
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
                                format_comment(trailing),
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
            NodeLevel::TopLevel(_) => match self.lines {
                0 | 1 => write!(f, [hard_line_break()]),
                2 => write!(f, [empty_line()]),
                _ => match f.options().source_type() {
                    PySourceType::Stub => {
                        write!(f, [empty_line()])
                    }
                    PySourceType::Python | PySourceType::Ipynb => {
                        write!(f, [empty_line(), empty_line()])
                    }
                },
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

        // Don't reserve width for excluded pragma comments.
        let reserved_width = if is_pragma_comment(&normalized_comment) {
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
        let write_sourcemap = f.options().source_map_generation().is_enabled();

        write_sourcemap
            .then_some(source_position(self.range.start()))
            .fmt(f)?;

        match self.comment {
            Cow::Borrowed(borrowed) => {
                source_text_slice(TextRange::at(self.range.start(), borrowed.text_len())).fmt(f)?;
            }

            Cow::Owned(ref owned) => {
                text(owned).fmt(f)?;
            }
        }

        write_sourcemap
            .then_some(source_position(self.range.end()))
            .fmt(f)
    }
}

/// A helper for normalizing comments by:
/// * Trimming any trailing whitespace.
/// * Adding a leading space after the `#`, if necessary.
///
/// For example:
/// * `#comment` is normalized to `# comment`.
/// * `# comment ` is normalized to `# comment`.
/// * `#  comment` is left as-is.
/// * `#!comment` is left as-is.
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

    // Fast path for correctly formatted comments: if the comment starts with a space, or any
    // of the allowed characters, then it's included verbatim (apart for trimming any trailing
    // whitespace).
    if content.starts_with([' ', '!', ':', '#', '\'']) {
        return Ok(Cow::Borrowed(trimmed));
    }

    // Otherwise, we need to normalize the comment by adding a space after the `#`.
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
/// class Class:
///     ...
/// # comment
/// ```
///
/// This builder will insert two empty lines before the comment.
///
/// # Preview
///
/// For preview style, this builder will insert a single empty line after a
/// class definition in a stub file.
///
/// For example, given:
/// ```python
/// class Foo:
///     pass
/// # comment
/// ```
///
/// This builder will insert a single empty line before the comment.
pub(crate) fn empty_lines_before_trailing_comments(
    comments: &[SourceComment],
    node_kind: NodeKind,
) -> FormatEmptyLinesBeforeTrailingComments {
    FormatEmptyLinesBeforeTrailingComments {
        comments,
        node_kind,
    }
}

#[derive(Copy, Clone, Debug)]
pub(crate) struct FormatEmptyLinesBeforeTrailingComments<'a> {
    /// The trailing comments of the node.
    comments: &'a [SourceComment],
    node_kind: NodeKind,
}

impl Format<PyFormatContext<'_>> for FormatEmptyLinesBeforeTrailingComments<'_> {
    fn fmt(&self, f: &mut Formatter<PyFormatContext>) -> FormatResult<()> {
        if let Some(comment) = self
            .comments
            .iter()
            .find(|comment| comment.line_position().is_own_line())
        {
            // Black has different rules for stub vs. non-stub and top level vs. indented
            let empty_lines = match (f.options().source_type(), f.context().node_level()) {
                (PySourceType::Stub, NodeLevel::TopLevel(_)) => 1,
                (PySourceType::Stub, _) => u32::from(self.node_kind == NodeKind::StmtClassDef),
                (_, NodeLevel::TopLevel(_)) => 2,
                (_, _) => 1,
            };

            let actual = lines_before(comment.start(), f.context().source()).saturating_sub(1);
            for _ in actual..empty_lines {
                empty_line().fmt(f)?;
            }
        }
        Ok(())
    }
}

/// Format the empty lines between a node and its leading comments.
///
/// For example, given:
/// ```python
/// # comment
///
/// class Class:
///     ...
/// ```
///
/// While `leading_comments` will preserve the existing empty line, this builder will insert an
/// additional empty line before the comment.
pub(crate) fn empty_lines_after_leading_comments(
    comments: &[SourceComment],
) -> FormatEmptyLinesAfterLeadingComments {
    FormatEmptyLinesAfterLeadingComments { comments }
}

#[derive(Copy, Clone, Debug)]
pub(crate) struct FormatEmptyLinesAfterLeadingComments<'a> {
    /// The leading comments of the node.
    comments: &'a [SourceComment],
}

impl Format<PyFormatContext<'_>> for FormatEmptyLinesAfterLeadingComments<'_> {
    fn fmt(&self, f: &mut Formatter<PyFormatContext>) -> FormatResult<()> {
        if let Some(comment) = self
            .comments
            .iter()
            .rev()
            .find(|comment| comment.line_position().is_own_line())
        {
            // Black has different rules for stub vs. non-stub and top level vs. indented
            let empty_lines = match (f.options().source_type(), f.context().node_level()) {
                (PySourceType::Stub, NodeLevel::TopLevel(_)) => 1,
                (PySourceType::Stub, _) => 0,
                (_, NodeLevel::TopLevel(_)) => 2,
                (_, _) => 1,
            };

            let actual = lines_after(comment.end(), f.context().source()).saturating_sub(1);
            // If there are no empty lines, keep the comment tight to the node.
            if actual == 0 {
                return Ok(());
            }

            // If there are more than enough empty lines already, `leading_comments` will
            // trim them as necessary.
            if actual >= empty_lines {
                return Ok(());
            }

            for _ in actual..empty_lines {
                empty_line().fmt(f)?;
            }
        }
        Ok(())
    }
}
