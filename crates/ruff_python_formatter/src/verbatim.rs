use std::iter::FusedIterator;

use ruff_formatter::write;
use ruff_python_ast::node::AnyNodeRef;
use ruff_python_ast::{Ranged, Stmt};
use ruff_python_trivia::lines_before;
use ruff_text_size::TextRange;

use crate::comments::format::{empty_lines, format_comment};
use crate::comments::{leading_comments, trailing_comments, SourceComment};
use crate::prelude::*;
use crate::statement::suite::SuiteChildStatement;
use crate::verbatim_text;

/// Disables formatting for all statements between the `first_suppressed` that has a leading `fmt: off` comment
/// and the first trailing or leading `fmt: on` comment. The statements are formatted as they appear in the source code.
///
/// Returns the last formatted statement.
///
/// ## Panics
/// If `first_suppressed` has no leading suppression comment.
#[cold]
pub(crate) fn write_suppressed_statements_starting_with_leading_comment<'a>(
    // The first suppressed statement
    first_suppressed: SuiteChildStatement<'a>,
    statements: &mut std::slice::Iter<'a, Stmt>,
    f: &mut PyFormatter,
) -> FormatResult<&'a Stmt> {
    let comments = f.context().comments().clone();
    let source = f.context().source();

    let mut leading_comment_ranges =
        CommentRangeIter::outside_suppression(comments.leading_comments(first_suppressed), source);

    let before_format_off = leading_comment_ranges
        .next()
        .expect("Suppressed node to have leading comments");

    let (formatted_comments, format_off_comment) = before_format_off.unwrap_suppression_starts();

    // Format the leading comments before the fmt off
    write!(
        f,
        [
            leading_comments(formatted_comments),
            // Format the off comment without adding any trailing new lines
            format_comment(format_off_comment)
        ]
    )?;

    format_off_comment.mark_formatted();

    // Now inside a suppressed range
    write_suppressed_statements(
        format_off_comment,
        first_suppressed,
        leading_comment_ranges.as_slice(),
        statements,
        f,
    )
}

/// Disables formatting for all statements between the `last_formatted` and the first trailing or leading `fmt: on` comment.
/// The statements are formatted as they appear in the source code.
///
/// Returns the last formatted statement.
///
/// ## Panics
/// If `last_formatted` has no trailing suppression comment.
#[cold]
pub(crate) fn write_suppressed_statements_starting_with_trailing_comment<'a>(
    last_formatted: SuiteChildStatement<'a>,
    statements: &mut std::slice::Iter<'a, Stmt>,
    f: &mut PyFormatter,
) -> FormatResult<&'a Stmt> {
    let comments = f.context().comments().clone();
    let source = f.context().source();

    let trailing_node_comments = comments.trailing_comments(last_formatted);
    let mut trailing_comment_ranges =
        CommentRangeIter::outside_suppression(trailing_node_comments, source);

    // Formatted comments gets formatted as part of the statement.
    let (_, mut format_off_comment) = trailing_comment_ranges
        .next()
        .expect("Suppressed statement to have trailing comments")
        .unwrap_suppression_starts();

    let maybe_suppressed = trailing_comment_ranges.as_slice();

    // Mark them as formatted so that calling the node's formatting doesn't format the comments.
    for comment in maybe_suppressed {
        comment.mark_formatted();
    }
    format_off_comment.mark_formatted();

    // Format the leading comments, the node, and the trailing comments up to the `fmt: off` comment.
    last_formatted.fmt(f)?;

    format_off_comment.mark_unformatted();
    TrailingFormatOffComment(format_off_comment).fmt(f)?;

    for range in trailing_comment_ranges {
        match range {
            // A `fmt: off`..`fmt: on` sequence. Disable formatting for the in-between comments and
            SuppressionComments::SuppressionEnds {
                suppressed_comments: _,
                format_on_comment,
                formatted_comments,
                format_off_comment: new_format_off_comment,
            } => {
                format_on_comment.mark_unformatted();

                for comment in formatted_comments {
                    comment.mark_unformatted();
                }

                write!(
                    f,
                    [
                        verbatim_text(TextRange::new(
                            format_off_comment.end(),
                            format_on_comment.start(),
                        )),
                        trailing_comments(std::slice::from_ref(format_on_comment)),
                        trailing_comments(formatted_comments),
                    ]
                )?;

                // `fmt: off`..`fmt:on`..`fmt:off` sequence
                if let Some(new_format_off_comment) = new_format_off_comment {
                    new_format_off_comment.mark_unformatted();

                    TrailingFormatOffComment(new_format_off_comment).fmt(f)?;

                    format_off_comment = new_format_off_comment;
                } else {
                    // `fmt: off`..`fmt:on` sequence. The suppression ends here. Start formatting the nodes again.
                    return Ok(last_formatted.statement());
                }
            }

            // All comments in this range are suppressed
            SuppressionComments::Suppressed { comments: _ } => {}
            // SAFETY: Unreachable because the function returns as soon as we reach the end of the suppressed range
            SuppressionComments::SuppressionStarts { .. }
            | SuppressionComments::Formatted { .. } => unreachable!(),
        }
    }

    if let Some(first_suppressed) = statements.next() {
        write_suppressed_statements(
            format_off_comment,
            SuiteChildStatement::Other(first_suppressed),
            comments.leading_comments(first_suppressed),
            statements,
            f,
        )
    } else if let Some(last_comment) = trailing_node_comments.last() {
        verbatim_text(TextRange::new(format_off_comment.end(), last_comment.end())).fmt(f)?;
        Ok(last_formatted.statement())
    } else {
        Ok(last_formatted.statement())
    }
}

