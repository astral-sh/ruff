use std::borrow::Cow;
use std::iter::FusedIterator;
use std::slice::Iter;

use ruff_formatter::{write, FormatError};
use ruff_python_ast::AnyNodeRef;
use ruff_python_ast::Stmt;
use ruff_python_parser::{self as parser, TokenKind};
use ruff_python_trivia::lines_before;
use ruff_source_file::LineRanges;
use ruff_text_size::{Ranged, TextRange, TextSize};

use crate::comments::format::{empty_lines, format_comment};
use crate::comments::{leading_comments, trailing_comments, SourceComment};
use crate::prelude::*;
use crate::statement::clause::ClauseHeader;
use crate::statement::suite::SuiteChildStatement;
use crate::statement::trailing_semicolon;

/// Returns `true` if the statements coming after `leading_or_trailing_comments` are suppressed.
///
/// The result is only correct if called for statement comments in a non-suppressed range.
///
/// # Panics
/// If `leading_or_trailing_comments` contain any range that's outside of `source`.
pub(crate) fn starts_suppression(
    leading_or_trailing_comments: &[SourceComment],
    source: &str,
) -> bool {
    let mut iter = CommentRangeIter::outside_suppression(leading_or_trailing_comments, source);
    // Move the iter to the last element.
    let _ = iter.by_ref().last();

    matches!(iter.in_suppression, InSuppression::Yes)
}

