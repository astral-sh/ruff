use crate::comments::{
    dangling_node_comments, leading_node_comments, trailing_node_comments, Comments,
};
use crate::context::PyFormatContext;
pub use crate::options::{MagicTrailingComma, PyFormatOptions, QuoteStyle};
use ruff_formatter::format_element::tag;
use ruff_formatter::prelude::{
    dynamic_text, source_position, source_text_slice, text, ContainsNewlines, Formatter, Tag,
};
// `write!` aliased for https://github.com/dtolnay/thiserror/issues/239
use ruff_formatter::{
    format, normalize_newlines, write as ruff_write, Buffer, Format, FormatElement, FormatError,
    FormatResult, PrintError,
};
use ruff_formatter::{Formatted, Printed, SourceCode};
use ruff_python_ast::node::{AnyNodeRef, AstNode, NodeKind};
use ruff_python_ast::source_code::{CommentRanges, CommentRangesBuilder, Locator};
use ruff_text_size::{TextLen, TextRange};
use rustpython_parser::ast::{Mod, Ranged};
use rustpython_parser::lexer::{lex, LexicalError};
use rustpython_parser::{parse_tokens, Mode, ParseError};
use std::borrow::Cow;
use thiserror::Error;

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
mod trivia;

include!("../../ruff_formatter/shared_traits.rs");

/// 'ast is the lifetime of the source code (input), 'buf is the lifetime of the buffer (output)
pub(crate) type PyFormatter<'ast, 'buf> = Formatter<'buf, PyFormatContext<'ast>>;

/// Rule for formatting a JavaScript [`AstNode`].
pub(crate) trait FormatNodeRule<N>
where
    N: AstNode,
{
    fn fmt(&self, node: &N, f: &mut PyFormatter) -> FormatResult<()> {
        self.fmt_leading_comments(node, f)?;
        self.fmt_node(node, f)?;
        self.fmt_dangling_comments(node, f)?;
        self.fmt_trailing_comments(node, f)
    }

    /// Formats the node without comments. Ignores any suppression comments.
    fn fmt_node(&self, node: &N, f: &mut PyFormatter) -> FormatResult<()> {
        ruff_write!(f, [source_position(node.start())])?;
        self.fmt_fields(node, f)?;
        ruff_write!(f, [source_position(node.end())])
    }

    /// Formats the node's fields.
    fn fmt_fields(&self, item: &N, f: &mut PyFormatter) -> FormatResult<()>;

    /// Formats the [leading comments](comments#leading-comments) of the node.
    ///
    /// You may want to override this method if you want to manually handle the formatting of comments
    /// inside of the `fmt_fields` method or customize the formatting of the leading comments.
    fn fmt_leading_comments(&self, node: &N, f: &mut PyFormatter) -> FormatResult<()> {
        leading_node_comments(node).fmt(f)
    }

    /// Formats the [dangling comments](comments#dangling-comments) of the node.
    ///
    /// You should override this method if the node handled by this rule can have dangling comments because the
    /// default implementation formats the dangling comments at the end of the node, which isn't ideal but ensures that
    /// no comments are dropped.
    ///
    /// A node can have dangling comments if all its children are tokens or if all node childrens are optional.
    fn fmt_dangling_comments(&self, node: &N, f: &mut PyFormatter) -> FormatResult<()> {
        dangling_node_comments(node).fmt(f)
    }

    /// Formats the [trailing comments](comments#trailing-comments) of the node.
    ///
    /// You may want to override this method if you want to manually handle the formatting of comments
    /// inside of the `fmt_fields` method or customize the formatting of the trailing comments.
    fn fmt_trailing_comments(&self, node: &N, f: &mut PyFormatter) -> FormatResult<()> {
        trailing_node_comments(node).fmt(f)
    }
}

#[derive(Error, Debug)]
pub enum FormatModuleError {
    #[error("source contains syntax errors (lexer error): {0:?}")]
    LexError(LexicalError),
    #[error("source contains syntax errors (parser error): {0:?}")]
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

    format!(
        PyFormatContext::new(options, locator.contents(), comments),
        [root.format()]
    )
}

pub(crate) struct NotYetImplemented(NodeKind);

/// Formats a placeholder for nodes that have not yet been implemented
pub(crate) fn not_yet_implemented<'a, T>(node: T) -> NotYetImplemented
where
    T: Into<AnyNodeRef<'a>>,
{
    NotYetImplemented(node.into().kind())
}

