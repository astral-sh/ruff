use crate::context::NodeLevel;
use crate::prelude::*;
use crate::trivia::{lines_after, skip_trailing_trivia};
use ruff_formatter::write;
use ruff_text_size::TextSize;
use rustpython_parser::ast::Ranged;

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
}

impl<'buf, 'ast> PyFormatterExtensions<'ast, 'buf> for PyFormatter<'ast, 'buf> {
    fn join_nodes<'fmt>(&'fmt mut self, level: NodeLevel) -> JoinNodesBuilder<'fmt, 'ast, 'buf> {
        JoinNodesBuilder::new(self, level)
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

#[cfg(test)]
mod tests {
    use crate::comments::Comments;
    use crate::context::{NodeLevel, PyFormatContext};
    use crate::prelude::*;
    use ruff_formatter::format;
    use ruff_formatter::SimpleFormatOptions;
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

        let context =
            PyFormatContext::new(SimpleFormatOptions::default(), source, Comments::default());

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
            r#"a = 0x42


three_leading_newlines = 0x42


two_leading_newlines = 0x42

one_leading_newline = 0x42
no_leading_newline = 0x42"#
        );
    }

    // Should keep at most one empty level
    #[test]
    fn ranged_builder_statement_level() {
        let printed = format_ranged(NodeLevel::CompoundStatement);

        assert_eq!(
            &printed,
            r#"a = 0x42

three_leading_newlines = 0x42

two_leading_newlines = 0x42

one_leading_newline = 0x42
no_leading_newline = 0x42"#
        );
    }

    // Removes all empty lines
    #[test]
    fn ranged_builder_parenthesized_level() {
        let printed = format_ranged(NodeLevel::Expression);

        assert_eq!(
            &printed,
            r#"a = 0x42
three_leading_newlines = 0x42
two_leading_newlines = 0x42
one_leading_newline = 0x42
no_leading_newline = 0x42"#
        );
    }
}