/// Formats the statements from `first_suppressed` until the suppression ends (by a `fmt: on` comment)
/// as they appear in the source code.
fn write_suppressed_statements<'a>(
    // The `fmt: off` comment that starts the suppressed range. Can be a leading comment of `first_suppressed` or
    // a trailing comment of the previous node.
    format_off_comment: &SourceComment,
    // The first suppressed statement
    first_suppressed: SuiteChildStatement<'a>,
    // The leading comments of `first_suppressed` that come after the `format_off_comment`
    first_suppressed_leading_comments: &[SourceComment],
    // The remaining statements
    statements: &mut std::slice::Iter<'a, Stmt>,
    f: &mut PyFormatter,
) -> FormatResult<&'a Stmt> {
    let comments = f.context().comments().clone();
    let source = f.context().source();

    // TODO(micha) Fixup indent
    let mut statement = first_suppressed;
    let mut leading_node_comments = first_suppressed_leading_comments;
    let mut format_off_comment = format_off_comment;

    loop {
        for range in CommentRangeIter::in_suppression(leading_node_comments, source) {
            match range {
                // All leading comments are suppressed
                SuppressionComments::Suppressed { comments } => {
                    for comment in comments {
                        comment.mark_formatted();
                    }
                }

                // Node has a leading `fmt: on` comment and maybe another `fmt: off` comment
                SuppressionComments::SuppressionEnds {
                    suppressed_comments,
                    format_on_comment,
                    formatted_comments,
                    format_off_comment: new_format_off_comment,
                } => {
                    for comment in suppressed_comments {
                        comment.mark_formatted();
                    }

                    write!(
                        f,
                        [
                            verbatim_text(TextRange::new(
                                format_off_comment.end(),
                                format_on_comment.start(),
                            )),
                            leading_comments(std::slice::from_ref(format_on_comment)),
                            leading_comments(formatted_comments),
                        ]
                    )?;

                    if let Some(new_format_off_comment) = new_format_off_comment {
                        format_off_comment = new_format_off_comment;
                        format_comment(format_off_comment).fmt(f)?;
                        format_off_comment.mark_formatted();
                    } else {
                        // Suppression ends here. Test if the node has a trailing suppression comment and, if so,
                        // recurse and format the trailing comments and the following statements as suppressed.
                        return if comments
                            .trailing_comments(statement)
                            .iter()
                            .any(|comment| comment.is_suppression_off_comment(source))
                        {
                            // Node has a trailing suppression comment, hell yeah, start all over again.
                            write_suppressed_statements_starting_with_trailing_comment(
                                statement, statements, f,
                            )
                        } else {
                            // Formats the trailing comments
                            statement.fmt(f)?;
                            Ok(statement.statement())
                        };
                    }
                }

                // Unreachable because the function exits as soon as it reaches the end of the suppression
                // and it already starts in a suppressed range.
                SuppressionComments::SuppressionStarts { .. } => unreachable!(),
                SuppressionComments::Formatted { .. } => unreachable!(),
            }
        }

        comments.mark_verbatim_node_comments_formatted(AnyNodeRef::from(statement));

        for range in CommentRangeIter::in_suppression(comments.trailing_comments(statement), source)
        {
            match range {
                SuppressionComments::Suppressed { comments } => {
                    for comment in comments {
                        comment.mark_formatted();
                    }
                }

                SuppressionComments::SuppressionEnds {
                    suppressed_comments,
                    format_on_comment,
                    formatted_comments,
                    format_off_comment: new_format_off_comment,
                } => {
                    for comment in suppressed_comments {
                        comment.mark_formatted();
                    }

                    write!(
                        f,
                        [
                            verbatim_text(TextRange::new(
                                format_off_comment.end(),
                                format_on_comment.start()
                            )),
                            format_comment(format_on_comment),
                            hard_line_break(),
                            trailing_comments(formatted_comments),
                        ]
                    )?;

                    format_on_comment.mark_formatted();

                    if let Some(new_format_off_comment) = new_format_off_comment {
                        format_off_comment = new_format_off_comment;
                        format_comment(format_off_comment).fmt(f)?;
                        format_off_comment.mark_formatted();
                    } else {
                        return Ok(statement.statement());
                    }
                }

                // Unreachable because the function exits as soon as it reaches the end of the suppression
                // and it already starts in a suppressed range.
                SuppressionComments::SuppressionStarts { .. } => unreachable!(),
                SuppressionComments::Formatted { .. } => unreachable!(),
            }
        }

        if let Some(next_statement) = statements.next() {
            statement = SuiteChildStatement::Other(next_statement);
            leading_node_comments = comments.leading_comments(next_statement);
        } else {
            let end = comments
                .trailing_comments(statement)
                .last()
                .map_or(statement.end(), Ranged::end);

            verbatim_text(TextRange::new(format_off_comment.end(), end)).fmt(f)?;

            return Ok(statement.statement());
        }
    }
}

