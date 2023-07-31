use ruff_python_ast::Ranged;
use ruff_text_size::{TextRange, TextSize};

use crate::comments::{dangling_comments, SourceComment};
use ruff_formatter::{format_args, write, Argument, Arguments};
use ruff_python_trivia::{
    lines_after, skip_trailing_trivia, SimpleToken, SimpleTokenKind, SimpleTokenizer,
};

use crate::context::NodeLevel;
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
        let saved_level = f.context().node_level();

        f.context_mut()
            .set_node_level(NodeLevel::ParenthesizedExpression);

        let result = group(&format_args![
            if_group_breaks(&text("(")),
            soft_block_indent(&Arguments::from(&self.inner)),
            if_group_breaks(&text(")")),
        ])
        .fmt(f);

        f.context_mut().set_node_level(saved_level);

        result
    }
}

/// Provides Python specific extensions to [`Formatter`].
pub(crate) trait PyFormatterExtensions<'ast, 'buf> {
    /// Creates a joiner that inserts the appropriate number of empty lines between two nodes, depending on the
    /// line breaks that separate the two nodes in the source document. The `level` customizes the maximum allowed
    /// empty lines between any two nodes. Separates any two nodes by at least a hard line break.
    ///
    /// * [`NodeLevel::Module`]: Up to two empty lines
    /// * [`NodeLevel::CompoundStatement`]: Up to one empty line
    /// * [`NodeLevel::Expression`]: No empty lines
    fn join_nodes<'fmt>(&'fmt mut self, level: NodeLevel) -> JoinNodesBuilder<'fmt, 'ast, 'buf>;

    /// A builder that separates each element by a `,` and a [`soft_line_break_or_space`].
    /// It emits a trailing `,` that is only shown if the enclosing group expands. It forces the enclosing
    /// group to expand if the last item has a trailing `comma` and the magical comma option is enabled.
    fn join_comma_separated<'fmt>(
        &'fmt mut self,
        sequence_end: TextSize,
    ) -> JoinCommaSeparatedBuilder<'fmt, 'ast, 'buf>;
}

impl<'buf, 'ast> PyFormatterExtensions<'ast, 'buf> for PyFormatter<'ast, 'buf> {
    fn join_nodes<'fmt>(&'fmt mut self, level: NodeLevel) -> JoinNodesBuilder<'fmt, 'ast, 'buf> {
        JoinNodesBuilder::new(self, level)
    }

    fn join_comma_separated<'fmt>(
        &'fmt mut self,
        sequence_end: TextSize,
    ) -> JoinCommaSeparatedBuilder<'fmt, 'ast, 'buf> {
        JoinCommaSeparatedBuilder::new(self, sequence_end)
    }
}

#[must_use = "must eventually call `finish()` on the builder."]
pub(crate) struct JoinNodesBuilder<'fmt, 'ast, 'buf> {
    fmt: &'fmt mut PyFormatter<'ast, 'buf>,
    result: FormatResult<()>,
    last_end: Option<TextSize>,
    node_level: NodeLevel,
}

impl<'fmt, 'ast, 'buf> JoinNodesBuilder<'fmt, 'ast, 'buf> {
    fn new(fmt: &'fmt mut PyFormatter<'ast, 'buf>, level: NodeLevel) -> Self {
        Self {
            fmt,
            result: Ok(()),
            last_end: None,
            node_level: level,
        }
    }

