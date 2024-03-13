use bitflags::bitflags;

pub(crate) use any::AnyString;
pub(crate) use normalize::{normalize_string, NormalizedString, StringNormalizer};
use ruff_formatter::format_args;
use ruff_python_ast::str::Quote;
use ruff_source_file::Locator;
use ruff_text_size::{TextLen, TextRange, TextSize};

use crate::comments::{leading_comments, trailing_comments};
use crate::expression::parentheses::in_parentheses_only_soft_line_break_or_space;
use crate::prelude::*;
use crate::QuoteStyle;

mod any;
pub(crate) mod docstring;
mod normalize;

#[derive(Copy, Clone, Debug, Default)]
pub(crate) enum Quoting {
    #[default]
    CanChange,
    Preserve,
}

/// Formats any implicitly concatenated string. This could be any valid combination
/// of string, bytes or f-string literals.
pub(crate) struct FormatStringContinuation<'a> {
    string: &'a AnyString<'a>,
}

impl<'a> FormatStringContinuation<'a> {
    pub(crate) fn new(string: &'a AnyString<'a>) -> Self {
        Self { string }
    }
}

impl Format<PyFormatContext<'_>> for FormatStringContinuation<'_> {
    fn fmt(&self, f: &mut PyFormatter) -> FormatResult<()> {
        let comments = f.context().comments().clone();
        let quoting = self.string.quoting(&f.context().locator());

        let mut joiner = f.join_with(in_parentheses_only_soft_line_break_or_space());

        for part in self.string.parts(quoting) {
            joiner.entry(&format_args![
                line_suffix_boundary(),
                leading_comments(comments.leading(&part)),
                part,
                trailing_comments(comments.trailing(&part))
            ]);
        }

        joiner.finish()
    }
}

#[derive(Debug)]
pub(crate) struct StringPart {
    /// The prefix.
    prefix: StringPrefix,

    /// The actual quotes of the string in the source
    quotes: StringQuotes,

    /// The range of the string's content (full range minus quotes and prefix)
    content_range: TextRange,
}

impl StringPart {
    pub(crate) fn from_source(range: TextRange, locator: &Locator) -> Self {
        let string_content = locator.slice(range);

        let prefix = StringPrefix::parse(string_content);
        let after_prefix = &string_content[usize::from(prefix.text_len())..];

        let quotes =
            StringQuotes::parse(after_prefix).expect("Didn't find string quotes after prefix");
        let relative_raw_content_range = TextRange::new(
            prefix.text_len() + quotes.text_len(),
            string_content.text_len() - quotes.text_len(),
        );
        let raw_content_range = relative_raw_content_range + range.start();

        Self {
            prefix,
            content_range: raw_content_range,
            quotes,
        }
    }

    /// Returns the prefix of the string part.
    pub(crate) const fn prefix(&self) -> StringPrefix {
        self.prefix
    }

    /// Returns the surrounding quotes of the string part.
    pub(crate) const fn quotes(&self) -> StringQuotes {
        self.quotes
    }

    /// Returns the range of the string's content in the source (minus prefix and quotes).
    pub(crate) const fn content_range(&self) -> TextRange {
        self.content_range
    }
}

bitflags! {
    #[derive(Copy, Clone, Debug, PartialEq, Eq)]
    pub(crate) struct StringPrefix: u8 {
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
    pub(crate) fn parse(input: &str) -> StringPrefix {
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

    pub(crate) const fn text_len(self) -> TextSize {
        TextSize::new(self.bits().count_ones())
    }

    pub(super) const fn is_raw_string(self) -> bool {
        self.contains(StringPrefix::RAW) || self.contains(StringPrefix::RAW_UPPER)
    }

    pub(super) const fn is_fstring(self) -> bool {
        self.contains(StringPrefix::F_STRING)
    }

    pub(super) const fn is_byte(self) -> bool {
        self.contains(StringPrefix::BYTE)
    }
}

impl Format<PyFormatContext<'_>> for StringPrefix {
    fn fmt(&self, f: &mut PyFormatter) -> FormatResult<()> {
        // Retain the casing for the raw prefix:
        // https://black.readthedocs.io/en/stable/the_black_code_style/current_style.html#r-strings-and-r-strings
        if self.contains(StringPrefix::RAW) {
            token("r").fmt(f)?;
        } else if self.contains(StringPrefix::RAW_UPPER) {
            token("R").fmt(f)?;
        }

        if self.contains(StringPrefix::BYTE) {
            token("b").fmt(f)?;
        }

        if self.contains(StringPrefix::F_STRING) {
            token("f").fmt(f)?;
        }

        // Remove the unicode prefix `u` if any because it is meaningless in Python 3+.

        Ok(())
    }
}

#[derive(Copy, Clone, Debug)]
pub(crate) struct StringQuotes {
    triple: bool,
    quote_char: Quote,
}

impl StringQuotes {
    pub(crate) fn parse(input: &str) -> Option<StringQuotes> {
        let mut chars = input.chars();

        let quote_char = chars.next()?;
        let quote = Quote::try_from(quote_char).ok()?;

        let triple = chars.next() == Some(quote_char) && chars.next() == Some(quote_char);

        Some(Self {
            triple,
            quote_char: quote,
        })
    }

    pub(crate) const fn is_triple(self) -> bool {
        self.triple
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
    fn fmt(&self, f: &mut PyFormatter) -> FormatResult<()> {
        let quotes = match (self.quote_char, self.triple) {
            (Quote::Single, false) => "'",
            (Quote::Single, true) => "'''",
            (Quote::Double, false) => "\"",
            (Quote::Double, true) => "\"\"\"",
        };

        token(quotes).fmt(f)
    }
}

impl TryFrom<QuoteStyle> for Quote {
    type Error = ();

    fn try_from(style: QuoteStyle) -> Result<Quote, ()> {
        match style {
            QuoteStyle::Single => Ok(Quote::Single),
            QuoteStyle::Double => Ok(Quote::Double),
            QuoteStyle::Preserve => Err(()),
        }
    }
}

impl From<Quote> for QuoteStyle {
    fn from(value: Quote) -> Self {
        match value {
            Quote::Single => QuoteStyle::Single,
            Quote::Double => QuoteStyle::Double,
        }
    }
}