#[derive(Copy, Clone, Debug)]
enum InSuppression {
    No,
    Yes,
}

#[derive(Debug)]
enum SuppressionComments<'a> {
    /// The first `fmt: off` comment.
    SuppressionStarts {
        /// The comments appearing before the `fmt: off` comment
        formatted_comments: &'a [SourceComment],
        format_off_comment: &'a SourceComment,
    },

    /// A `fmt: on` comment inside a suppressed range.
    SuppressionEnds {
        /// The comments before the `fmt: on` comment that should *not* be formatted.
        suppressed_comments: &'a [SourceComment],
        format_on_comment: &'a SourceComment,

        /// The comments after the `fmt: on` comment (if any), that should be formatted.
        formatted_comments: &'a [SourceComment],

        /// Any following `fmt: off` comment if any.
        /// * `None`: The suppression ends here (for good)
        /// * `Some`: A `fmt: off`..`fmt: on` .. `fmt: off` sequence. The suppression continues after
        ///     the `fmt: off` comment.
        format_off_comment: Option<&'a SourceComment>,
    },

    /// Comments that all fall into the suppressed range.
    Suppressed { comments: &'a [SourceComment] },

    /// Comments that all fall into the formatted range.
    Formatted {
        #[allow(unused)]
        comments: &'a [SourceComment],
    },
}

