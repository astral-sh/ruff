pub(crate) use any::AnyString;
pub(crate) use normalize::{normalize_string, NormalizedString, StringNormalizer};
use ruff_formatter::format_args;
use ruff_python_ast::str::Quote;
use ruff_python_ast::{
    self as ast,
    str_prefix::{AnyStringPrefix, StringLiteralPrefix},
    AnyStringFlags, StringFlags,
};
use ruff_text_size::{Ranged, TextRange};

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
pub(crate) struct FormatImplicitConcatenatedString<'a> {
    string: AnyString<'a>,
}

impl<'a> FormatImplicitConcatenatedString<'a> {
    pub(crate) fn new(string: impl Into<AnyString<'a>>) -> Self {
        Self {
            string: string.into(),
        }
    }
}

impl Format<PyFormatContext<'_>> for FormatImplicitConcatenatedString<'_> {
    fn fmt(&self, f: &mut PyFormatter) -> FormatResult<()> {
        let comments = f.context().comments().clone();
        let quoting = self.string.quoting(&f.context().locator());

        let mut joiner = f.join_with(in_parentheses_only_soft_line_break_or_space());

        for part in self.string.parts(quoting) {
            let part_comments = comments.leading_dangling_trailing(&part);
            joiner.entry(&format_args![
                line_suffix_boundary(),
                leading_comments(part_comments.leading),
                part,
                trailing_comments(part_comments.trailing)
            ]);
        }

        joiner.finish()
    }
}

impl Format<PyFormatContext<'_>> for AnyStringPrefix {
    fn fmt(&self, f: &mut PyFormatter) -> FormatResult<()> {
        // Remove the unicode prefix `u` if any because it is meaningless in Python 3+.
        if !matches!(
            self,
            AnyStringPrefix::Regular(StringLiteralPrefix::Empty | StringLiteralPrefix::Unicode)
        ) {
            token(self.as_str()).fmt(f)?;
        }
        Ok(())
    }
}

#[derive(Copy, Clone, Debug)]
pub(crate) struct StringQuotes {
    triple: bool,
    quote_char: Quote,
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

impl From<AnyStringFlags> for StringQuotes {
    fn from(value: AnyStringFlags) -> Self {
        Self {
            triple: value.is_triple_quoted(),
            quote_char: value.quote_style(),
        }
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

#[derive(Debug, Clone, Copy)]
pub(crate) struct StringPart {
    flags: AnyStringFlags,
    range: TextRange,
}

impl Ranged for StringPart {
    fn range(&self) -> TextRange {
        self.range
    }
}

impl StringPart {
    /// Use the `kind()` method to retrieve information about the
    fn flags(self) -> AnyStringFlags {
        self.flags
    }

    /// Returns the range of the string's content in the source (minus prefix and quotes).
    fn content_range(self) -> TextRange {
        let kind = self.flags();
        TextRange::new(
            self.start() + kind.opener_len(),
            self.end() - kind.closer_len(),
        )
    }
}

impl From<&ast::StringLiteral> for StringPart {
    fn from(value: &ast::StringLiteral) -> Self {
        Self {
            range: value.range,
            flags: value.flags.into(),
        }
    }
}

impl From<&ast::BytesLiteral> for StringPart {
    fn from(value: &ast::BytesLiteral) -> Self {
        Self {
            range: value.range,
            flags: value.flags.into(),
        }
    }
}

impl From<&ast::FString> for StringPart {
    fn from(value: &ast::FString) -> Self {
        Self {
            range: value.range,
            flags: value.flags.into(),
        }
    }
}
