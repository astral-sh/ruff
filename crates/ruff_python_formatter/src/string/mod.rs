use bitflags::bitflags;

pub(crate) use any::AnyString;
pub(crate) use normalize::{NormalizedString, StringNormalizer};
use ruff_formatter::{format_args, write};
use ruff_source_file::Locator;
use ruff_text_size::{Ranged, TextLen, TextRange, TextSize};

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

        let parts = self.string.parts(quoting);

        // Don't try the flat layout if it is know that the implicit string remains on multiple lines either because one
        // part is a multline or a part has a leading or trailing comment.
        let should_try_flat = !parts.clone().any(|part| {
            let part_comments = comments.leading_dangling_trailing(&part);

            part.is_multiline(f.context().source())
                || part_comments.has_leading()
                || part_comments.has_trailing()
        });

        let format_flat = format_with(|f: &mut PyFormatter| {
            let mut merged_prefix = StringPrefix::empty();
            let mut all_raw = true;
            let quotes = parts.clone().next().map_or(
                StringQuotes {
                    triple: false,
                    quote_char: QuoteChar::Double,
                },
                |part| StringPart::from_source(part.range(), &f.context().locator()).quotes,
            );

            for part in parts.clone() {
                let string_part = StringPart::from_source(part.range(), &f.context().locator());

                let prefix = string_part.prefix;
                merged_prefix = prefix.union(merged_prefix);
                all_raw &= prefix.is_raw_string();

                // quotes are more complicated. We need to collect the statistics about the used quotes for each string
                // - number of single quotes
                // - number of double quotes
                // - number of triple quotes
                // And they need to be normalized as a second step
                // Also requires tracking how many times a simple string uses an escaped triple quoted sequence to avoid
                // stability issues.
            }

            // Prefer lower case raw string flags over uppercase if both are present.
            if merged_prefix.contains(StringPrefix::RAW)
                && merged_prefix.contains(StringPrefix::RAW_UPPER)
            {
                merged_prefix.remove(StringPrefix::RAW_UPPER);
            }

            // Remove the raw prefix if there's a mixture of raw and non-raw string. The formatting code coming later normalizes raw strings to regular
            // strings if the flag isn't present.
            if !all_raw {
                merged_prefix.remove(StringPrefix::RAW);
            }

            // We need to find the common prefix and quotes for all parts and use that one.
            // no prefix: easy
            // bitflags! {
            //     #[derive(Copy, Clone, Debug, PartialEq, Eq)]
            //     pub(crate) struct StringPrefix: u8 {
            //         const UNICODE   = 0b0000_0001;
            //         /// `r"test"`
            //         const RAW       = 0b0000_0010;
            //         /// `R"test"
            //         const RAW_UPPER = 0b0000_0100;
            //         const BYTE      = 0b0000_1000;
            //         const F_STRING  = 0b0001_0000;
            //     }
            // }
            //
            // Prefix precedence:
            // - Unicode -> Always remove
            // - Raw upper -> Remove except when all parts are raw upper
            // - Raw -> Remove except when all parts are raw or raw upper.
            // - F-String -> Preserve
            // - Bytes -> Preserve
            // Quotes:
            // - Single quotes: Identify the number of single and double quotes in the string and use the one with the least count.
            // - single and triple: Use triple quotes
            // - triples: Use `choose_quote` for every part and use the one with the highest count

            write!(f, [merged_prefix, quotes])?;
            for part in parts.clone() {
                let string_part = StringPart::from_source(part.range(), &f.context().locator());

                write!(f, [source_text_slice(string_part.content_range)])?;
            }

            quotes.fmt(f)
        });

        let format_expanded = format_with(|f| {
            let mut joiner = f.join_with(in_parentheses_only_soft_line_break_or_space());

            for part in parts.clone() {
                joiner.entry(&format_args![
                    line_suffix_boundary(),
                    leading_comments(comments.leading(&part)),
                    part,
                    trailing_comments(comments.trailing(&part))
                ]);
            }

            joiner.finish()
        });

        // TODO: where's the group coming from?

        if should_try_flat {
            group(&format_args![
                if_group_fits_on_line(&format_flat),
                if_group_breaks(&format_expanded)
            ])
            .fmt(f)
        } else {
            format_expanded.fmt(f)
        }
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
    quote_char: QuoteChar,
}

impl StringQuotes {
    pub(crate) fn parse(input: &str) -> Option<StringQuotes> {
        let mut chars = input.chars();

        let quote_char = chars.next()?;
        let quote = QuoteChar::try_from(quote_char).ok()?;

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
            (QuoteChar::Single, false) => "'",
            (QuoteChar::Single, true) => "'''",
            (QuoteChar::Double, false) => "\"",
            (QuoteChar::Double, true) => "\"\"\"",
        };

        token(quotes).fmt(f)
    }
}

/// The quotation character used to quote a string, byte, or fstring literal.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum QuoteChar {
    /// A single quote: `'`
    Single,

    /// A double quote: '"'
    Double,
}

impl QuoteChar {
    pub const fn as_char(self) -> char {
        match self {
            QuoteChar::Single => '\'',
            QuoteChar::Double => '"',
        }
    }

    #[must_use]
    pub const fn invert(self) -> QuoteChar {
        match self {
            QuoteChar::Single => QuoteChar::Double,
            QuoteChar::Double => QuoteChar::Single,
        }
    }

    #[must_use]
    pub const fn from_style(style: QuoteStyle) -> Option<QuoteChar> {
        match style {
            QuoteStyle::Single => Some(QuoteChar::Single),
            QuoteStyle::Double => Some(QuoteChar::Double),
            QuoteStyle::Preserve => None,
        }
    }
}

impl From<QuoteChar> for QuoteStyle {
    fn from(value: QuoteChar) -> Self {
        match value {
            QuoteChar::Single => QuoteStyle::Single,
            QuoteChar::Double => QuoteStyle::Double,
        }
    }
}

impl TryFrom<char> for QuoteChar {
    type Error = ();

    fn try_from(value: char) -> Result<Self, Self::Error> {
        match value {
            '\'' => Ok(QuoteChar::Single),
            '"' => Ok(QuoteChar::Double),
            _ => Err(()),
        }
    }
}
