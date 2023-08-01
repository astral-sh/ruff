use ruff_formatter::{format_args, write, Argument, Arguments};
use ruff_python_ast::Ranged;
use ruff_python_trivia::{SimpleToken, SimpleTokenKind, SimpleTokenizer};
use ruff_text_size::{TextRange, TextSize};

use crate::comments::{dangling_comments, SourceComment};
use crate::context::{NodeLevel, WithNodeLevel};
use crate::prelude::*;
use crate::MagicTrailingComma;

/// Adds parentheses and indents `content` if it doesn't fit on a line.
pub(crate) fn parenthesize_if_expands<'ast, T>(content: &T) -> ParenthesizeIfExpands<'_, 'ast>
where
    T: Format<PyFormatContext<'ast>>,
{
    ParenthesizeIfExpands {
        inner: Argument::new(content),
    }
}

pub(crate) struct ParenthesizeIfExpands<'a, 'ast> {
    inner: Argument<'a, PyFormatContext<'ast>>,
}

impl<'ast> Format<PyFormatContext<'ast>> for ParenthesizeIfExpands<'_, 'ast> {
    fn fmt(&self, f: &mut Formatter<PyFormatContext<'ast>>) -> FormatResult<()> {
        {
            let mut f = WithNodeLevel::new(NodeLevel::ParenthesizedExpression, f);

            write!(
                f,
                [group(&format_args![
                    if_group_breaks(&text("(")),
                    soft_block_indent(&Arguments::from(&self.inner)),
                    if_group_breaks(&text(")")),
                ])]
            )
        }
    }
}

/// Provides Python specific extensions to [`Formatter`].
pub(crate) trait PyFormatterExtensions<'ast, 'buf> {
    /// A builder that separates each element by a `,` and a [`soft_line_break_or_space`].
    /// It emits a trailing `,` that is only shown if the enclosing group expands. It forces the enclosing
    /// group to expand if the last item has a trailing `comma` and the magical comma option is enabled.
    fn join_comma_separated<'fmt>(
        &'fmt mut self,
        sequence_end: TextSize,
    ) -> JoinCommaSeparatedBuilder<'fmt, 'ast, 'buf>;
}

impl<'buf, 'ast> PyFormatterExtensions<'ast, 'buf> for PyFormatter<'ast, 'buf> {
    fn join_comma_separated<'fmt>(
        &'fmt mut self,
        sequence_end: TextSize,
    ) -> JoinCommaSeparatedBuilder<'fmt, 'ast, 'buf> {
        JoinCommaSeparatedBuilder::new(self, sequence_end)
    }
}

#[derive(Copy, Clone, Debug)]
enum Entries {
    /// No previous entry
    None,
    /// One previous ending at the given position.
    One(TextSize),
    /// More than one entry, the last one ending at the specific position.
    MoreThanOne(TextSize),
}

impl Entries {
    fn position(self) -> Option<TextSize> {
        match self {
            Entries::None => None,
            Entries::One(position) | Entries::MoreThanOne(position) => Some(position),
        }
    }

    const fn is_one_or_more(self) -> bool {
        !matches!(self, Entries::None)
    }

    const fn is_more_than_one(self) -> bool {
        matches!(self, Entries::MoreThanOne(_))
    }

    const fn next(self, end_position: TextSize) -> Self {
        match self {
            Entries::None => Entries::One(end_position),
            Entries::One(_) | Entries::MoreThanOne(_) => Entries::MoreThanOne(end_position),
        }
    }
}

pub(crate) struct JoinCommaSeparatedBuilder<'fmt, 'ast, 'buf> {
    result: FormatResult<()>,
    fmt: &'fmt mut PyFormatter<'ast, 'buf>,
    entries: Entries,
    sequence_end: TextSize,
}

impl<'fmt, 'ast, 'buf> JoinCommaSeparatedBuilder<'fmt, 'ast, 'buf> {
    fn new(f: &'fmt mut PyFormatter<'ast, 'buf>, sequence_end: TextSize) -> Self {
        Self {
            fmt: f,
            result: Ok(()),
            entries: Entries::None,
            sequence_end,
        }
    }