impl Format<PyFormatContext<'_>> for NotYetImplemented {
    fn fmt(&self, f: &mut PyFormatter) -> FormatResult<()> {
        let text = std::format!("NOT_YET_IMPLEMENTED_{:?}", self.0);

        f.write_element(FormatElement::Tag(Tag::StartVerbatim(
            tag::VerbatimKind::Verbatim {
                length: text.text_len(),
            },
        )))?;

        f.write_element(FormatElement::DynamicText {
            text: Box::from(text),
        })?;

        f.write_element(FormatElement::Tag(Tag::EndVerbatim))?;
        Ok(())
    }
}

pub(crate) struct NotYetImplementedCustomText(&'static str);

/// Formats a placeholder for nodes that have not yet been implemented
pub(crate) const fn not_yet_implemented_custom_text(
    text: &'static str,
) -> NotYetImplementedCustomText {
    NotYetImplementedCustomText(text)
}

impl Format<PyFormatContext<'_>> for NotYetImplementedCustomText {
    fn fmt(&self, f: &mut PyFormatter) -> FormatResult<()> {
        f.write_element(FormatElement::Tag(Tag::StartVerbatim(
            tag::VerbatimKind::Verbatim {
                length: self.0.text_len(),
            },
        )))?;

        text(self.0).fmt(f)?;

        f.write_element(FormatElement::Tag(Tag::EndVerbatim))
    }
}

pub(crate) struct VerbatimText(TextRange);

#[allow(unused)]
pub(crate) fn verbatim_text<T>(item: &T) -> VerbatimText
where
    T: Ranged,
{
    VerbatimText(item.range())
}

impl Format<PyFormatContext<'_>> for VerbatimText {
    fn fmt(&self, f: &mut PyFormatter) -> FormatResult<()> {
        f.write_element(FormatElement::Tag(Tag::StartVerbatim(
            tag::VerbatimKind::Verbatim {
                length: self.0.len(),
            },
        )))?;

        match normalize_newlines(f.context().locator().slice(self.0), ['\r']) {
            Cow::Borrowed(_) => {
                ruff_write!(f, [source_text_slice(self.0, ContainsNewlines::Detect)])?;
            }
            Cow::Owned(cleaned) => {
                ruff_write!(
                    f,
                    [
                        dynamic_text(&cleaned, Some(self.0.start())),
                        source_position(self.0.end())
                    ]
                )?;
            }
        }

        f.write_element(FormatElement::Tag(Tag::EndVerbatim))?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use crate::{format_module, format_node, PyFormatOptions};
    use anyhow::Result;
    use insta::assert_snapshot;
    use ruff_python_ast::source_code::CommentRangesBuilder;
    use rustpython_parser::lexer::lex;
    use rustpython_parser::{parse_tokens, Mode};

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
if [
    aaaaaa,
    BBBB,ccccccccc,ddddddd,eeeeeeeeee,ffffff
] & bbbbbbbbbbbbbbbbbbddddddddddddddddddddddddddddbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb:
    ...
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
        let python_ast = parse_tokens(tokens, Mode::Module, "<filename>").unwrap();

        let formatted = format_node(
            &python_ast,
            &comment_ranges,
            src,
            PyFormatOptions::default(),
        )
        .unwrap();

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
            r#"while True:
    if something.changed:
        do.stuff()  # trailing comment
"#
        );
    }

    #[test]
    fn string_processing() {
        use crate::prelude::*;
        use ruff_formatter::{format, format_args, write};

        struct FormatString<'a>(&'a str);

        impl Format<SimpleFormatContext> for FormatString<'_> {
            fn fmt(
                &self,
                f: &mut ruff_formatter::formatter::Formatter<SimpleFormatContext>,
            ) -> FormatResult<()> {
                let format_str = format_with(|f| {
                    write!(f, [text("\"")])?;

                    let mut words = self.0.split_whitespace().peekable();
                    let mut fill = f.fill();

                    let separator = format_with(|f| {
                        group(&format_args![
                            if_group_breaks(&text("\"")),
                            soft_line_break_or_space(),
                            if_group_breaks(&text("\" "))
                        ])
                        .fmt(f)
                    });

                    while let Some(word) = words.next() {
                        let is_last = words.peek().is_none();
                        let format_word = format_with(|f| {
                            write!(f, [dynamic_text(word, None)])?;

                            if is_last {
                                write!(f, [text("\"")])?;
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
                        if_group_breaks(&text("(")),
                        soft_block_indent(&format_str),
                        if_group_breaks(&text(")"))
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
