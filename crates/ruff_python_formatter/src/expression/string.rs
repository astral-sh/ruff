use crate::builders::optional_parentheses;
use crate::comments::{leading_comments, trailing_comments};
use crate::expression::parentheses::Parentheses;
use crate::prelude::*;
use crate::QuoteStyle;
use bitflags::bitflags;
use ruff_formatter::{format_args, write, FormatError};
use ruff_python_ast::str::is_implicit_concatenation;
use ruff_text_size::{TextLen, TextRange, TextSize};
use rustpython_parser::ast::{ExprConstant, Ranged};
use rustpython_parser::lexer::{lex_starts_at, LexicalError, LexicalErrorType};
use rustpython_parser::{Mode, Tok};
use std::borrow::Cow;

#[derive(Copy, Clone, Debug)]
pub enum StringLayout {
    Default(Option<Parentheses>),

    /// Enforces that implicit continuation strings are printed on a single line even if they exceed
    /// the configured line width.
    Flat,
}

impl Default for StringLayout {
    fn default() -> Self {
        Self::Default(None)
    }
}

pub(super) struct FormatString<'a> {
    constant: &'a ExprConstant,
    layout: StringLayout,
}

impl<'a> FormatString<'a> {
    pub(super) fn new(constant: &'a ExprConstant, layout: StringLayout) -> Self {
        debug_assert!(constant.value.is_str());
        Self { constant, layout }
    }
}

impl<'a> Format<PyFormatContext<'_>> for FormatString<'a> {
    fn fmt(&self, f: &mut Formatter<PyFormatContext<'_>>) -> FormatResult<()> {
        let string_range = self.constant.range();
        let string_content = f.context().locator().slice(string_range);

        if is_implicit_concatenation(string_content) {
            let format_continuation = FormatStringContinuation::new(self.constant, self.layout);

            if let StringLayout::Default(Some(Parentheses::Custom)) = self.layout {
                optional_parentheses(&format_continuation).fmt(f)
            } else {
                format_continuation.fmt(f)
            }
        } else {
            FormatStringPart::new(string_range).fmt(f)
        }
    }
}

struct FormatStringContinuation<'a> {
    constant: &'a ExprConstant,
    layout: StringLayout,
}

impl<'a> FormatStringContinuation<'a> {
    fn new(constant: &'a ExprConstant, layout: StringLayout) -> Self {
        debug_assert!(constant.value.is_str());
        Self { constant, layout }
    }
}

impl Format<PyFormatContext<'_>> for FormatStringContinuation<'_> {
    fn fmt(&self, f: &mut Formatter<PyFormatContext<'_>>) -> FormatResult<()> {
        let comments = f.context().comments().clone();
        let locator = f.context().locator();
        let mut dangling_comments = comments.dangling_comments(self.constant);

        let string_range = self.constant.range();
        let string_content = locator.slice(string_range);

        // The AST parses implicit concatenation as a single string.
        // Call into the lexer to extract the individual chunks and format each string on its own.
        // This code does not yet implement the automatic joining of strings that fit on the same line
        // because this is a black preview style.
        let lexer = lex_starts_at(string_content, Mode::Expression, string_range.start());

        let separator = format_with(|f| match self.layout {
            StringLayout::Default(_) => soft_line_break_or_space().fmt(f),
            StringLayout::Flat => space().fmt(f),
        });

        let mut joiner = f.join_with(separator);

        for token in lexer {
            let (token, token_range) = match token {
                Ok(spanned) => spanned,
                Err(LexicalError {
                    error: LexicalErrorType::IndentationError,
                    ..
                }) => {
                    // This can happen if the string continuation appears anywhere inside of a parenthesized expression
                    // because the lexer doesn't know about the parentheses. For example, the following snipped triggers an Indentation error
                    // ```python
                    // {
                    //     "key": (
                    //         [],
                    //         'a'
                    //             'b'
                    //          'c'
                    //     )
                    // }
                    // ```
                    // Ignoring the error here is *safe* because we know that the program once parsed to a valid AST.
                    continue;
                }
                Err(_) => {
                    return Err(FormatError::SyntaxError);
                }
            };

            match token {
                Tok::String { .. } => {
                    // ```python
                    // (
                    //      "a"
                    //      # leading
                    //      "the comment above"
                    // )
                    // ```
                    let leading_comments_end = dangling_comments
                        .partition_point(|comment| comment.slice().start() <= token_range.start());

                    let (leading_part_comments, rest) =
                        dangling_comments.split_at(leading_comments_end);

                    // ```python
                    // (
                    //      "a" # trailing comment
                    //      "the comment above"
                    // )
                    // ```
                    let trailing_comments_end = rest.partition_point(|comment| {
                        comment.line_position().is_end_of_line()
                            && !locator.contains_line_break(TextRange::new(
                                token_range.end(),
                                comment.slice().start(),
                            ))
                    });

                    let (trailing_part_comments, rest) = rest.split_at(trailing_comments_end);

                    joiner.entry(&format_args![
                        line_suffix_boundary(),
                        leading_comments(leading_part_comments),
                        FormatStringPart::new(token_range),
                        trailing_comments(trailing_part_comments)
                    ]);

                    dangling_comments = rest;
                }
                Tok::Comment(_)
                | Tok::NonLogicalNewline
                | Tok::Newline
                | Tok::Indent
                | Tok::Dedent => continue,
                token => unreachable!("Unexpected token {token:?}"),
            }
        }

        debug_assert!(dangling_comments.is_empty());

        joiner.finish()
    }
}