impl<'a> SuppressionComments<'a> {
    fn unwrap_suppression_starts(&self) -> (&'a [SourceComment], &'a SourceComment) {
        match self {
            SuppressionComments::SuppressionStarts {
                formatted_comments,
                format_off_comment,
            } => (formatted_comments, *format_off_comment),
            _ => {
                panic!("Expected SuppressionStarts")
            }
        }
    }
}

struct CommentRangeIter<'a> {
    comments: &'a [SourceComment],
    source: &'a str,
    in_suppression: InSuppression,
}

impl<'a> CommentRangeIter<'a> {
    fn in_suppression(comments: &'a [SourceComment], source: &'a str) -> Self {
        Self {
            comments,
            in_suppression: InSuppression::Yes,
            source,
        }
    }

    fn outside_suppression(comments: &'a [SourceComment], source: &'a str) -> Self {
        Self {
            comments,
            in_suppression: InSuppression::No,
            source,
        }
    }

    /// Returns a slice containing the remaining comments.
    fn as_slice(&self) -> &'a [SourceComment] {
        self.comments
    }
}

impl<'a> Iterator for CommentRangeIter<'a> {
    type Item = SuppressionComments<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.comments.is_empty() {
            None
        } else {
            Some(match self.in_suppression {
                // Inside of a suppressed range
                InSuppression::Yes => {
                    if let Some(format_on_position) = self
                        .comments
                        .iter()
                        .position(|comment| comment.is_suppression_on_comment(self.source))
                    {
                        let (suppressed_comments, formatted) =
                            self.comments.split_at(format_on_position);
                        let (format_on_comment, rest) = formatted.split_first().unwrap();

                        let (formatted_comments, format_off_comment) =
                            if let Some(format_off_position) = rest
                                .iter()
                                .position(|comment| comment.is_suppression_off_comment(self.source))
                            {
                                let (formatted_comments, suppressed_comments) =
                                    rest.split_at(format_off_position);
                                let (format_off_comment, rest) =
                                    suppressed_comments.split_first().unwrap();

                                self.comments = rest;

                                (formatted_comments, Some(format_off_comment))
                            } else {
                                self.in_suppression = InSuppression::No;

                                self.comments = &[];
                                (rest, None)
                            };

                        SuppressionComments::SuppressionEnds {
                            suppressed_comments,
                            format_on_comment,
                            formatted_comments,
                            format_off_comment,
                        }
                    } else {
                        SuppressionComments::Suppressed {
                            comments: std::mem::take(&mut self.comments),
                        }
                    }
                }

                // Outside of a suppression
                InSuppression::No => {
                    if let Some(format_off_position) = self
                        .comments
                        .iter()
                        .position(|comment| comment.is_suppression_off_comment(self.source))
                    {
                        self.in_suppression = InSuppression::Yes;

                        let (formatted_comments, suppressed) =
                            self.comments.split_at(format_off_position);
                        let format_off_comment = &suppressed[0];

                        self.comments = &suppressed[1..];

                        SuppressionComments::SuppressionStarts {
                            formatted_comments,
                            format_off_comment,
                        }
                    } else {
                        SuppressionComments::Formatted {
                            comments: std::mem::take(&mut self.comments),
                        }
                    }
                }
            })
        }
    }
}

impl FusedIterator for CommentRangeIter<'_> {}

struct TrailingFormatOffComment<'a>(&'a SourceComment);

impl Format<PyFormatContext<'_>> for TrailingFormatOffComment<'_> {
    fn fmt(&self, f: &mut Formatter<PyFormatContext<'_>>) -> FormatResult<()> {
        debug_assert!(self.0.is_unformatted());
        let lines_before_comment = lines_before(self.0.start(), f.context().source());

        write!(
            f,
            [empty_lines(lines_before_comment), format_comment(self.0)]
        )?;

        self.0.mark_formatted();

        Ok(())
    }
}
