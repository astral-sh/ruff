use memchr::memchr2;

pub(crate) use normalize::{normalize_string, NormalizedString, StringNormalizer};
use ruff_formatter::format_args;
use ruff_python_ast::str::Quote;
use ruff_python_ast::{
    self as ast,
    str_prefix::{AnyStringPrefix, StringLiteralPrefix},
    AnyStringFlags, StringFlags, StringLike, StringLikePart,
};
use ruff_source_file::Locator;
use ruff_text_size::Ranged;

use crate::comments::{leading_comments, trailing_comments};
use crate::expression::expr_f_string::f_string_quoting;
use crate::expression::parentheses::in_parentheses_only_soft_line_break_or_space;
use crate::other::f_string::FormatFString;
use crate::other::string_literal::StringLiteralKind;
use crate::prelude::*;
use crate::QuoteStyle;

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
    string: StringLike<'a>,
}

impl<'a> FormatImplicitConcatenatedString<'a> {
    pub(crate) fn new(string: impl Into<StringLike<'a>>) -> Self {
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

        for part in self.string.parts() {
            let part_comments = comments.leading_dangling_trailing(&part);

            let format_part = format_with(|f: &mut PyFormatter| match part {
                StringLikePart::String(part) => {
                    let kind = if self.string.is_fstring() {
                        #[allow(deprecated)]
                        StringLiteralKind::InImplicitlyConcatenatedFString(quoting)
                    } else {
                        StringLiteralKind::String
                    };

                    part.format().with_options(kind).fmt(f)
                }
                StringLikePart::Bytes(bytes_literal) => bytes_literal.format().fmt(f),
                StringLikePart::FString(part) => FormatFString::new(part, quoting).fmt(f),
            });

            joiner.entry(&format_args![
                line_suffix_boundary(),
                leading_comments(part_comments.leading),
                format_part,
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

// Extension trait that adds formatter specific helper methods to `StringLike`.
pub(crate) trait StringLikeExtensions {
    fn quoting(&self, locator: &Locator<'_>) -> Quoting;

    fn is_multiline(&self, source: &str) -> bool;
}

impl StringLikeExtensions for ast::StringLike<'_> {
    fn quoting(&self, locator: &Locator<'_>) -> Quoting {
        match self {
            Self::String(_) | Self::Bytes(_) => Quoting::CanChange,
            Self::FString(f_string) => f_string_quoting(f_string, locator),
        }
    }

    fn is_multiline(&self, source: &str) -> bool {
        match self {
            Self::String(_) | Self::Bytes(_) => {
                self.parts()
                    .next()
                    .is_some_and(|part| part.flags().is_triple_quoted())
                    && memchr2(b'\n', b'\r', source[self.range()].as_bytes()).is_some()
            }
            Self::FString(fstring) => {
                memchr2(b'\n', b'\r', source[fstring.range].as_bytes()).is_some()
            }
        }
    }
}
