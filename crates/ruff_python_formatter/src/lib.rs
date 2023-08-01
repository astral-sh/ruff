use crate::comments::{
    dangling_node_comments, leading_node_comments, trailing_node_comments, Comments,
};
use crate::context::PyFormatContext;
pub use crate::options::{MagicTrailingComma, PyFormatOptions, QuoteStyle};
use ruff_formatter::format_element::{tag, LineMode};
use ruff_formatter::prelude::tag::LabelDefinition;
use ruff_formatter::prelude::{
    dynamic_text, hard_line_break, source_position, source_text_slice, text, ContainsNewlines,
    Document, Formatter, LabelId, Tag,
};
use ruff_formatter::{
    format, normalize_newlines, write, Buffer, Format, FormatContext, FormatElement, FormatError,
    FormatOptions, FormatResult, FormatState, IndentStyle, PrintError, SourceCodeSlice, VecBuffer,
};
use ruff_formatter::{Formatted, Printed, SourceCode};
use ruff_python_ast::node::{AnyNodeRef, AstNode, NodeKind};
use ruff_python_ast::{Mod, Ranged};
use ruff_python_index::{CommentRanges, CommentRangesBuilder};
use ruff_python_parser::lexer::{lex, lex_starts_at, LexicalError};
use ruff_python_parser::{parse_tokens, Mode, ParseError, Tok};
use ruff_python_trivia::textwrap::dedent;
use ruff_python_trivia::{indentation_at_offset, is_python_whitespace, Cursor};
use ruff_source_file::{Locator, UniversalNewlineIterator};
use ruff_text_size::{TextLen, TextRange, TextSize};
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
        write!(f, [source_position(node.start())])?;
        self.fmt_fields(node, f)?;
        write!(f, [source_position(node.end())])
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
    /// A node can have dangling comments if all its children are tokens or if all node children are optional.
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

    let context = PyFormatContext::new(options, locator.contents(), comments);
    let mut state = FormatState::new(context);
    let mut buffer = VecBuffer::new(&mut state);

    write!(buffer, [root.format()])?;

    let mut document = Document::from(buffer.into_vec());

    // TODO only call if document contains any suppression comment
    handle_suppression_comments(&mut document, state.context().source_code());
    document.propagate_expand();

    Ok(Formatted::new(document, state.into_context()))
}

