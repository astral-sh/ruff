use crate::context::NodeLevel;
use crate::prelude::*;
use crate::trivia::{first_non_trivia_token, lines_after, skip_trailing_trivia, Token, TokenKind};
use ruff_formatter::{format_args, write, Argument, Arguments};
use ruff_text_size::TextSize;
use rustpython_parser::ast::Ranged;

/// Adds parentheses and indents `content` if it doesn't fit on a line.
pub(crate) fn optional_parentheses<'ast, T>(content: &T) -> OptionalParentheses<'_, 'ast>
where
    T: Format<PyFormatContext<'ast>>,
{
    OptionalParentheses {
        inner: Argument::new(content),
    }
}

pub(crate) struct OptionalParentheses<'a, 'ast> {
    inner: Argument<'a, PyFormatContext<'ast>>,
}

impl<'ast> Format<PyFormatContext<'ast>> for OptionalParentheses<'_, 'ast> {
    fn fmt(&self, f: &mut Formatter<PyFormatContext<'ast>>) -> FormatResult<()> {
        group(&format_args![
            if_group_breaks(&text("(")),
            soft_block_indent(&Arguments::from(&self.inner)),
            if_group_breaks(&text(")"))
        ])
        .fmt(f)
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
    fn join_comma_separated<'fmt>(&'fmt mut self) -> JoinCommaSeparatedBuilder<'fmt, 'ast, 'buf>;
}

impl<'buf, 'ast> PyFormatterExtensions<'ast, 'buf> for PyFormatter<'ast, 'buf> {
    fn join_nodes<'fmt>(&'fmt mut self, level: NodeLevel) -> JoinNodesBuilder<'fmt, 'ast, 'buf> {
        JoinNodesBuilder::new(self, level)
    }

    fn join_comma_separated<'fmt>(&'fmt mut self) -> JoinCommaSeparatedBuilder<'fmt, 'ast, 'buf> {
        JoinCommaSeparatedBuilder::new(self)
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
                let source = self.fmt.context().contents();
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
                    NodeLevel::Expression => hard_line_break().fmt(self.fmt),
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

pub(crate) struct JoinCommaSeparatedBuilder<'fmt, 'ast, 'buf> {
    result: FormatResult<()>,
    fmt: &'fmt mut PyFormatter<'ast, 'buf>,
    end_of_last_entry: Option<TextSize>,
    /// We need to track whether we have more than one entry since a sole entry doesn't get a
    /// magic trailing comma even when expanded
    len: usize,
}

impl<'fmt, 'ast, 'buf> JoinCommaSeparatedBuilder<'fmt, 'ast, 'buf> {
    fn new(f: &'fmt mut PyFormatter<'ast, 'buf>) -> Self {
        Self {
            fmt: f,
            result: Ok(()),
            end_of_last_entry: None,
            len: 0,
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
        self.result = self.result.and_then(|_| {
            if self.end_of_last_entry.is_some() {
                write!(self.fmt, [text(","), soft_line_break_or_space()])?;
            }

            self.end_of_last_entry = Some(node.end());
            self.len += 1;

            content.fmt(self.fmt)
        });

        self
    }

    #[allow(unused)]
    pub(crate) fn entries<T, I, F>(&mut self, entries: I) -> &mut Self
    where
        T: Ranged,
        F: Format<PyFormatContext<'ast>>,
        I: Iterator<Item = (T, F)>,
    {
        for (node, content) in entries {
            self.entry(&node, &content);
        }

        self
    }

    pub(crate) fn nodes<'a, T, I>(&mut self, entries: I) -> &mut Self
    where
        T: Ranged + AsFormat<PyFormatContext<'ast>> + 'a,
        I: Iterator<Item = &'a T>,
    {
        for node in entries {
            self.entry(node, &node.format());
        }

        self
    }

    pub(crate) fn finish(&mut self) -> FormatResult<()> {
        self.result.and_then(|_| {
            if let Some(last_end) = self.end_of_last_entry.take() {
                let magic_trailing_comma = self.fmt.options().magic_trailing_comma().is_respect()
                    && matches!(
                        first_non_trivia_token(last_end, self.fmt.context().contents()),
                        Some(Token {
                            kind: TokenKind::Comma,
                            ..
                        })
                    );

                // If there is a single entry, only keep the magic trailing comma, don't add it if
                // it wasn't there. If there is more than one entry, always add it.
                if magic_trailing_comma || self.len > 1 {
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

#[cfg(test)]
mod tests {
    use crate::comments::Comments;
    use crate::context::{NodeLevel, PyFormatContext};
    use crate::prelude::*;
    use crate::PyFormatOptions;
    use ruff_formatter::format;
    use rustpython_parser::ast::ModModule;
    use rustpython_parser::Parse;

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
        let printed = format_ranged(NodeLevel::Expression);

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
