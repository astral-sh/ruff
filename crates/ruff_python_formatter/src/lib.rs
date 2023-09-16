use thiserror::Error;
use tracing::Level;

use ruff_formatter::prelude::*;
use ruff_formatter::{format, FormatError, Formatted, PrintError, Printed, SourceCode};
use ruff_python_ast::node::AstNode;
use ruff_python_ast::Mod;
use ruff_python_index::{CommentRanges, CommentRangesBuilder};
use ruff_python_parser::lexer::{lex, LexicalError};
use ruff_python_parser::{parse_tokens, Mode, ParseError};
use ruff_source_file::Locator;

use crate::comments::{
    dangling_comments, leading_comments, trailing_comments, Comments, SourceComment,
};
pub use crate::context::PyFormatContext;
pub use crate::options::{MagicTrailingComma, PreviewMode, PyFormatOptions, QuoteStyle};
use crate::verbatim::suppressed_node;

pub(crate) mod builders;
pub mod cli;
mod comments;
pub(crate) mod context;
pub(crate) mod expression;
mod generated;
pub(crate) mod module;
mod options;
pub(crate) mod other;
pub(crate) mod pattern;
mod prelude;
pub(crate) mod statement;
pub(crate) mod type_param;
mod verbatim;

include!("../../ruff_formatter/shared_traits.rs");

/// 'ast is the lifetime of the source code (input), 'buf is the lifetime of the buffer (output)
pub(crate) type PyFormatter<'ast, 'buf> = Formatter<'buf, PyFormatContext<'ast>>;

/// Rule for formatting a Python [`AstNode`].
pub(crate) trait FormatNodeRule<N>
where
    N: AstNode,
{
    fn fmt(&self, node: &N, f: &mut PyFormatter) -> FormatResult<()> {
        let comments = f.context().comments().clone();

        let node_comments = comments.leading_dangling_trailing(node.as_any_node_ref());

        if self.is_suppressed(node_comments.trailing, f.context()) {
            suppressed_node(node.as_any_node_ref()).fmt(f)
        } else {
            leading_comments(node_comments.leading).fmt(f)?;

            let is_source_map_enabled = f.options().source_map_generation().is_enabled();

            if is_source_map_enabled {
                source_position(node.start()).fmt(f)?;
            }

            self.fmt_fields(node, f)?;
            self.fmt_dangling_comments(node_comments.dangling, f)?;

            if is_source_map_enabled {
                source_position(node.end()).fmt(f)?;
            }

            trailing_comments(node_comments.trailing).fmt(f)
        }
    }

    /// Formats the node's fields.
    fn fmt_fields(&self, item: &N, f: &mut PyFormatter) -> FormatResult<()>;

    /// Formats the [dangling comments](comments#dangling-comments) of the node.
    ///
    /// You should override this method if the node handled by this rule can have dangling comments because the
    /// default implementation formats the dangling comments at the end of the node, which isn't ideal but ensures that
    /// no comments are dropped.
    ///
    /// A node can have dangling comments if all its children are tokens or if all node children are optional.
    fn fmt_dangling_comments(
        &self,
        dangling_node_comments: &[SourceComment],
        f: &mut PyFormatter,
    ) -> FormatResult<()> {
        dangling_comments(dangling_node_comments).fmt(f)
    }

    fn is_suppressed(
        &self,
        _trailing_comments: &[SourceComment],
        _context: &PyFormatContext,
    ) -> bool {
        false
    }
}

#[derive(Error, Debug)]
pub enum FormatModuleError {
    #[error("source contains syntax errors: {0:?}")]
    LexError(LexicalError),
    #[error("source contains syntax errors: {0:?}")]
    ParseError(ParseError),
    #[error(transparent)]
    FormatError(#[from] FormatError),
    #[error(transparent)]
    PrintError(#[from] PrintError),
}

impl From<LexicalError> for FormatModuleError {
    fn from(value: LexicalError) -> Self {
        Self::LexError(value)
    }
}

impl From<ParseError> for FormatModuleError {
    fn from(value: ParseError) -> Self {
        Self::ParseError(value)
    }
}

#[tracing::instrument(level=Level::TRACE, skip_all)]
pub fn format_module(
    contents: &str,
    options: PyFormatOptions,
) -> Result<Printed, FormatModuleError> {
    // Tokenize once
    let mut tokens = Vec::new();
    let mut comment_ranges = CommentRangesBuilder::default();

    for result in lex(contents, Mode::Module) {
        let (token, range) = result?;

        comment_ranges.visit_token(&token, range);
        tokens.push(Ok((token, range)));
    }

    let comment_ranges = comment_ranges.finish();

    // Parse the AST.
    let python_ast = parse_tokens(tokens, Mode::Module, "<filename>")?;

    let formatted = format_node(&python_ast, &comment_ranges, contents, options)?;

    Ok(formatted.print()?)
}

pub fn format_node<'a>(
    root: &'a Mod,
    comment_ranges: &'a CommentRanges,
    source: &'a str,
    options: PyFormatOptions,
) -> FormatResult<Formatted<PyFormatContext<'a>>> {
    let comments = Comments::from_ast(root, SourceCode::new(source), comment_ranges);

