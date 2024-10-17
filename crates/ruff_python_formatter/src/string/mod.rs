use std::borrow::Cow;

use memchr::memchr2;

pub(crate) use normalize::{normalize_string, NormalizedString, StringNormalizer};
use ruff_formatter::{format_args, write};
use ruff_python_ast::str::Quote;
use ruff_python_ast::str_prefix::{ByteStringPrefix, FStringPrefix};
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
use crate::preview::is_join_implicit_concatenated_string_enabled;
use crate::string::normalize::QuoteMetadata;
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

    fn merged_flags(&self, context: &PyFormatContext) -> Option<AnyStringFlags> {
        if !is_join_implicit_concatenated_string_enabled(context) {
            return None;
        }

        // Early exit if it's known that this string can't be joined
        if !self.string.is_implicit_and_can_join(context) {
            return None;
        }

        // Don't merge multiline strings because that's pointless, a multiline string can
        // never fit on a single line.
        // TODO: The `is_multiline` implementation for f-string is an over-approximation and can
        //   return `true` even if the f-string then gets formatted to a single line.
        //   That's why we disregard the early exit here (it's just an optimisation).
        if !self.string.is_fstring() && self.string.is_multiline(context.source()) {
            return None;
        }

        // The string is either a regular string, f-string, or bytes string.
        let normalizer = StringNormalizer::from_context(context);

        // TODO: Do we need to respect the quoting from an enclosing f-string?
        let mut merged_quotes: Option<QuoteMetadata> = None;

        // Only preserve the string type but disregard the `u` and `r` prefixes.
        // * It's not necessary to preserve the `r` prefix because Ruff doesn't support joining raw strings (we shouldn't get here).
        // * It's not necessary to preserve the `u` prefix because Ruff discards the `u` prefix (it's meaningless in Python 3+)
        let prefix = match self.string {
            StringLike::String(_) => AnyStringPrefix::Regular(StringLiteralPrefix::Empty),
            StringLike::Bytes(_) => AnyStringPrefix::Bytes(ByteStringPrefix::Regular),
            StringLike::FString(_) => AnyStringPrefix::Format(FStringPrefix::Regular),
        };

        let first_part = self.string.parts().next()?;

        // Only determining the preferred quote for the first string is sufficient
        // because we don't support joining triple quoted strings with non triple quoted strings.
        let Ok(preferred_quote) = Quote::try_from(normalizer.preferred_quote_style(first_part))
        else {
            // TODO: Handle preserve
            return None;
        };

        for part in self.string.parts() {
            let part_quote_metadata = QuoteMetadata::from_part(part, preferred_quote, context);

            if let Some(merged) = merged_quotes.as_mut() {
                *merged = part_quote_metadata.merge(merged)?;
            } else {
                merged_quotes = Some(part_quote_metadata);
            }
        }

        Some(AnyStringFlags::new(
            prefix,
            merged_quotes?.choose(preferred_quote),
            false,
        ))
    }
}

impl Format<PyFormatContext<'_>> for FormatImplicitConcatenatedString<'_> {
    fn fmt(&self, f: &mut PyFormatter) -> FormatResult<()> {
        let comments = f.context().comments().clone();
        let quoting = self.string.quoting(&f.context().locator());

        // Formats the string where each part is preserved as in the source.
        // The parts are joined by soft line breaks.
        let format_expanded = format_with(|f| {
            let mut joiner = f.join_with(in_parentheses_only_soft_line_break_or_space());
            for part in self.string.parts() {
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

                let part_comments = comments.leading_dangling_trailing(&part);
                joiner.entry(&format_args![
                    line_suffix_boundary(),
                    leading_comments(part_comments.leading),
                    format_part,
                    trailing_comments(part_comments.trailing)
                ]);
            }

            joiner.finish()
        });

        // If the string can be joined, try joining the implicit concatenated string into a single string
        // if it fits on the line. Otherwise, parenthesize the string parts and format each part on its
        // own line.
        if let Some(flags) = self.merged_flags(f.context()) {
            // Merges all string parts into a single string.
            let format_flat = format_with(|f| {
                let quotes = StringQuotes::from(flags);

                write!(f, [flags.prefix(), quotes])?;

                // TODO: FStrings when the f-string preview style is enabled???

                for part in self.string.parts() {
                    let content = f.context().locator().slice(part.content_range());
                    let normalized = normalize_string(
                        content,
                        0,
                        flags,
                        flags.is_f_string() && !part.flags().is_f_string(),
                        true,
                        false,
                    );
                    match normalized {
                        Cow::Borrowed(_) => source_text_slice(part.content_range()).fmt(f)?,
                        Cow::Owned(normalized) => text(&normalized).fmt(f)?,
                    }
                }

                quotes.fmt(f)
            });

            write!(
                f,
                [
                    // TODO: strings in expression statements aren't joined correctly because they aren't wrap in a group :(
                    if_group_fits_on_line(&format_flat),
                    if_group_breaks(&format_expanded)
                ]
            )
        } else {
            format_expanded.fmt(f)
        }
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

    /// Tests if this is an implicitly concatenated string that can't be joined.
    ///
    /// * Returns `false` if this is not an implicitly concatenated string or the parts can't be joined
    ///   because any part is a raw string, triple quoted string, or has leading or trailing comments.
    /// * Returns `true` if it is an implicit concatenated string and Ruff tries to join it if it fits on the line.
    fn is_implicit_and_can_join(&self, context: &PyFormatContext) -> bool;
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
            Self::String(_) | Self::Bytes(_) => self.parts().any(|part| {
                part.flags().is_triple_quoted()
                    && memchr2(b'\n', b'\r', source[self.range()].as_bytes()).is_some()
            }),
            Self::FString(fstring) => {
                memchr2(b'\n', b'\r', source[fstring.range].as_bytes()).is_some()
            }
        }
    }

    fn is_implicit_and_can_join(&self, context: &PyFormatContext) -> bool {
        if !self.is_implicit_concatenated() {
            return false;
        }

        for part in self.parts() {
            // Similar to Black, don't collapse triple quoted and raw strings.
            // We could technically join strings that are raw-strings and use the same quotes but lets not do this for now.
            // Joining triple quoted strings is more complicated because an
            // implicit concatenated string could become a docstring (if it's the first string in a block).
            // That means the joined string formatting would have to call into
            // the docstring formatting or otherwise guarantee that the output
            // won't change on a second run.
            if part.flags().is_triple_quoted() || part.flags().is_raw_string() {
                return false;
            }

            // For now, preserve comments documenting a specific part over possibly
            // collapsing onto a single line. Collapsing could result in pragma comments
            // now covering more code.
            if context.comments().leading_trailing(&part).next().is_some() {
                return false;
            }
        }

        true
    }
}
