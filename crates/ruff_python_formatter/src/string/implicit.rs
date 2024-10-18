use std::borrow::Cow;

use ruff_python_ast::str::Quote;
use ruff_python_ast::str_prefix::{
    AnyStringPrefix, ByteStringPrefix, FStringPrefix, StringLiteralPrefix,
};
use ruff_python_ast::{AnyStringFlags, StringFlags, StringLike, StringLikePart};

use crate::comments::{leading_comments, trailing_comments};
use crate::expression::parentheses::in_parentheses_only_soft_line_break_or_space;
use crate::other::f_string::FormatFString;
use crate::other::string_literal::StringLiteralKind;
use crate::prelude::*;
use crate::preview::is_join_implicit_concatenated_string_enabled;
use crate::string::normalize::QuoteMetadata;
use crate::string::{normalize_string, StringLikeExtensions, StringNormalizer, StringQuotes};

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
        let expanded = FormatImplicitConcatenatedStringExpanded::new(self.string);

        // If the string can be joined, try joining the implicit concatenated string into a single string
        // if it fits on the line. Otherwise, parenthesize the string parts and format each part on its
        // own line.
        if let Some(flat) = FormatImplicitConcatenatedStringFlat::new(self.string, f.context()) {
            ruff_formatter::write!(
                f,
                [
                    // TODO: strings in expression statements aren't joined correctly because they aren't wrap in a group :(
                    if_group_fits_on_line(&flat),
                    if_group_breaks(&expanded)
                ]
            )
        } else {
            expanded.fmt(f)
        }
    }
}

pub(crate) struct FormatImplicitConcatenatedStringExpanded<'a> {
    string: StringLike<'a>,
}

impl<'a> FormatImplicitConcatenatedStringExpanded<'a> {
    pub(crate) fn new(string: StringLike<'a>) -> Self {
        assert!(string.is_implicit_concatenated());

        Self { string }
    }
}

impl Format<PyFormatContext<'_>> for FormatImplicitConcatenatedStringExpanded<'_> {
    fn fmt(&self, f: &mut Formatter<PyFormatContext<'_>>) -> FormatResult<()> {
        let comments = f.context().comments().clone();
        let quoting = self.string.quoting(&f.context().locator());

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
            joiner.entry(&ruff_formatter::format_args![
                line_suffix_boundary(),
                leading_comments(part_comments.leading),
                format_part,
                trailing_comments(part_comments.trailing)
            ]);
        }

        joiner.finish()
    }
}

pub(crate) struct FormatImplicitConcatenatedStringFlat<'a> {
    string: StringLike<'a>,
    flags: AnyStringFlags,
}

impl<'a> FormatImplicitConcatenatedStringFlat<'a> {
    pub(crate) fn new(string: StringLike<'a>, context: &PyFormatContext) -> Option<Self> {
        fn merge_flags(string: StringLike, context: &PyFormatContext) -> Option<AnyStringFlags> {
            if !is_join_implicit_concatenated_string_enabled(context) {
                return None;
            }

            // Early exit if it's known that this string can't be joined
            if !string.is_implicit_and_can_join(context) {
                return None;
            }

            // Don't merge multiline strings because that's pointless, a multiline string can
            // never fit on a single line.
            // TODO: The `is_multiline` implementation for f-string is an over-approximation and can
            //   return `true` even if the f-string then gets formatted to a single line.
            //   That's why we disregard the early exit here (it's just an optimisation).
            if !string.is_fstring() && string.is_multiline(context.source()) {
                return None;
            }

            // The string is either a regular string, f-string, or bytes string.
            let normalizer = StringNormalizer::from_context(context);

            // TODO: Do we need to respect the quoting from an enclosing f-string?
            let mut merged_quotes: Option<QuoteMetadata> = None;

            // Only preserve the string type but disregard the `u` and `r` prefixes.
            // * It's not necessary to preserve the `r` prefix because Ruff doesn't support joining raw strings (we shouldn't get here).
            // * It's not necessary to preserve the `u` prefix because Ruff discards the `u` prefix (it's meaningless in Python 3+)
            let prefix = match string {
                StringLike::String(_) => AnyStringPrefix::Regular(StringLiteralPrefix::Empty),
                StringLike::Bytes(_) => AnyStringPrefix::Bytes(ByteStringPrefix::Regular),
                StringLike::FString(_) => AnyStringPrefix::Format(FStringPrefix::Regular),
            };

            let first_part = string.parts().next()?;

            // Only determining the preferred quote for the first string is sufficient
            // because we don't support joining triple quoted strings with non triple quoted strings.
            let Ok(preferred_quote) = Quote::try_from(normalizer.preferred_quote_style(first_part))
            else {
                // TODO: Handle preserve
                return None;
            };

            for part in string.parts() {
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

        if !string.is_implicit_concatenated() {
            return None;
        }

        Some(Self {
            flags: merge_flags(string, context)?,
            string,
        })
    }
}

impl Format<PyFormatContext<'_>> for FormatImplicitConcatenatedStringFlat<'_> {
    fn fmt(&self, f: &mut Formatter<PyFormatContext<'_>>) -> FormatResult<()> {
        // Merges all string parts into a single string.
        let quotes = StringQuotes::from(self.flags);

        ruff_formatter::write!(f, [self.flags.prefix(), quotes])?;

        // TODO: FStrings when the f-string preview style is enabled???

        for part in self.string.parts() {
            let content = f.context().locator().slice(part.content_range());
            let normalized = normalize_string(
                content,
                0,
                self.flags,
                self.flags.is_f_string() && !part.flags().is_f_string(),
                true,
                false,
            );
            match normalized {
                Cow::Borrowed(_) => source_text_slice(part.content_range()).fmt(f)?,
                Cow::Owned(normalized) => text(&normalized).fmt(f)?,
            }
        }

        quotes.fmt(f)
    }
}