    pub(crate) fn entry<T>(
        &mut self,
        node: &T,
        content: &dyn Format<PyFormatContext<'ast>>,
    ) -> &mut Self
    where
        T: Ranged,
    {
        self.entry_with_line_separator(node, content, soft_line_break_or_space())
    }

    pub(crate) fn entry_with_line_separator<N, Separator>(
        &mut self,
        node: &N,
        content: &dyn Format<PyFormatContext<'ast>>,
        separator: Separator,
    ) -> &mut Self
    where
        N: Ranged,
        Separator: Format<PyFormatContext<'ast>>,
    {
        self.result = self.result.and_then(|_| {
            if self.entries.is_one_or_more() {
                write!(self.fmt, [text(","), separator])?;
            }

            self.entries = self.entries.next(node.end());

            content.fmt(self.fmt)
        });

        self
    }

    #[allow(unused)]
    pub(crate) fn entries<T, I, F>(&mut self, entries: I) -> &mut Self
    where
        T: Ranged,
        F: Format<PyFormatContext<'ast>>,
        I: IntoIterator<Item = (T, F)>,
    {
        for (node, content) in entries {
            self.entry(&node, &content);
        }

        self
    }

    pub(crate) fn nodes<'a, T, I>(&mut self, entries: I) -> &mut Self
    where
        T: Ranged + AsFormat<PyFormatContext<'ast>> + 'a,
        I: IntoIterator<Item = &'a T>,
    {
        for node in entries {
            self.entry(node, &node.format());
        }

        self
    }

    pub(crate) fn finish(&mut self) -> FormatResult<()> {
        self.result.and_then(|_| {
            if let Some(last_end) = self.entries.position() {
                let magic_trailing_comma = match self.fmt.options().magic_trailing_comma() {
                    MagicTrailingComma::Respect => {
                        let first_token = SimpleTokenizer::new(
                            self.fmt.context().source(),
                            TextRange::new(last_end, self.sequence_end),
                        )
                        .skip_trivia()
                        // Skip over any closing parentheses belonging to the expression
                        .find(|token| token.kind() != SimpleTokenKind::RParen);

                        matches!(
                            first_token,
                            Some(SimpleToken {
                                kind: SimpleTokenKind::Comma,
                                ..
                            })
                        )
                    }
                    MagicTrailingComma::Ignore => false,
                };

                // If there is a single entry, only keep the magic trailing comma, don't add it if
                // it wasn't there. If there is more than one entry, always add it.
                if magic_trailing_comma || self.entries.is_more_than_one() {
                    if_group_breaks(&text(",")).fmt(self.fmt)?;
                }

                if magic_trailing_comma {
                    expand_parent().fmt(self.fmt)?;
                }
            }

            Ok(())
        })
    }
}

/// Format comments inside empty parentheses, brackets or curly braces.
///
/// Empty `()`, `[]` and `{}` are special because there can be dangling comments, and they can be in
/// two positions:
/// ```python
/// x = [  # end-of-line
///     # own line
/// ]
/// ```
/// These comments are dangling because they can't be assigned to any element inside as they would
/// in all other cases.
pub(crate) fn empty_parenthesized_with_dangling_comments(
    opening: StaticText,
    comments: &[SourceComment],
    closing: StaticText,
) -> EmptyWithDanglingComments {
    EmptyWithDanglingComments {
        opening,
        comments,
        closing,
    }
}

pub(crate) struct EmptyWithDanglingComments<'a> {
    opening: StaticText,
    comments: &'a [SourceComment],
    closing: StaticText,
}

impl<'ast> Format<PyFormatContext<'ast>> for EmptyWithDanglingComments<'_> {
    fn fmt(&self, f: &mut Formatter<PyFormatContext>) -> FormatResult<()> {
        let end_of_line_split = self
            .comments
            .partition_point(|comment| comment.line_position().is_end_of_line());
        debug_assert!(self.comments[end_of_line_split..]
            .iter()
            .all(|comment| comment.line_position().is_own_line()));
        write!(
            f,
            [group(&format_args![
                self.opening,
                // end-of-line comments
                dangling_comments(&self.comments[..end_of_line_split]),
                // own line comments, which need to be indented
                soft_block_indent(&dangling_comments(&self.comments[end_of_line_split..])),
                self.closing
            ])]
        )
    }
}
