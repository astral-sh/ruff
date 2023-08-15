use std::borrow::Cow;
use std::iter::FusedIterator;

use unicode_width::UnicodeWidthStr;

use ruff_formatter::{write, FormatError};
use ruff_python_ast::node::AnyNodeRef;
use ruff_python_ast::{
    ElifElseClause, ExceptHandlerExceptHandler, MatchCase, Ranged, Stmt, StmtClassDef, StmtFor,
    StmtFunctionDef, StmtIf, StmtMatch, StmtTry, StmtWhile, StmtWith,
};
use ruff_python_parser::lexer::{lex_starts_at, LexResult};
use ruff_python_parser::{Mode, Tok};
use ruff_python_trivia::{lines_before, SimpleToken, SimpleTokenKind, SimpleTokenizer};
use ruff_source_file::Locator;
use ruff_text_size::{TextRange, TextSize};

use crate::comments::format::{empty_lines, format_comment};
use crate::comments::{leading_comments, trailing_comments, SourceComment};
use crate::prelude::*;
use crate::statement::suite::SuiteChildStatement;

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
            comments.leading_comments(first_suppressed),
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
                // All leading comments are suppressed
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
            leading_node_comments = comments.leading_comments(next_statement);
        } else {
            let end = comments
                .trailing_comments(statement)
                .last()
                .map_or(statement.end(), Ranged::end);

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
        let line_start = Locator::new(source).line_start(stmt.start());

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
        let lexer = lex_starts_at(
            &f.context().source()[self.verbatim_range],
            Mode::Module,
            self.verbatim_range.start(),
        );

        let logical_lines = LogicalLinesIter::new(lexer, self.verbatim_range);
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
                verbatim_text(trimmed_line_range, logical_line.contains_newlines).fmt(f)?;

                // Write the line separator that terminates the line, except if it is the last line (that isn't separated by a hard line break).
                if logical_line.has_trailing_newline {
                    // Insert an empty line if the text is non-empty but all characters have a width of zero.
                    // This is necessary to work around the fact that the Printer omits hard line breaks if the line width is 0.
                    // The alternative is to "fix" the printer and explicitly track the width and whether the line is empty.
                    // There's currently no use case for zero-width content outside of the verbatim context (and, form feeds are a Python specific speciality).
                    // It, therefore, feels wrong to add additional complexity to the very hot `Printer::print_char` function,
                    // to work around this special case. Therefore, work around the Printer behavior here, in the cold verbatim-formatting.
                    if f.context().source()[trimmed_line_range].width() == 0 {
                        empty_line().fmt(f)?;
                    } else {
                        hard_line_break().fmt(f)?;
                    }
                }
            }

            first = false;
        }

        Ok(())
    }
}

struct LogicalLinesIter<I> {
    lexer: I,
    // The end of the last logical line
    last_line_end: TextSize,
    // The position where the content to lex ends.
    content_end: TextSize,
}

impl<I> LogicalLinesIter<I> {
    fn new(lexer: I, verbatim_range: TextRange) -> Self {
        Self {
            lexer,
            last_line_end: verbatim_range.start(),
            content_end: verbatim_range.end(),
        }
    }
}

impl<I> Iterator for LogicalLinesIter<I>
where
    I: Iterator<Item = LexResult>,
{
    type Item = FormatResult<LogicalLine>;

    fn next(&mut self) -> Option<Self::Item> {
        let mut parens = 0u32;
        let mut contains_newlines = ContainsNewlines::No;

        let (content_end, full_end) = loop {
            match self.lexer.next() {
                Some(Ok((token, range))) => match token {
                    Tok::Newline => break (range.start(), range.end()),
                    // Ignore if inside an expression
                    Tok::NonLogicalNewline if parens == 0 => break (range.start(), range.end()),
                    Tok::NonLogicalNewline => {
                        contains_newlines = ContainsNewlines::Yes;
                    }
                    Tok::Lbrace | Tok::Lpar | Tok::Lsqb => {
                        parens = parens.saturating_add(1);
                    }
                    Tok::Rbrace | Tok::Rpar | Tok::Rsqb => {
                        parens = parens.saturating_sub(1);
                    }
                    Tok::String { value, .. } if value.contains(['\n', '\r']) => {
                        contains_newlines = ContainsNewlines::Yes;
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
                            contains_newlines: ContainsNewlines::No,
                            has_trailing_newline: false,
                        }))
                    } else {
                        None
                    };
                }
                Some(Err(_)) => {
                    return Some(Err(FormatError::syntax_error(
                        "Unexpected token when lexing verbatim statement range.",
                    )))
                }
            }
        };

        let line_start = self.last_line_end;
        self.last_line_end = full_end;

        Some(Ok(LogicalLine {
            content_range: TextRange::new(line_start, content_end),
            contains_newlines,
            has_trailing_newline: true,
        }))
    }
}