struct FormatStringPart {
    part_range: TextRange,
}

impl FormatStringPart {
    const fn new(range: TextRange) -> Self {
        Self { part_range: range }
    }
}

impl Format<PyFormatContext<'_>> for FormatStringPart {
    fn fmt(&self, f: &mut Formatter<PyFormatContext<'_>>) -> FormatResult<()> {
        let string_content = f.context().locator().slice(self.part_range);

        let prefix = StringPrefix::parse(string_content);
        let after_prefix = &string_content[usize::from(prefix.text_len())..];

        let quotes = StringQuotes::parse(after_prefix).ok_or(FormatError::SyntaxError)?;
        let relative_raw_content_range = TextRange::new(
            prefix.text_len() + quotes.text_len(),
            string_content.text_len() - quotes.text_len(),
        );
        let raw_content_range = relative_raw_content_range + self.part_range.start();

        let raw_content = &string_content[relative_raw_content_range];
        let preferred_quotes = preferred_quotes(raw_content, quotes, f.options().quote_style());

        write!(f, [prefix, preferred_quotes])?;

        let (normalized, contains_newlines) = normalize_string(raw_content, preferred_quotes);

        match normalized {
            Cow::Borrowed(_) => {
                source_text_slice(raw_content_range, contains_newlines).fmt(f)?;
            }
            Cow::Owned(normalized) => {
                dynamic_text(&normalized, Some(raw_content_range.start())).fmt(f)?;
            }
        }

        preferred_quotes.fmt(f)
    }
}

bitflags! {
    #[derive(Copy, Clone, Debug)]
    struct StringPrefix: u8 {
        const UNICODE   = 0b0000_0001;
        /// `r"test"`
        const RAW       = 0b0000_0010;
        /// `R"test"
        const RAW_UPPER = 0b0000_0100;
        const BYTE      = 0b0000_1000;
        const F_STRING  = 0b0001_0000;
    }
}

impl StringPrefix {
    fn parse(input: &str) -> StringPrefix {
        let chars = input.chars();
        let mut prefix = StringPrefix::empty();

        for c in chars {
            let flag = match c {
                'u' | 'U' => StringPrefix::UNICODE,
                'f' | 'F' => StringPrefix::F_STRING,
                'b' | 'B' => StringPrefix::BYTE,
                'r' => StringPrefix::RAW,
                'R' => StringPrefix::RAW_UPPER,
                '\'' | '"' => break,
                c => {
                    unreachable!(
                        "Unexpected character '{c}' terminating the prefix of a string literal"
                    );
                }
            };

            prefix |= flag;
        }

        prefix
    }

    const fn text_len(self) -> TextSize {
        TextSize::new(self.bits().count_ones())
    }
}

impl Format<PyFormatContext<'_>> for StringPrefix {
    fn fmt(&self, f: &mut Formatter<PyFormatContext<'_>>) -> FormatResult<()> {
        // Retain the casing for the raw prefix:
        // https://black.readthedocs.io/en/stable/the_black_code_style/current_style.html#r-strings-and-r-strings
        if self.contains(StringPrefix::RAW) {
            text("r").fmt(f)?;
        } else if self.contains(StringPrefix::RAW_UPPER) {
            text("R").fmt(f)?;
        }

        if self.contains(StringPrefix::BYTE) {
            text("b").fmt(f)?;
        }

        if self.contains(StringPrefix::F_STRING) {
            text("f").fmt(f)?;
        }

        // Remove the unicode prefix `u` if any because it is meaningless in Python 3+.

        Ok(())
    }
}