fn handle_suppression_comments(mut document: &mut Document, source: SourceCode) {
    let mut indent_level = 0u32;
    let mut elements = document.iter_mut().peekable();
    let mut nesting = 0u32;

    while let Some(element) = elements.next() {
        match element {
            FormatElement::Tag(tag::Tag::StartLabelled(label_id))
                if *label_id == LabelId::of(PyLabel::FmtOff) =>
            {
                let mut suppression_start = None;
                let mut parenthesized = nesting > 0;
                let suppression_indent = indent_level;
                let mut suppression_nesting = nesting;
                let start_label = element;

                let end_label = loop {
                    match elements.next() {
                        // TODO bother with nested labels?
                        Some(element @ FormatElement::Tag(tag::Tag::EndLabelled)) => {
                            break element;
                        }
                        Some(FormatElement::SourcePosition(position)) => {
                            suppression_start = Some(*position);
                        }
                        Some(FormatElement::SourceCodeSlice { slice, .. }) => {
                            suppression_start = Some(slice.end());
                        }
                        Some(_) => continue,
                        None => {
                            // Uh, missing end label, let's better return
                            return;
                        }
                    }
                };

                let suppression_start = suppression_start
                    .expect("Expected a source position for the start of the suppression.");

                // Remove the start label because we want to use the end marker for the verbatim text.
                *start_label = FormatElement::Tombstone;

                let mut suppression_end = suppression_start;

                while let Some(element) = elements.next() {
                    match element {
                        FormatElement::Tag(tag::Tag::StartLabelled(label))
                            if *label == LabelId::of(PyLabel::FmtOn)
                                && indent_level == suppression_indent =>
                        {
                            match elements.next() {
                                Some(FormatElement::SourcePosition(start)) => {
                                    suppression_end = *start;
                                }
                                Some(FormatElement::SourceCodeSlice { slice, .. }) => {
                                    suppression_end = slice.start();
                                }
                                _ => {}
                            }

                            break;
                        }
                        FormatElement::SourcePosition(position) => {
                            suppression_end = *position;
                            *element = FormatElement::Tombstone;
                        }
                        FormatElement::SourceCodeSlice { slice, .. } => {
                            suppression_end = slice.end();
                            *element = FormatElement::Tombstone;
                        }

                        FormatElement::Tag(
                            tag::Tag::StartIndent | tag::Tag::StartIndentIfGroupBreaks(_),
                        ) => {
                            indent_level += 1;
                        }

                        FormatElement::Tag(
                            tag::Tag::EndIndent | tag::Tag::EndIndentIfGroupBreaks,
                        ) => {
                            indent_level = indent_level.saturating_sub(1);

                            if indent_level < suppression_indent {
                                break;
                            }
                        }

                        FormatElement::StaticText {
                            text: "(" | "[" | "{",
                        } => {
                            nesting = nesting.saturating_add(1);
                            suppression_nesting = suppression_nesting.saturating_add(1);
                            *element = FormatElement::Tombstone;
                        }

                        FormatElement::StaticText {
                            text: ")" | "]" | "}",
                        } => {
                            nesting = nesting.saturating_sub(1);

                            suppression_nesting = suppression_nesting.saturating_sub(1);
                            *element = FormatElement::Tombstone;

                            if suppression_nesting == 0 && parenthesized {
                                break;
                            }
                        }

                        // Tags only influence how something is printed, but contain no content.
                        // Leave tags intact to avoid unintentionally breaking a document because of unbalanced tags.
                        FormatElement::Tag(_) => continue,

                        element => {
                            // Nuke all content
                            *element = FormatElement::Tombstone
                        }
                    }
                }

                let verbatim_slice =
                    source.slice(TextRange::new(suppression_start, suppression_end));

                *end_label = match normalize_newlines(verbatim_slice.text(source), ['\r']) {
                    Cow::Borrowed(_) => FormatElement::SourceCodeSlice {
                        contains_newlines: true,
                        slice: verbatim_slice,
                    },
                    Cow::Owned(owned) => FormatElement::DynamicText {
                        text: Box::from(owned),
                    },
                };
            }

            FormatElement::Tag(tag::Tag::StartLabelled(label_id))
                if *label_id == LabelId::of(PyLabel::FmtOn) =>
            {
                // Warn, On without a corresponding start
            }

            FormatElement::StaticText {
                text: "(" | "[" | "{",
            } => {
                nesting = nesting.saturating_add(1);
            }

            FormatElement::StaticText {
                text: ")" | "]" | "}",
            } => {
                nesting = nesting.saturating_sub(1);
            }

            FormatElement::Tag(tag::Tag::StartIndent | tag::Tag::StartIndentIfGroupBreaks(_)) => {
                indent_level += 1;
            }

            FormatElement::Tag(tag::Tag::EndIndent | tag::Tag::EndIndentIfGroupBreaks) => {
                indent_level = indent_level.saturating_sub(1);
            }

            // TODO handle Interned and best fitting
            _ => {}
        }
    }

    // TODO lex the context to verify that we haven't messed up the content.
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
                write!(f, [source_text_slice(self.0, ContainsNewlines::Detect)])?;
            }
            Cow::Owned(cleaned) => {
                write!(
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

#[derive(Copy, Clone, Debug)]
pub enum PyLabel {
    FmtOff,
    FmtOn,
}

impl LabelDefinition for PyLabel {
    fn value(&self) -> u64 {
        *self as u64
    }

    fn name(&self) -> &'static str {
        match self {
            PyLabel::FmtOff => "fmt:off",
            PyLabel::FmtOn => "fmt:on",
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::{format_module, format_node, PyFormatOptions};
    use anyhow::Result;
    use insta::assert_snapshot;
    use ruff_python_index::CommentRangesBuilder;
    use ruff_python_parser::lexer::lex;
    use ruff_python_parser::{parse_tokens, Mode};

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
_type_comment_re = re.compile(
        r"""
        (?<!ignore)     # note: this will force the non-greedy + in <type> to match
        """,
        # fmt: off
        re.MULTILINE|re.VERBOSE
        # fmt: on
    )
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
        use ruff_formatter::FormatContext;
        dbg!(formatted
            .document()
            .display(formatted.context().source_code()));

        dbg!(formatted
            .context()
            .comments()
            .debug(formatted.context().source_code()));

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
