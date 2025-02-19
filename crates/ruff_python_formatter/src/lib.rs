use thiserror::Error;
use tracing::Level;

pub use range::format_range;
use ruff_formatter::prelude::*;
use ruff_formatter::{format, write, FormatError, Formatted, PrintError, Printed, SourceCode};
use ruff_python_ast::{AnyNodeRef, Mod};
use ruff_python_parser::{parse, ParseError, ParseOptions, Parsed};
use ruff_python_trivia::CommentRanges;
use ruff_text_size::Ranged;

use crate::comments::{
    has_skip_comment, leading_comments, trailing_comments, Comments, SourceComment,
};
pub use crate::context::PyFormatContext;
pub use crate::options::{
    DocstringCode, DocstringCodeLineWidth, MagicTrailingComma, PreviewMode, PyFormatOptions,
    QuoteStyle,
};
use crate::range::is_logical_line;
pub use crate::shared_traits::{AsFormat, FormattedIter, FormattedIterExt, IntoFormat};
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
mod preview;
mod range;
mod shared_traits;
pub(crate) mod statement;
pub(crate) mod string;
pub(crate) mod type_param;
mod verbatim;

/// 'ast is the lifetime of the source code (input), 'buf is the lifetime of the buffer (output)
pub(crate) type PyFormatter<'ast, 'buf> = Formatter<'buf, PyFormatContext<'ast>>;

