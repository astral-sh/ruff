use std::iter;
use std::str::FromStr;
use thiserror::Error;
use tracing::{warn, Level};

use ruff_formatter::prelude::*;
use ruff_formatter::{format, FormatError, Formatted, PrintError, Printed, SourceCode};
use ruff_python_ast::node::{AnyNodeRef, AstNode};
use ruff_python_ast::{
    Mod, Stmt, StmtClassDef, StmtFor, StmtFunctionDef, StmtIf, StmtWhile, StmtWith,
};
use ruff_python_index::tokens_and_ranges;
use ruff_python_parser::lexer::LexicalError;
use ruff_python_parser::{parse_ok_tokens, Mode, ParseError};
use ruff_python_trivia::CommentRanges;
use ruff_source_file::Locator;
use ruff_text_size::{Ranged, TextLen, TextRange, TextSize};

use crate::comments::{
    dangling_comments, leading_comments, trailing_comments, Comments, SourceComment,
};
pub use crate::context::PyFormatContext;
pub use crate::options::{MagicTrailingComma, PreviewMode, PyFormatOptions, QuoteStyle};
use crate::statement::suite::SuiteKind;
use crate::verbatim::suppressed_node;
pub use settings::FormatterSettings;

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
mod settings;
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

#[tracing::instrument(name = "format", level = Level::TRACE, skip_all)]
pub fn format_module_source(
    source: &str,
    options: PyFormatOptions,
) -> Result<Printed, FormatModuleError> {
    let (tokens, comment_ranges) = tokens_and_ranges(source)?;
    let module = parse_ok_tokens(tokens, Mode::Module, "<filename>")?;
    let formatted = format_module_ast(&module, &comment_ranges, source, options)?;
    Ok(formatted.print()?)
}

/// Range formatting coordinate: Zero-indexed row and zero-indexed char-based column separated by
/// colon, e.g. `1:2`.
///
/// See [`Locator::convert_row_and_column`] for details on the semantics.
#[derive(Copy, Clone, Debug, Default)]
pub struct LspRowColumn {
    row: usize,
    col: usize,
}

impl FromStr for LspRowColumn {
    type Err = &'static str;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let Some((row, col)) = s.split_once(':') else {
            return Err("Coordinate is missing a colon, the format is `<row>:<column>`");
        };

        Ok(LspRowColumn {
            row: row.parse().map_err(|_| "row must be a number")?,
            col: col.parse().map_err(|_| "col must be a number")?,
        })
    }
}
#[tracing::instrument(name = "format", level = Level::TRACE, skip_all)]
pub fn format_module_source_range(
    source: &str,
    options: PyFormatOptions,
    start: Option<LspRowColumn>,
    end: Option<LspRowColumn>,
) -> Result<String, FormatModuleError> {
    let (tokens, comment_ranges) = tokens_and_ranges(source)?;
    let module = parse_ok_tokens(tokens, Mode::Module, "<filename>")?;
    let locator = Locator::new(source);

    let start = if let Some(start) = start {
        locator
            .convert_row_and_column(start.row, start.col)
            .ok_or(FormatError::RangeError {
                row: start.row,
                col: start.col,
            })?
    } else {
        TextSize::default()
    };
    let end = if let Some(end) = end {
        locator
            .convert_row_and_column(end.row, end.col)
            .ok_or(FormatError::RangeError {
                row: end.row,
                col: end.col,
            })?
    } else {
        source.text_len()
    };

    let formatted = format_module_range(
        &module,
        &comment_ranges,
        source,
        options,
        &locator,
        TextRange::new(start, end),
    )?;
    Ok(formatted)
}

pub fn format_module_ast<'a>(
    module: &'a Mod,
    comment_ranges: &'a CommentRanges,
    source: &'a str,
    options: PyFormatOptions,
) -> FormatResult<Formatted<PyFormatContext<'a>>> {
    let source_code = SourceCode::new(source);
    let comments = Comments::from_ast(module, source_code, comment_ranges);
    let locator = Locator::new(source);

    let formatted = format!(
        PyFormatContext::new(options, locator.contents(), comments),
        [module.format()]
    )?;
    formatted
        .context()
        .comments()
        .assert_all_formatted(source_code);
    Ok(formatted)
}

