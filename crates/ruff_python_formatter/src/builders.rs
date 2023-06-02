use crate::context::NodeLevel;
use crate::prelude::*;
use crate::trivia::lines_before;
use ruff_formatter::write;
use rustpython_parser::ast::Ranged;

/// Provides Python specific extensions to [`Formatter`].
pub(crate) trait PyFormatterExtensions<'context, 'buf> {
    /// Creates a joiner that inserts the appropriate number of empty lines between two nodes, depending on the
    /// line breaks that separate the two nodes in the source document. The `level` customizes the maximum allowed
    /// empty lines between any two nodes. Separates any two nodes by at least a hard line break.
    ///
    /// * [`NodeLevel::Module`]: Up to two empty lines
    /// * [`NodeLevel::Statement`]: Up to one empty line
    /// * [`NodeLevel::Parenthesized`]: No empty lines
    fn join_nodes<'fmt>(&'fmt mut self, level: NodeLevel)
        -> JoinNodesBuilder<'fmt, 'context, 'buf>;
}

impl<'buf, 'context> PyFormatterExtensions<'context, 'buf> for PyFormatter<'context, 'buf> {
    fn join_nodes<'fmt>(
        &'fmt mut self,
        level: NodeLevel,
    ) -> JoinNodesBuilder<'fmt, 'context, 'buf> {
        JoinNodesBuilder::new(self, level)
    }
}

#[must_use = "must eventually call `finish()` on the builder."]
pub(crate) struct JoinNodesBuilder<'fmt, 'context, 'buf> {
    fmt: &'fmt mut PyFormatter<'context, 'buf>,
    result: FormatResult<()>,
    has_elements: bool,
    node_level: NodeLevel,
}

impl<'fmt, 'context, 'buf> JoinNodesBuilder<'fmt, 'context, 'buf> {
    fn new(fmt: &'fmt mut PyFormatter<'context, 'buf>, level: NodeLevel) -> Self {
        Self {
            fmt,
            result: Ok(()),
            has_elements: false,
            node_level: level,
        }
    }

    /// Writes a `node`, inserting the appropriate number of line breaks depending on the number of
    /// line breaks that were present in the source document. Uses `content` to format the `node`.
    pub(crate) fn entry<T>(&mut self, node: &T, content: &dyn Format<PyFormatContext<'context>>)
    where
        T: Ranged,
    {
        let node_level = self.node_level;
        let separator = format_with(|f: &mut PyFormatter| match node_level {
            NodeLevel::TopLevel => match lines_before(f.context().contents(), node.start()) {
                0 | 1 => hard_line_break().fmt(f),
                2 => empty_line().fmt(f),
                _ => write!(f, [empty_line(), empty_line()]),
            },
            NodeLevel::Statement => match lines_before(f.context().contents(), node.start()) {
                0 | 1 => hard_line_break().fmt(f),
                _ => empty_line().fmt(f),
            },
            NodeLevel::Parenthesized => hard_line_break().fmt(f),
        });

        self.entry_with_separator(&separator, content);
    }

    /// Writes a sequence of node with their content tuples, inserting the appropriate number of line breaks between any two of them
    /// depending on the number of line breaks that exist in the source document.
    #[allow(unused)]
    pub(crate) fn entries<T, F, I>(&mut self, entries: I) -> &mut Self
    where
        T: Ranged,
        F: Format<PyFormatContext<'context>>,
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
        T: Ranged + AsFormat<PyFormatContext<'context>> + 'a,
        I: IntoIterator<Item = &'a T>,
    {
        for node in nodes {
            self.entry(node, &node.format());
        }

        self
    }

    /// Writes a single entry using the specified separator to separate the entry from a previous entry.
    pub(crate) fn entry_with_separator(
        &mut self,
        separator: &dyn Format<PyFormatContext<'context>>,
        content: &dyn Format<PyFormatContext<'context>>,
    ) {
        self.result = self.result.and_then(|_| {
            if self.has_elements {
                separator.fmt(self.fmt)?;
            }

            self.has_elements = true;

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
        let printed = format_ranged(NodeLevel::Statement);

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
        let printed = format_ranged(NodeLevel::Parenthesized);

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