/// Returns `true` if the statements coming after `leading_or_trailing_comments` are no longer suppressed.
///
/// The result is only correct if called for statement comments in a suppressed range.
///
/// # Panics
/// If `leading_or_trailing_comments` contain any range that's outside of `source`.
pub(crate) fn ends_suppression(
    leading_or_trailing_comments: &[SourceComment],
    source: &str,
) -> bool {
    let mut iter = CommentRangeIter::in_suppression(leading_or_trailing_comments, source);
    // Move the iter to the last element.
    let _ = iter.by_ref().last();

    !matches!(iter.in_suppression, InSuppression::Yes)
}

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
        CommentRangeIter::outside_suppression(comments.leading(first_suppressed), source);

    let before_format_off = leading_comment_ranges
        .next()
        .expect("Suppressed node to have leading comments");

    let (formatted_comments, format_off_comment) = before_format_off.unwrap_suppression_starts();

    // Format the leading comments before the fmt off
    // ```python
    // # leading comment that gets formatted
    // # fmt: off
    // statement
    // ```
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
    let indentation = Indentation::from_stmt(last_formatted.statement(), source);

    let trailing_node_comments = comments.trailing(last_formatted);
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
            // A `fmt: off`..`fmt: on` sequence. Disable formatting for the in-between comments.
            // ```python
            // def test():
            //      pass
            //      # fmt: off
            //          # haha
            //      # fmt: on
            //      # fmt: off (maybe)
            // ```
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
                        FormatVerbatimStatementRange {
                            verbatim_range: TextRange::new(
                                format_off_comment.end(),
                                format_on_comment.start(),
                            ),
                            indentation
                        },
                        trailing_comments(std::slice::from_ref(format_on_comment)),
                        trailing_comments(formatted_comments),
                    ]
                )?;

                // `fmt: off`..`fmt:on`..`fmt:off` sequence
                // ```python
                // def test():
                //      pass
                //      # fmt: off
                //          # haha
                //      # fmt: on
                //      # fmt: off
                // ```
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
            // SAFETY: Unreachable because the function returns as soon as it reaches the end of the suppressed range
            SuppressionComments::SuppressionStarts { .. }
            | SuppressionComments::Formatted { .. } => unreachable!(),
        }
    }

    // The statement with the suppression comment isn't the last statement in the suite.
    // Format the statements up to the first `fmt: on` comment (or end of the suite) as verbatim/suppressed.
    // ```python
    // a + b
    // # fmt: off
    //
    // def a():
    //  pass
    // ```
    if let Some(first_suppressed) = statements.next() {
        write_suppressed_statements(
            format_off_comment,
            SuiteChildStatement::Other(first_suppressed),
            comments.leading(first_suppressed),
            statements,
            f,
        )
    }
    // The suppression comment is the block's last node. Format any trailing comments as suppressed
    // ```python
    // def test():
    //      pass
    //      # fmt: off
    //      # a trailing comment
    // ```
    else if let Some(last_comment) = trailing_node_comments.last() {
        FormatVerbatimStatementRange {
            verbatim_range: TextRange::new(format_off_comment.end(), last_comment.end()),
            indentation,
        }
        .fmt(f)?;
        Ok(last_formatted.statement())
    }
    // The suppression comment is the very last code in the block. There's nothing more to format.
    // ```python
    // def test():
    //      pass
    //      # fmt: off
    // ```
    else {
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

    let mut statement = first_suppressed;
    let mut leading_node_comments = first_suppressed_leading_comments;
    let mut format_off_comment = format_off_comment;
    let indentation = Indentation::from_stmt(first_suppressed.statement(), source);

    loop {
        for range in CommentRangeIter::in_suppression(leading_node_comments, source) {
            match range {
                // All leading comments are suppressed
                // ```python
                // # suppressed comment
                // statement
                // ```
                SuppressionComments::Suppressed { comments } => {
                    for comment in comments {
                        comment.mark_formatted();
                    }
                }

                // Node has a leading `fmt: on` comment and maybe another `fmt: off` comment
                // ```python
                // # suppressed comment (optional)
                // # fmt: on
                // # formatted comment (optional)
                // # fmt: off (optional)
                // statement
                // ```
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
                            FormatVerbatimStatementRange {
                                verbatim_range: TextRange::new(
                                    format_off_comment.end(),
                                    format_on_comment.start(),
                                ),
                                indentation
                            },
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
                            .trailing(statement)
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

        for range in CommentRangeIter::in_suppression(comments.trailing(statement), source) {
            match range {
                // All trailing comments are suppressed
                // ```python
                // statement
                // # suppressed
                // ```
                SuppressionComments::Suppressed { comments } => {
                    for comment in comments {
                        comment.mark_formatted();
                    }
                }

                // Node has a trailing `fmt: on` comment and maybe another `fmt: off` comment
                // ```python
                // statement
                // # suppressed comment (optional)
                // # fmt: on
                // # formatted comment (optional)
                // # fmt: off (optional)
                // ```
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
                            FormatVerbatimStatementRange {
                                verbatim_range: TextRange::new(
                                    format_off_comment.end(),
                                    format_on_comment.start()
                                ),
                                indentation
                            },
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
            leading_node_comments = comments.leading(next_statement);
        } else {
            let mut current = AnyNodeRef::from(statement.statement());
            // Expand the range of the statement to include any trailing comments or semicolons.
            let end = loop {
                if let Some(comment) = comments.trailing(current).last() {
                    break comment.end();
                } else if let Some(child) = current.last_child_in_body() {
                    current = child;
                } else {
                    break trailing_semicolon(current, source)
                        .map_or(statement.end(), TextRange::end);
                }
            };

            FormatVerbatimStatementRange {
                verbatim_range: TextRange::new(format_off_comment.end(), end),
                indentation,
            }
            .fmt(f)?;

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
        ///   the `fmt: off` comment.
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
        if let SuppressionComments::SuppressionStarts {
            formatted_comments,
            format_off_comment,
        } = self
        {
            (formatted_comments, *format_off_comment)
        } else {
            panic!("Expected SuppressionStarts")
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
            return None;
        }

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

/// Stores the indentation of a statement by storing the number of indentation characters.
/// Storing the number of indentation characters is sufficient because:
/// * Two indentations are equal if they result in the same column, regardless of the used tab size.
///   This implementation makes use of this fact and assumes a tab size of 1.
/// * The source document is correctly indented because it is valid Python code (or the formatter would have failed parsing the code).
#[derive(Copy, Clone)]
struct Indentation(u32);

impl Indentation {
    fn from_stmt(stmt: &Stmt, source: &str) -> Indentation {
        let line_start = source.line_start(stmt.start());

        let mut indentation = 0u32;
        for c in source[TextRange::new(line_start, stmt.start())].chars() {
            if is_indent_whitespace(c) {
                indentation += 1;
            } else {
                break;
            }
        }

        Indentation(indentation)
    }

    fn trim_indent(self, ranged: impl Ranged, source: &str) -> TextRange {
        let range = ranged.range();
        let mut start_offset = TextSize::default();

        for c in source[range].chars().take(self.0 as usize) {
            if is_indent_whitespace(c) {
                start_offset += TextSize::new(1);
            } else {
                break;
            }
        }

        TextRange::new(range.start() + start_offset, range.end())
    }
}

/// Returns `true` for a space or tab character.
///
/// This is different than [`is_python_whitespace`] in that it returns `false` for a form feed character.
/// Form feed characters are excluded because they should be preserved in the suppressed output.
const fn is_indent_whitespace(c: char) -> bool {
    matches!(c, ' ' | '\t')
}

/// Formats a verbatim range where the top-level nodes are statements (or statement-level comments).
///
/// Formats each statement as written in the source code, but adds the right indentation to match
/// the indentation of formatted statements:
///
/// ```python
/// def test():
///   print("formatted")
///   # fmt: off
///   (
///     not_formatted + b
///   )
///   # fmt: on
/// ```
///
/// Gets formatted as
///
/// ```python
/// def test():
///     print("formatted")
///     # fmt: off
///     (
///     not_formatted + b
///     )
///     # fmt: on
/// ```
///
/// Notice how the `not_formatted + b` expression statement gets the same indentation as the `print` statement above,
/// but the indentation of the expression remains unchanged. It changes the indentation to:
/// * Prevent syntax errors because of different indentation levels between formatted and suppressed statements.
/// * Align with the `fmt: skip` where statements are indented as well, but inner expressions are formatted as is.
struct FormatVerbatimStatementRange {
    verbatim_range: TextRange,
    indentation: Indentation,
}

impl Format<PyFormatContext<'_>> for FormatVerbatimStatementRange {
    fn fmt(&self, f: &mut Formatter<PyFormatContext<'_>>) -> FormatResult<()> {
        let logical_lines = LogicalLinesIter::new(
            f.context().tokens().in_range(self.verbatim_range).iter(),
            self.verbatim_range,
        );
        let mut first = true;

        for logical_line in logical_lines {
            let logical_line = logical_line?;

            let trimmed_line_range = self
                .indentation
                .trim_indent(&logical_line, f.context().source());

            // A line without any content, write an empty line, except for the first or last (indent only) line.
            if trimmed_line_range.is_empty() {
                if logical_line.has_trailing_newline {
                    if first {
                        hard_line_break().fmt(f)?;
                    } else {
                        empty_line().fmt(f)?;
                    }
                }
            } else {
                // Non empty line, write the text of the line
                write!(
                    f,
                    [
                        source_position(trimmed_line_range.start()),
                        verbatim_text(trimmed_line_range),
                        source_position(trimmed_line_range.end())
                    ]
                )?;

                // Write the line separator that terminates the line, except if it is the last line (that isn't separated by a hard line break).
                if logical_line.has_trailing_newline {
                    hard_line_break().fmt(f)?;
                }
            }

            first = false;
        }

        Ok(())
    }
}

struct LogicalLinesIter<'a> {
    tokens: Iter<'a, parser::Token>,
    // The end of the last logical line
    last_line_end: TextSize,
    // The position where the content to lex ends.
    content_end: TextSize,
}

impl<'a> LogicalLinesIter<'a> {
    fn new(tokens: Iter<'a, parser::Token>, verbatim_range: TextRange) -> Self {
        Self {
            tokens,
            last_line_end: verbatim_range.start(),
            content_end: verbatim_range.end(),
        }
    }
}

impl Iterator for LogicalLinesIter<'_> {
    type Item = FormatResult<LogicalLine>;

    fn next(&mut self) -> Option<Self::Item> {
        let mut parens = 0u32;

        let (content_end, full_end) = loop {
            match self.tokens.next() {
                Some(token) if token.kind() == TokenKind::Unknown => {
                    return Some(Err(FormatError::syntax_error(
                        "Unexpected token when lexing verbatim statement range.",
                    )))
                }
                Some(token) => match token.kind() {
                    TokenKind::Newline => break (token.start(), token.end()),
                    // Ignore if inside an expression
                    TokenKind::NonLogicalNewline if parens == 0 => {
                        break (token.start(), token.end())
                    }
                    TokenKind::Lbrace | TokenKind::Lpar | TokenKind::Lsqb => {
                        parens = parens.saturating_add(1);
                    }
                    TokenKind::Rbrace | TokenKind::Rpar | TokenKind::Rsqb => {
                        parens = parens.saturating_sub(1);
                    }
                    _ => {}
                },
                None => {
                    // Returns any content that comes after the last newline. This is mainly whitespace
                    // or characters that the `Lexer` skips, like a form-feed character.
                    return if self.last_line_end < self.content_end {
                        let content_start = self.last_line_end;
                        self.last_line_end = self.content_end;
                        Some(Ok(LogicalLine {
                            content_range: TextRange::new(content_start, self.content_end),
                            has_trailing_newline: false,
                        }))
                    } else {
                        None
                    };
                }
            }
        };

        let line_start = self.last_line_end;
        self.last_line_end = full_end;

        Some(Ok(LogicalLine {
            content_range: TextRange::new(line_start, content_end),
            has_trailing_newline: true,
        }))
    }
}

impl FusedIterator for LogicalLinesIter<'_> {}

/// A logical line or a comment (or form feed only) line
struct LogicalLine {
    /// The range of this lines content (excluding the trailing newline)
    content_range: TextRange,
    /// Does this logical line have a trailing newline or does it just happen to be the last line.
    has_trailing_newline: bool,
}

impl Ranged for LogicalLine {
    fn range(&self) -> TextRange {
        self.content_range
    }
}

pub(crate) struct VerbatimText {
    verbatim_range: TextRange,
}

pub(crate) fn verbatim_text<T>(item: T) -> VerbatimText
where
    T: Ranged,
{
    VerbatimText {
        verbatim_range: item.range(),
    }
}

impl Format<PyFormatContext<'_>> for VerbatimText {
    fn fmt(&self, f: &mut PyFormatter) -> FormatResult<()> {
        f.write_element(FormatElement::Tag(Tag::StartVerbatim(
            tag::VerbatimKind::Verbatim {
                length: self.verbatim_range.len(),
            },
        )));

        match normalize_newlines(&f.context().source()[self.verbatim_range], ['\r']) {
            Cow::Borrowed(_) => {
                write!(f, [source_text_slice(self.verbatim_range)])?;
            }
            Cow::Owned(cleaned) => {
                text(&cleaned).fmt(f)?;
            }
        }

        f.write_element(FormatElement::Tag(Tag::EndVerbatim));
        Ok(())
    }
}

/// Disables formatting for `node` and instead uses the same formatting as the node has in source.
///
/// The `node` gets indented as any formatted node to avoid syntax errors when the indentation string changes (e.g. from 2 spaces to 4).
/// The `node`s leading and trailing comments are formatted as usual, except if they fall into the suppressed node's range.
#[cold]
pub(crate) fn suppressed_node<'a, N>(node: N) -> FormatSuppressedNode<'a>
where
    N: Into<AnyNodeRef<'a>>,
{
    FormatSuppressedNode { node: node.into() }
}

pub(crate) struct FormatSuppressedNode<'a> {
    node: AnyNodeRef<'a>,
}

impl Format<PyFormatContext<'_>> for FormatSuppressedNode<'_> {
    fn fmt(&self, f: &mut Formatter<PyFormatContext<'_>>) -> FormatResult<()> {
        let comments = f.context().comments().clone();
        let node_comments = comments.leading_dangling_trailing(self.node);

        // Mark all comments as formatted that fall into the node range
        for comment in node_comments.leading {
            if comment.start() > self.node.start() {
                comment.mark_formatted();
            }
        }

        for comment in node_comments.trailing {
            if comment.start() < self.node.end() {
                comment.mark_formatted();
            }
        }

        // Some statements may end with a semicolon. Preserve the semicolon
        let semicolon_range = self
            .node
            .is_statement()
            .then(|| trailing_semicolon(self.node, f.context().source()))
            .flatten();
        let verbatim_range = semicolon_range.map_or(self.node.range(), |semicolon| {
            TextRange::new(self.node.start(), semicolon.end())
        });
        comments.mark_verbatim_node_comments_formatted(self.node);

        // Write the outer comments and format the node as verbatim
        write!(
            f,
            [
                leading_comments(node_comments.leading),
                source_position(verbatim_range.start()),
                verbatim_text(verbatim_range),
                source_position(verbatim_range.end()),
                trailing_comments(node_comments.trailing)
            ]
        )
    }
}

#[cold]
pub(crate) fn write_suppressed_clause_header(
    header: ClauseHeader,
    f: &mut PyFormatter,
) -> FormatResult<()> {
    let range = header.range(f.context().source())?;

    // Write the outer comments and format the node as verbatim
    write!(
        f,
        [
            source_position(range.start()),
            verbatim_text(range),
            source_position(range.end())
        ]
    )?;

    let comments = f.context().comments();
    header.visit(&mut |child| {
        for comment in comments.leading_trailing(child) {
            comment.mark_formatted();
        }
        comments.mark_verbatim_node_comments_formatted(child);
    });

    Ok(())
}