pub fn format_module_range<'a>(
    module: &'a Mod,
    comment_ranges: &'a CommentRanges,
    source: &'a str,
    options: PyFormatOptions,
    locator: &Locator<'a>,
    range: TextRange,
) -> FormatResult<String> {
    let comments = Comments::from_ast(&module, SourceCode::new(source), &comment_ranges);

    let Mod::Module(module_inner) = &module else {
        panic!("That's not a module");
    };

    // TODO: Move this to LspRowColumn? we first count chars to then discard that anyway
    // Consider someone wanted to format `print(i); print(j)`. This wouldn't work indent-wise, so
    // we always do whole lines instead which means we can count indentation normally
    // ```python
    // if True:
    //     for i in range(10): j=i+1; print(i); print(j)
    // ```
    let range = TextRange::new(
        locator.line_start(range.start()),
        locator.line_end(range.end()),
    );

    // ```
    // a = 1; b = 2; c = 3; d = 4; e = 5
    //             ^ b end  ^ d start
    //          ^^^^^^^^^^^^^^^ range
    //          ^ range start ^ range end
    // ```
    // TODO: If it goes beyond the end of the last stmt or before start, do we need to format
    // the parent?
    // TODO: Change suite formatting so we can use a slice instead
    let mut parent_body = &module_inner.body;
    let mut in_range: Vec<Stmt>;

    let in_range = loop {
        in_range = parent_body
            .into_iter()
            // TODO: check whether these bounds need equality
            .skip_while(|child| range.start() > child.end())
            .take_while(|child| child.start() < range.end())
            .cloned()
            .collect();

        let [single_stmt] = in_range.as_slice() else {
            break in_range;
        };

        match single_stmt {
            Stmt::For(StmtFor { body, .. })
            | Stmt::While(StmtWhile { body, .. })
            | Stmt::With(StmtWith { body, .. })
            | Stmt::FunctionDef(StmtFunctionDef { body, .. })
            | Stmt::ClassDef(StmtClassDef { body, .. }) => {
                // We need to format the header or a trailing comment
                // TODO: ignore trivia
                if range.start() < body.first().unwrap().start()
                    || range.end() > body.last().unwrap().end()
                {
                    break in_range;
                } else {
                    parent_body = &body;
                }
            }
            Stmt::If(StmtIf {
                body,
                elif_else_clauses,
                ..
            }) => {
                if range.start() < body.first().unwrap().start()
                    || range.end()
                        > elif_else_clauses
                            .last()
                            .map(|clause| clause.body.last().unwrap().end())
                            .unwrap_or(body.lsa)
                {
                    break in_range;
                } else if let Some(body) = iter::once(body)
                    .chain(elif_else_clauses.iter().map(|clause| clause.body))
                    .find(|body| {
                        body.first().unwrap().start() <= range.start()
                            && range.end() <= body.last().unwrap().end()
                    })
                {
                    in_range = body.clone();
                } else {
                    break in_range;
                }
            }
            // | Stmt::StmtTry(ast::StmtTry { body, .. })
            // | Stmt::ExceptHandlerExceptHandler(ast::ExceptHandlerExceptHandler { body, .. })
            // | Stmt::ElifElseClause(ast::ElifElseClause { body, .. }) => &body,
            // match
            _ => break in_range,
        }
    };

    let (Some(first), Some(last)) = (in_range.first(), in_range.last()) else {
        // TODO: Use tracing again https://github.com/tokio-rs/tracing/issues/2721
        // TODO: Forward this to something proper
        eprintln!("The formatting range contains no statements");
        return Ok(source.to_string());
    };

    let mut buffer = source[TextRange::up_to(first.start())].to_string();

    let formatted: Formatted<PyFormatContext> = format!(
        PyFormatContext::new(options.clone(), locator.contents(), comments),
        [in_range.format().with_options(SuiteKind::TopLevel)]
    )?;
    //println!("{}", formatted.document().display(SourceCode::new(source)));
    // TODO: Make the printer use the buffer instead
    buffer += formatted.print_with_indent(1)?.as_code();
    buffer += &source[TextRange::new(last.end(), source.text_len())];
    return Ok(buffer.to_string());
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

    use ruff_python_index::tokens_and_ranges;

    use ruff_python_parser::{parse_ok_tokens, Mode};

    use crate::{format_module_ast, format_module_source, PyFormatOptions};

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
        let (tokens, comment_ranges) = tokens_and_ranges(source).unwrap();

        // Parse the AST.
        let source_path = "code_inline.py";
        let module = parse_ok_tokens(tokens, Mode::Module, source_path).unwrap();
        let options = PyFormatOptions::from_extension(Path::new(source_path));
        let formatted = format_module_ast(&module, &comment_ranges, source, options).unwrap();

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