impl<I> FusedIterator for LogicalLinesIter<I> where I: Iterator<Item = LexResult> {}

/// A logical line or a comment (or form feed only) line
struct LogicalLine {
    /// The range of this lines content (excluding the trailing newline)
    content_range: TextRange,
    /// Whether the content in `content_range` contains any newlines.
    contains_newlines: ContainsNewlines,
    /// Does this logical line have a trailing newline or does it just happen to be the last line.
    has_trailing_newline: bool,
}

impl Ranged for LogicalLine {
    fn range(&self) -> TextRange {
        self.content_range
    }
}

struct VerbatimText {
    verbatim_range: TextRange,
    contains_newlines: ContainsNewlines,
}

fn verbatim_text<T>(item: T, contains_newlines: ContainsNewlines) -> VerbatimText
where
    T: Ranged,
{
    VerbatimText {
        verbatim_range: item.range(),
        contains_newlines,
    }
}

impl Format<PyFormatContext<'_>> for VerbatimText {
    fn fmt(&self, f: &mut PyFormatter) -> FormatResult<()> {
        f.write_element(FormatElement::Tag(Tag::StartVerbatim(
            tag::VerbatimKind::Verbatim {
                length: self.verbatim_range.len(),
            },
        )));

        match normalize_newlines(f.context().locator().slice(self.verbatim_range), ['\r']) {
            Cow::Borrowed(_) => {
                write!(
                    f,
                    [source_text_slice(
                        self.verbatim_range,
                        self.contains_newlines
                    )]
                )?;
            }
            Cow::Owned(cleaned) => {
                write!(
                    f,
                    [
                        dynamic_text(&cleaned, Some(self.verbatim_range.start())),
                        source_position(self.verbatim_range.end())
                    ]
                )?;
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
        let node_comments = comments.leading_dangling_trailing_comments(self.node);

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

        comments.mark_verbatim_node_comments_formatted(self.node);

        // Write the outer comments and format the node as verbatim
        write!(
            f,
            [
                leading_comments(node_comments.leading),
                verbatim_text(self.node, ContainsNewlines::Detect),
                trailing_comments(node_comments.trailing)
            ]
        )
    }
}

#[derive(Copy, Clone)]
pub(crate) enum SuppressedClauseHeader<'a> {
    Class(&'a StmtClassDef),
    Function(&'a StmtFunctionDef),
    If(&'a StmtIf),
    ElifElse(&'a ElifElseClause),
    Try(&'a StmtTry),
    ExceptHandler(&'a ExceptHandlerExceptHandler),
    TryFinally(&'a StmtTry),
    Match(&'a StmtMatch),
    MatchCase(&'a MatchCase),
    For(&'a StmtFor),
    While(&'a StmtWhile),
    With(&'a StmtWith),
    OrElse(OrElseParent<'a>),
}

impl<'a> SuppressedClauseHeader<'a> {
    fn range(self, source: &str) -> FormatResult<TextRange> {
        let keyword_range = self.keyword_range(source)?;

        let mut last_child_end = None;

        self.visit_children(&mut |child| last_child_end = Some(child.end()));

        let end = match self {
            SuppressedClauseHeader::Class(class) => {
                Some(last_child_end.unwrap_or(class.name.end()))
            }
            SuppressedClauseHeader::Function(function) => {
                Some(last_child_end.unwrap_or(function.name.end()))
            }
            SuppressedClauseHeader::ElifElse(_)
            | SuppressedClauseHeader::Try(_)
            | SuppressedClauseHeader::If(_)
            | SuppressedClauseHeader::TryFinally(_)
            | SuppressedClauseHeader::Match(_)
            | SuppressedClauseHeader::MatchCase(_)
            | SuppressedClauseHeader::For(_)
            | SuppressedClauseHeader::While(_)
            | SuppressedClauseHeader::With(_)
            | SuppressedClauseHeader::OrElse(_) => last_child_end,

            SuppressedClauseHeader::ExceptHandler(handler) => handler
                .name
                .as_ref()
                .map(ruff_python_ast::Ranged::end)
                .or(last_child_end),
        };

        let colon = colon_range(end.unwrap_or(keyword_range.end()), source)?;

        Ok(TextRange::new(keyword_range.start(), colon.end()))
    }

    // Needs to know child nodes.
    // Could use normal lexer to get header end since we know that it is rare
    fn visit_children<F>(self, visitor: &mut F)
    where
        F: FnMut(AnyNodeRef),
    {
        fn visit<'a, N, F>(node: N, visitor: &mut F)
        where
            N: Into<AnyNodeRef<'a>>,
            F: FnMut(AnyNodeRef<'a>),
        {
            visitor(node.into());
        }

        match self {
            SuppressedClauseHeader::Class(class) => {
                if let Some(type_params) = &class.type_params {
                    visit(type_params.as_ref(), visitor);
                }

                if let Some(arguments) = &class.arguments {
                    visit(arguments.as_ref(), visitor);
                }
            }
            SuppressedClauseHeader::Function(function) => {
                visit(function.parameters.as_ref(), visitor);
                if let Some(type_params) = function.type_params.as_ref() {
                    visit(type_params, visitor);
                }
            }
            SuppressedClauseHeader::If(if_stmt) => {
                visit(if_stmt.test.as_ref(), visitor);
            }
            SuppressedClauseHeader::ElifElse(clause) => {
                if let Some(test) = clause.test.as_ref() {
                    visit(test, visitor);
                }
            }

            SuppressedClauseHeader::ExceptHandler(handler) => {
                if let Some(ty) = handler.type_.as_deref() {
                    visit(ty, visitor);
                }
            }
            SuppressedClauseHeader::Match(match_stmt) => {
                visit(match_stmt.subject.as_ref(), visitor);
            }
            SuppressedClauseHeader::MatchCase(match_case) => {
                visit(&match_case.pattern, visitor);

                if let Some(guard) = match_case.guard.as_deref() {
                    visit(guard, visitor);
                }
            }
            SuppressedClauseHeader::For(for_stmt) => {
                visit(for_stmt.target.as_ref(), visitor);
                visit(for_stmt.iter.as_ref(), visitor);
            }
            SuppressedClauseHeader::While(while_stmt) => {
                visit(while_stmt.test.as_ref(), visitor);
            }
            SuppressedClauseHeader::With(with_stmt) => {
                for item in &with_stmt.items {
                    visit(item, visitor);
                }
            }
            SuppressedClauseHeader::Try(_)
            | SuppressedClauseHeader::TryFinally(_)
            | SuppressedClauseHeader::OrElse(_) => {}
        }
    }

    fn keyword_range(self, source: &str) -> FormatResult<TextRange> {
        match self {
            SuppressedClauseHeader::Class(header) => {
                find_keyword(header.start(), SimpleTokenKind::Class, source)
            }
            SuppressedClauseHeader::Function(header) => {
                let keyword = if header.is_async {
                    SimpleTokenKind::Async
                } else {
                    SimpleTokenKind::Def
                };
                find_keyword(header.start(), keyword, source)
            }
            SuppressedClauseHeader::If(header) => {
                find_keyword(header.start(), SimpleTokenKind::If, source)
            }
            SuppressedClauseHeader::ElifElse(ElifElseClause {
                test: None, range, ..
            }) => find_keyword(range.start(), SimpleTokenKind::Else, source),
            SuppressedClauseHeader::ElifElse(ElifElseClause {
                test: Some(_),
                range,
                ..
            }) => find_keyword(range.start(), SimpleTokenKind::Elif, source),
            SuppressedClauseHeader::Try(header) => {
                find_keyword(header.start(), SimpleTokenKind::Try, source)
            }
            SuppressedClauseHeader::ExceptHandler(header) => {
                find_keyword(header.start(), SimpleTokenKind::Except, source)
            }
            SuppressedClauseHeader::TryFinally(header) => {
                let last_statement = header
                    .orelse
                    .last()
                    .map(AnyNodeRef::from)
                    .or_else(|| header.handlers.last().map(AnyNodeRef::from))
                    .or_else(|| header.body.last().map(AnyNodeRef::from))
                    .unwrap();

                find_keyword(last_statement.end(), SimpleTokenKind::Finally, source)
            }
            SuppressedClauseHeader::Match(header) => {
                find_keyword(header.start(), SimpleTokenKind::Match, source)
            }
            SuppressedClauseHeader::MatchCase(header) => {
                find_keyword(header.start(), SimpleTokenKind::Case, source)
            }
            SuppressedClauseHeader::For(header) => {
                let keyword = if header.is_async {
                    SimpleTokenKind::Async
                } else {
                    SimpleTokenKind::For
                };
                find_keyword(header.start(), keyword, source)
            }
            SuppressedClauseHeader::While(header) => {
                find_keyword(header.start(), SimpleTokenKind::While, source)
            }
            SuppressedClauseHeader::With(header) => {
                let keyword = if header.is_async {
                    SimpleTokenKind::Async
                } else {
                    SimpleTokenKind::With
                };

                find_keyword(header.start(), keyword, source)
            }
            SuppressedClauseHeader::OrElse(header) => match header {
                OrElseParent::Try(try_stmt) => {
                    let last_statement = try_stmt
                        .handlers
                        .last()
                        .map(AnyNodeRef::from)
                        .or_else(|| try_stmt.body.last().map(AnyNodeRef::from))
                        .unwrap();

                    find_keyword(last_statement.end(), SimpleTokenKind::Else, source)
                }
                OrElseParent::For(StmtFor { body, .. })
                | OrElseParent::While(StmtWhile { body, .. }) => {
                    find_keyword(body.last().unwrap().end(), SimpleTokenKind::Else, source)
                }
            },
        }
    }
}

fn find_keyword(
    start_position: TextSize,
    keyword: SimpleTokenKind,
    source: &str,
) -> FormatResult<TextRange> {
    let mut tokenizer = SimpleTokenizer::starts_at(start_position, source).skip_trivia();

    match tokenizer.next() {
        Some(token) if token.kind() == keyword => Ok(token.range()),
        Some(other) => {
            debug_assert!(
                false,
                "Expected the keyword token {keyword:?} but found the token {other:?} instead."
            );
            Err(FormatError::syntax_error(
                "Expected the keyword token but found another token instead.",
            ))
        }
        None => {
            debug_assert!(
                false,
                "Expected the keyword token {keyword:?} but reached the end of the source instead."
            );
            Err(FormatError::syntax_error(
                "Expected the case header keyword token but reached the end of the source instead.",
            ))
        }
    }
}

fn colon_range(after_keyword_or_condition: TextSize, source: &str) -> FormatResult<TextRange> {
    let mut tokenizer = SimpleTokenizer::starts_at(after_keyword_or_condition, source)
        .skip_trivia()
        .skip_while(|token| token.kind() == SimpleTokenKind::RParen);

    match tokenizer.next() {
        Some(SimpleToken {
            kind: SimpleTokenKind::Colon,
            range,
        }) => Ok(range),
        Some(token) => {
            debug_assert!(false, "Expected the colon marking the end of the case header but found {token:?} instead.");
            Err(FormatError::syntax_error("Expected colon marking the end of the case header but found another token instead."))
        }
        None => {
            debug_assert!(false, "Expected the colon marking the end of the case header but found the end of the range.");
            Err(FormatError::syntax_error("Expected the colon marking the end of the case header but found the end of the range."))
        }
    }
}

#[derive(Copy, Clone)]
pub(crate) enum OrElseParent<'a> {
    Try(&'a StmtTry),
    For(&'a StmtFor),
    While(&'a StmtWhile),
}

impl Format<PyFormatContext<'_>> for SuppressedClauseHeader<'_> {
    #[cold]
    fn fmt(&self, f: &mut Formatter<PyFormatContext<'_>>) -> FormatResult<()> {
        // Write the outer comments and format the node as verbatim
        write!(
            f,
            [verbatim_text(
                self.range(f.context().source())?,
                ContainsNewlines::Detect
            ),]
        )?;

        let comments = f.context().comments();
        self.visit_children(&mut |child| comments.mark_verbatim_node_comments_formatted(child));

        Ok(())
    }
}