/// Rule for formatting a Python AST node.
pub(crate) trait FormatNodeRule<N>
where
    N: Ranged,
    for<'a> AnyNodeRef<'a>: From<&'a N>,
{
    fn fmt(&self, node: &N, f: &mut PyFormatter) -> FormatResult<()> {
        let comments = f.context().comments().clone();

        let node_ref = AnyNodeRef::from(node);
        let node_comments = comments.leading_dangling_trailing(node_ref);

        if self.is_suppressed(node_comments.trailing, f.context()) {
            suppressed_node(node_ref).fmt(f)
        } else {
            leading_comments(node_comments.leading).fmt(f)?;

            // Emit source map information for nodes that are valid "narrowing" targets
            // in range formatting. Never emit source map information if they're disabled
            // for performance reasons.
            let emit_source_position = (is_logical_line(node_ref) || node_ref.is_mod_module())
                && f.options().source_map_generation().is_enabled();

            emit_source_position
                .then_some(source_position(node.start()))
                .fmt(f)?;

            self.fmt_fields(node, f)?;

            debug_assert!(node_comments.dangling.iter().all(SourceComment::is_formatted), "The node has dangling comments that need to be formatted manually. Add the special dangling comments handling to `fmt_fields`.");

            write!(
                f,
                [
                    emit_source_position.then_some(source_position(node.end())),
                    trailing_comments(node_comments.trailing)
                ]
            )
        }
    }

    /// Formats the node's fields.
    fn fmt_fields(&self, item: &N, f: &mut PyFormatter) -> FormatResult<()>;

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
    #[error(transparent)]
    ParseError(#[from] ParseError),
    #[error(transparent)]
    FormatError(#[from] FormatError),
    #[error(transparent)]
    PrintError(#[from] PrintError),
}

#[tracing::instrument(name = "format", level = Level::TRACE, skip_all)]
pub fn format_module_source(
    source: &str,
    options: PyFormatOptions,
) -> Result<Printed, FormatModuleError> {
    let source_type = options.source_type();
    let parsed = parse(source, ParseOptions::from(source_type))?;
    let comment_ranges = CommentRanges::from(parsed.tokens());
    let formatted = format_module_ast(&parsed, &comment_ranges, source, options)?;
    Ok(formatted.print()?)
}

pub fn format_module_ast<'a>(
    parsed: &'a Parsed<Mod>,
    comment_ranges: &'a CommentRanges,
    source: &'a str,
    options: PyFormatOptions,
) -> FormatResult<Formatted<PyFormatContext<'a>>> {
    let source_code = SourceCode::new(source);
    let comments = Comments::from_ast(parsed.syntax(), source_code, comment_ranges);

    let formatted = format!(
        PyFormatContext::new(options, source, comments, parsed.tokens()),
        [parsed.syntax().format()]
    )?;
    formatted
        .context()
        .comments()
        .assert_all_formatted(source_code);
    Ok(formatted)
}

/// Public function for generating a printable string of the debug comments.
pub fn pretty_comments(module: &Mod, comment_ranges: &CommentRanges, source: &str) -> String {
    let source_code = SourceCode::new(source);
    let comments = Comments::from_ast(module, source_code, comment_ranges);

    std::format!("{comments:#?}", comments = comments.debug(source_code))
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use anyhow::Result;
    use insta::assert_snapshot;

    use ruff_python_ast::PySourceType;
    use ruff_python_parser::{parse, ParseOptions};
    use ruff_python_trivia::CommentRanges;
    use ruff_text_size::{TextRange, TextSize};

    use crate::{format_module_ast, format_module_source, format_range, PyFormatOptions};

    /// Very basic test intentionally kept very similar to the CLI
    #[test]
    fn basic() -> Result<()> {
        let input = r"
# preceding
if    True:
    pass
# trailing
";
        let expected = r"# preceding
if True:
    pass
# trailing
";
        let actual = format_module_source(input, PyFormatOptions::default())?
            .as_code()
            .to_string();
        assert_eq!(expected, actual);
        Ok(())
    }

    /// Use this test to debug the formatting of some snipped
    #[ignore]
    #[test]
    fn quick_test() {
        let source = r#"
def main() -> None:
    if True:
        some_very_long_variable_name_abcdefghijk = Foo()
        some_very_long_variable_name_abcdefghijk = some_very_long_variable_name_abcdefghijk[
            some_very_long_variable_name_abcdefghijk.some_very_long_attribute_name
            == "This is a very long string abcdefghijk"
        ]

"#;
        let source_type = PySourceType::Python;

        // Parse the AST.
        let source_path = "code_inline.py";
        let parsed = parse(source, ParseOptions::from(source_type)).unwrap();
        let comment_ranges = CommentRanges::from(parsed.tokens());
        let options = PyFormatOptions::from_extension(Path::new(source_path));
        let formatted = format_module_ast(&parsed, &comment_ranges, source, options).unwrap();

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
            r"for converter in connection.ops.get_db_converters(
    expression
) + expression.get_db_converters(connection):
    ...
"
        );
    }

    /// Use this test to quickly debug some formatting issue.
    #[ignore]
    #[test]
    fn range_formatting_quick_test() {
        let source = r#"def convert_str(value: str) -> str:  # Trailing comment
    """Return a string as-is."""

<RANGE_START>

    return value  # Trailing comment
<RANGE_END>"#;

        let mut source = source.to_string();

        let start = TextSize::try_from(
            source
                .find("<RANGE_START>")
                .expect("Start marker not found"),
        )
        .unwrap();

        source.replace_range(
            start.to_usize()..start.to_usize() + "<RANGE_START>".len(),
            "",
        );

        let end =
            TextSize::try_from(source.find("<RANGE_END>").expect("End marker not found")).unwrap();

        source.replace_range(end.to_usize()..end.to_usize() + "<RANGE_END>".len(), "");

        let source_type = PySourceType::Python;
        let options = PyFormatOptions::from_source_type(source_type);
        let printed = format_range(&source, TextRange::new(start, end), options).unwrap();

        let mut formatted = source.to_string();
        formatted.replace_range(
            std::ops::Range::<usize>::from(printed.source_range()),
            printed.as_code(),
        );

        assert_eq!(
            formatted,
            r#"print ( "format me" )
print("format me")
print("format me")
print ( "format me" )
print ( "format me" )"#
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
                            write!(f, [text(word)])?;

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
            r"aaaaaaaaaa bbbbbbbbbb cccccccccc dddddddddd eeeeeeeeee ffffffffff gggggggggg h";
        let breaks =
            r"aaaaaaaaaa bbbbbbbbbb cccccccccc dddddddddd eeeeeeeeee ffffffffff gggggggggg hh";

        let output = format!(
            SimpleFormatContext::default(),
            [FormatString(fits), hard_line_break(), FormatString(breaks)]
        )
        .expect("Formatting to succeed");

        assert_snapshot!(output.print().expect("Printing to succeed").as_code());
    }
}
