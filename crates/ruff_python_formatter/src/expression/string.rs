use crate::prelude::*;
use crate::{not_yet_implemented_custom_text, QuoteStyle};
use bitflags::bitflags;
use ruff_formatter::{write, FormatError};
use ruff_python_ast::str::is_implicit_concatenation;
use ruff_text_size::{TextLen, TextRange, TextSize};
use rustpython_parser::ast::{ExprConstant, Ranged};
use std::borrow::Cow;

pub(super) struct FormatString {
    string_range: TextRange,
}

impl FormatString {
    pub(super) fn new(constant: &ExprConstant) -> Self {
        debug_assert!(constant.value.is_str());
        Self {
            string_range: constant.range(),
        }
    }
}

impl Format<PyFormatContext<'_>> for FormatString {
    fn fmt(&self, f: &mut Formatter<PyFormatContext<'_>>) -> FormatResult<()> {
        let string_content = f.context().locator().slice(self.string_range);

        if is_implicit_concatenation(string_content) {
            not_yet_implemented_custom_text(r#""NOT_YET_IMPLEMENTED" "IMPLICIT_CONCATENATION""#)
                .fmt(f)
        } else {
            FormatStringPart::new(self.string_range).fmt(f)
        }
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
        let (preferred_quote, contains_newlines) = preferred_quotes(raw_content);

        let preferred_quotes = StringQuotes {
            style: preferred_quote,
            triple: quotes.triple,
        };

        write!(f, [prefix, preferred_quotes])?;

        let normalized = normalize_quotes(raw_content, preferred_quote);

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

/// Detects the preferred quotes for `input`. The preferred quote style is the one that
/// requires less escape sequences.
fn preferred_quotes(input: &str) -> (QuoteStyle, ContainsNewlines) {
    let mut single_quotes = 0u32;
    let mut double_quotes = 0u32;
    let mut contains_newlines = ContainsNewlines::No;

    for c in input.chars() {
        match c {
            '\'' => {
                single_quotes += 1;
            }

            '"' => {
                double_quotes += 1;
            }

            '\n' | '\r' => {
                contains_newlines = ContainsNewlines::Yes;
            }

            _ => continue,
        }
    }

    let quote_style = if double_quotes > single_quotes {
        QuoteStyle::Single
    } else {
        QuoteStyle::Double
    };

    (quote_style, contains_newlines)
}

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

    const fn text_len(&self) -> TextSize {
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
fn normalize_quotes(input: &str, style: QuoteStyle) -> Cow<str> {
    // The normalized string if `input` is not yet normalized.
    // `output` must remain empty if `input` is already normalized.
    let mut output = String::new();
    // Tracks the last index of `input` that has been written to `output`.
    // If `last_index` is `0` at the end, then the input is already normalized and can be returned as is.
    let mut last_index = 0;

    let preferred_quote = style.as_char();
    let opposite_quote = style.opposite().as_char();

    let mut chars = input.char_indices();

    while let Some((index, c)) = chars.next() {
        if c == '\\' {
            if let Some((_, next)) = chars.next() {
                if next == opposite_quote {
                    // Remove the escape by ending before the backslash and starting again with the quote
                    output.push_str(&input[last_index..index]);
                    last_index = index + '\\'.len_utf8();
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

    if last_index == 0 {
        Cow::Borrowed(input)
    } else {
        output.push_str(&input[last_index..]);
        Cow::Owned(output)
    }
}