    let locator = Locator::new(source);

    let formatted = format!(
        PyFormatContext::new(options, locator.contents(), comments),
        [root.format()]
    )?;
    formatted
        .context()
        .comments()
        .assert_all_formatted(SourceCode::new(source));
    Ok(formatted)
}

/// Public function for generating a printable string of the debug comments.
pub fn pretty_comments(root: &Mod, comment_ranges: &CommentRanges, source: &str) -> String {
    let comments = Comments::from_ast(root, SourceCode::new(source), comment_ranges);

    std::format!(
        "{comments:#?}",
        comments = comments.debug(SourceCode::new(source))
    )
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use anyhow::Result;
    use insta::assert_snapshot;

    use ruff_python_index::CommentRangesBuilder;
    use ruff_python_parser::lexer::lex;
    use ruff_python_parser::{parse_tokens, Mode};

    use crate::{format_module, format_node, PyFormatOptions};

    /// Very basic test intentionally kept very similar to the CLI
    #[test]
    fn basic() -> Result<()> {
        let input = r#"
# preceding
if    True:
    pass
# trailing
"#;
        let expected = r#"# preceding
if True:
    pass
# trailing
"#;
        let actual = format_module(input, PyFormatOptions::default())?
            .as_code()
            .to_string();
        assert_eq!(expected, actual);
        Ok(())
    }

    /// Use this test to debug the formatting of some snipped
    #[ignore]
    #[test]
    fn quick_test() {
        let src = r#"
(header.timecnt * 5  # Transition times and types
  + header.typecnt * 6  # Local time type records
  + header.charcnt  # Time zone designations
  + header.leapcnt * 8  # Leap second records
  + header.isstdcnt  # Standard/wall indicators
  + header.isutcnt)  # UT/local indicators
"#;
        // Tokenize once
        let mut tokens = Vec::new();
        let mut comment_ranges = CommentRangesBuilder::default();

        for result in lex(src, Mode::Module) {
            let (token, range) = result.unwrap();
            comment_ranges.visit_token(&token, range);
            tokens.push(Ok((token, range)));
        }

        let comment_ranges = comment_ranges.finish();

        // Parse the AST.
        let source_path = "code_inline.py";
        let python_ast = parse_tokens(tokens, Mode::Module, source_path).unwrap();
        let options = PyFormatOptions::from_extension(Path::new(source_path));
        let formatted = format_node(&python_ast, &comment_ranges, src, options).unwrap();

        // Uncomment the `dbg` to print the IR.
        // Use `dbg_write!(f, []) instead of `write!(f, [])` in your formatting code to print some IR
        // inside of a `Format` implementation
        // use ruff_formatter::FormatContext;
        // dbg!(formatted
        //     .document()
        //     .display(formatted.context().source_code()));
        //
        // dbg!(formatted
        //     .context()
        //     .comments()
        //     .debug(formatted.context().source_code()));

        let printed = formatted.print().unwrap();

        assert_eq!(
            printed.as_code(),
            r#"for converter in connection.ops.get_db_converters(
    expression
) + expression.get_db_converters(connection):
    ...
"#
        );
    }

    #[test]
    fn string_processing() {
        use crate::prelude::*;
        use ruff_formatter::{format, format_args, write};

        struct FormatString<'a>(&'a str);

        impl Format<SimpleFormatContext> for FormatString<'_> {
            fn fmt(&self, f: &mut Formatter<SimpleFormatContext>) -> FormatResult<()> {
                let format_str = format_with(|f| {
                    write!(f, [token("\"")])?;

                    let mut words = self.0.split_whitespace().peekable();
                    let mut fill = f.fill();

                    let separator = format_with(|f| {
                        group(&format_args![
                            if_group_breaks(&token("\"")),
                            soft_line_break_or_space(),
                            if_group_breaks(&token("\" "))
                        ])
                        .fmt(f)
                    });

                    while let Some(word) = words.next() {
                        let is_last = words.peek().is_none();
                        let format_word = format_with(|f| {
                            write!(f, [text(word, None)])?;

                            if is_last {
                                write!(f, [token("\"")])?;
                            }

                            Ok(())
                        });

                        fill.entry(&separator, &format_word);
                    }

                    fill.finish()
                });

                write!(
                    f,
                    [group(&format_args![
                        if_group_breaks(&token("(")),
                        soft_block_indent(&format_str),
                        if_group_breaks(&token(")"))
                    ])]
                )
            }
        }

        // 77 after g group (leading quote)
        let fits =
            r#"aaaaaaaaaa bbbbbbbbbb cccccccccc dddddddddd eeeeeeeeee ffffffffff gggggggggg h"#;
        let breaks =
            r#"aaaaaaaaaa bbbbbbbbbb cccccccccc dddddddddd eeeeeeeeee ffffffffff gggggggggg hh"#;

        let output = format!(
            SimpleFormatContext::default(),
            [FormatString(fits), hard_line_break(), FormatString(breaks)]
        )
        .expect("Formatting to succeed");

        assert_snapshot!(output.print().expect("Printing to succeed").as_code());
    }
}