/// Detects the preferred quotes for `input`.
/// * single quoted strings: The preferred quote style is the one that requires less escape sequences.
/// * triple quoted strings: Use double quotes except the string contains a sequence of `"""`.
fn preferred_quotes(
    input: &str,
    quotes: StringQuotes,
    configured_style: QuoteStyle,
) -> StringQuotes {
    let preferred_style = if quotes.triple {
        // True if the string contains a triple quote sequence of the configured quote style.
        let mut uses_triple_quotes = false;
        let mut chars = input.chars().peekable();

        while let Some(c) = chars.next() {
            let configured_quote_char = configured_style.as_char();
            match c {
                '\\' => {
                    if matches!(chars.peek(), Some('"' | '\\')) {
                        chars.next();
                    }
                }
                // `"` or `'`
                c if c == configured_quote_char => {
                    match chars.peek().copied() {
                        Some(c) if c == configured_quote_char => {
                            // `""` or `''`
                            chars.next();

                            if chars.peek().copied() == Some(configured_quote_char) {
                                // `"""` or `'''`
                                chars.next();
                                uses_triple_quotes = true;
                            }
                        }
                        Some(_) => {
                            // A single quote char, this is ok
                        }
                        None => {
                            // Trailing quote at the end of the comment
                            uses_triple_quotes = true;
                        }
                    }
                }
                _ => continue,
            }
        }

        if uses_triple_quotes {
            // String contains a triple quote sequence of the configured quote style.
            // Keep the existing quote style.
            quotes.style
        } else {
            configured_style
        }
    } else {
        let mut single_quotes = 0u32;
        let mut double_quotes = 0u32;

        for c in input.chars() {
            match c {
                '\'' => {
                    single_quotes += 1;
                }

                '"' => {
                    double_quotes += 1;
                }

                _ => continue,
            }
        }

        match configured_style {
            QuoteStyle::Single => {
                if single_quotes > double_quotes {
                    QuoteStyle::Double
                } else {
                    QuoteStyle::Single
                }
            }
            QuoteStyle::Double => {
                if double_quotes > single_quotes {
                    QuoteStyle::Single
                } else {
                    QuoteStyle::Double
                }
            }
        }
    };

    StringQuotes {
        triple: quotes.triple,
        style: preferred_style,
    }
}

#[derive(Copy, Clone, Debug)]
struct StringQuotes {
    triple: bool,
    style: QuoteStyle,
}

impl StringQuotes {
    fn parse(input: &str) -> Option<StringQuotes> {
        let mut chars = input.chars();

        let quote_char = chars.next()?;
        let style = QuoteStyle::try_from(quote_char).ok()?;

        let triple = chars.next() == Some(quote_char) && chars.next() == Some(quote_char);

        Some(Self { triple, style })
    }

    const fn text_len(self) -> TextSize {
        if self.triple {
            TextSize::new(3)
        } else {
            TextSize::new(1)
        }
    }
}

impl Format<PyFormatContext<'_>> for StringQuotes {
    fn fmt(&self, f: &mut Formatter<PyFormatContext<'_>>) -> FormatResult<()> {
        let quotes = match (self.style, self.triple) {
            (QuoteStyle::Single, false) => "'",
            (QuoteStyle::Single, true) => "'''",
            (QuoteStyle::Double, false) => "\"",
            (QuoteStyle::Double, true) => "\"\"\"",
        };

        text(quotes).fmt(f)
    }
}

/// Adds the necessary quote escapes and removes unnecessary escape sequences when quoting `input`
/// with the provided `style`.
///
/// Returns the normalized string and whether it contains new lines.
fn normalize_string(input: &str, quotes: StringQuotes) -> (Cow<str>, ContainsNewlines) {
    // The normalized string if `input` is not yet normalized.
    // `output` must remain empty if `input` is already normalized.
    let mut output = String::new();
    // Tracks the last index of `input` that has been written to `output`.
    // If `last_index` is `0` at the end, then the input is already normalized and can be returned as is.
    let mut last_index = 0;

    let mut newlines = ContainsNewlines::No;

    let style = quotes.style;
    let preferred_quote = style.as_char();
    let opposite_quote = style.invert().as_char();

    let mut chars = input.char_indices();

    while let Some((index, c)) = chars.next() {
        if c == '\r' {
            output.push_str(&input[last_index..index]);

            // Skip over the '\r' character, keep the `\n`
            if input.as_bytes().get(index + 1).copied() == Some(b'\n') {
                chars.next();
            }
            // Replace the `\r` with a `\n`
            else {
                output.push('\n');
            }

            last_index = index + '\r'.len_utf8();
            newlines = ContainsNewlines::Yes;
        } else if c == '\n' {
            newlines = ContainsNewlines::Yes;
        } else if !quotes.triple {
            if c == '\\' {
                if let Some(next) = input.as_bytes().get(index + 1).copied().map(char::from) {
                    #[allow(clippy::if_same_then_else)]
                    if next == opposite_quote {
                        // Remove the escape by ending before the backslash and starting again with the quote
                        chars.next();
                        output.push_str(&input[last_index..index]);
                        last_index = index + '\\'.len_utf8();
                    } else if next == preferred_quote {
                        // Quote is already escaped, skip over it.
                        chars.next();
                    } else if next == '\\' {
                        // Skip over escaped backslashes
                        chars.next();
                    }
                }
            } else if c == preferred_quote {
                // Escape the quote
                output.push_str(&input[last_index..index]);
                output.push('\\');
                output.push(c);
                last_index = index + preferred_quote.len_utf8();
            }
        }
    }

    let normalized = if last_index == 0 {
        Cow::Borrowed(input)
    } else {
        output.push_str(&input[last_index..]);
        Cow::Owned(output)
    };

    (normalized, newlines)
}