    /// Writes a `node`, inserting the appropriate number of line breaks depending on the number of
    /// line breaks that were present in the source document. Uses `content` to format the `node`.
    pub(crate) fn entry<T>(&mut self, node: &T, content: &dyn Format<PyFormatContext<'ast>>)
    where
        T: Ranged,
    {
        let node_level = self.node_level;

        self.result = self.result.and_then(|_| {
            if let Some(last_end) = self.last_end.replace(node.end()) {
                let source = self.fmt.context().source();
                let count_lines = |offset| {
                    // It's necessary to skip any trailing line comment because RustPython doesn't include trailing comments
                    // in the node's range
                    // ```python
                    // a # The range of `a` ends right before this comment
                    //
                    // b
                    // ```
                    //
                    // Simply using `lines_after` doesn't work if a statement has a trailing comment because
                    // it then counts the lines between the statement and the trailing comment, which is
                    // always 0. This is why it skips any trailing trivia (trivia that's on the same line)
                    // and counts the lines after.
                    let after_trailing_trivia = skip_trailing_trivia(offset, source);
                    lines_after(after_trailing_trivia, source)
                };

                match node_level {
                    NodeLevel::TopLevel => match count_lines(last_end) {
                        0 | 1 => hard_line_break().fmt(self.fmt),
                        2 => empty_line().fmt(self.fmt),
                        _ => write!(self.fmt, [empty_line(), empty_line()]),
                    },
                    NodeLevel::CompoundStatement => match count_lines(last_end) {
                        0 | 1 => hard_line_break().fmt(self.fmt),
                        _ => empty_line().fmt(self.fmt),
                    },
                    NodeLevel::Expression(_) | NodeLevel::ParenthesizedExpression => {
                        hard_line_break().fmt(self.fmt)
                    }
                }?;
            }

            content.fmt(self.fmt)
        });
    }

    /// Writes a sequence of node with their content tuples, inserting the appropriate number of line breaks between any two of them
    /// depending on the number of line breaks that exist in the source document.
    #[allow(unused)]
    pub(crate) fn entries<T, F, I>(&mut self, entries: I) -> &mut Self
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

    /// Writes a sequence of nodes, using their [`AsFormat`] implementation to format the content.
    /// Inserts the appropriate number of line breaks between any two nodes, depending on the number of
    /// line breaks in the source document.
    #[allow(unused)]
    pub(crate) fn nodes<'a, T, I>(&mut self, nodes: I) -> &mut Self
    where
        T: Ranged + AsFormat<PyFormatContext<'ast>> + 'a,
        I: IntoIterator<Item = &'a T>,
    {
        for node in nodes {
            self.entry(node, &node.format());
        }

        self
    }

    /// Writes a single entry using the specified separator to separate the entry from a previous entry.
    pub(crate) fn entry_with_separator<T>(
        &mut self,
        separator: &dyn Format<PyFormatContext<'ast>>,
        content: &dyn Format<PyFormatContext<'ast>>,
        node: &T,
    ) where
        T: Ranged,
    {
        self.result = self.result.and_then(|_| {
            if self.last_end.is_some() {
                separator.fmt(self.fmt)?;
            }

            self.last_end = Some(node.end());

            content.fmt(self.fmt)
        });
    }

    /// Finishes the joiner and gets the format result.
    pub(crate) fn finish(&mut self) -> FormatResult<()> {
        self.result
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

#[cfg(test)]
mod tests {
    use ruff_python_ast::ModModule;
    use ruff_python_parser::Parse;

    use ruff_formatter::format;

    use crate::comments::Comments;
    use crate::context::{NodeLevel, PyFormatContext};
    use crate::prelude::*;
    use crate::PyFormatOptions;

    fn format_ranged(level: NodeLevel) -> String {
        let source = r#"
a = 10



three_leading_newlines = 80


two_leading_newlines = 20

one_leading_newline = 10
no_leading_newline = 30
"#;

        let module = ModModule::parse(source, "test.py").unwrap();

        let context = PyFormatContext::new(PyFormatOptions::default(), source, Comments::default());

        let test_formatter =
            format_with(|f: &mut PyFormatter| f.join_nodes(level).nodes(&module.body).finish());

        let formatted = format!(context, [test_formatter]).unwrap();
        let printed = formatted.print().unwrap();

        printed.as_code().to_string()
    }

    // Keeps up to two empty lines
    #[test]
    fn ranged_builder_top_level() {
        let printed = format_ranged(NodeLevel::TopLevel);

        assert_eq!(
            &printed,
            r#"a = 10


three_leading_newlines = 80


two_leading_newlines = 20

one_leading_newline = 10
no_leading_newline = 30"#
        );
    }

    // Should keep at most one empty level
    #[test]
    fn ranged_builder_statement_level() {
        let printed = format_ranged(NodeLevel::CompoundStatement);

        assert_eq!(
            &printed,
            r#"a = 10

three_leading_newlines = 80

two_leading_newlines = 20

one_leading_newline = 10
no_leading_newline = 30"#
        );
    }

    // Removes all empty lines
    #[test]
    fn ranged_builder_parenthesized_level() {
        let printed = format_ranged(NodeLevel::Expression(None));

        assert_eq!(
            &printed,
            r#"a = 10
three_leading_newlines = 80
two_leading_newlines = 20
one_leading_newline = 10
no_leading_newline = 30"#
        );
    }
}
