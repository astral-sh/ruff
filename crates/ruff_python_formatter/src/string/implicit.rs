use itertools::Itertools;
use ruff_formatter::{FormatContext, format_args, write};
use ruff_python_ast::str::{Quote, TripleQuotes};
use ruff_python_ast::str_prefix::{
    AnyStringPrefix, ByteStringPrefix, FStringPrefix, StringLiteralPrefix, TStringPrefix,
};
use ruff_python_ast::{
    AnyStringFlags, FString, InterpolatedStringElement, StringFlags, StringLike, StringLikePart,
    TString,
};
use ruff_source_file::LineRanges;
use ruff_text_size::{Ranged, TextRange};
use std::borrow::Cow;

use crate::comments::{leading_comments, trailing_comments};
use crate::context::WithInterpolatedStringState;
use crate::expression::parentheses::in_parentheses_only_soft_line_break_or_space;
use crate::other::interpolated_string::{InterpolatedStringContext, InterpolatedStringLayout};
use crate::other::interpolated_string_element::FormatInterpolatedElement;
use crate::prelude::*;
use crate::string::docstring::needs_chaperone_space;
use crate::string::normalize::{
    QuoteMetadata, is_fstring_with_quoted_debug_expression,
    is_fstring_with_triple_quoted_literal_expression_containing_quotes,
    is_interpolated_string_with_quoted_format_spec_and_debug,
};
use crate::string::{StringLikeExtensions, StringNormalizer, StringQuotes, normalize_string};

/// Formats any implicitly concatenated string. This could be any valid combination
/// of string, bytes, f-string, or t-string literals.
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
        let flat = FormatImplicitConcatenatedStringFlat::new(self.string, f.context());
        let expanded = FormatImplicitConcatenatedStringExpanded::new(
            self.string,
            if flat.is_some() {
                ImplicitConcatenatedLayout::MaybeFlat
            } else {
                ImplicitConcatenatedLayout::Multipart
            },
        );

        // If the string can be joined, try joining the implicit concatenated string into a single string
        // if it fits on the line. Otherwise, parenthesize the string parts and format each part on its
        // own line.
        if let Some(flat) = flat {
            write!(
                f,
                [if_group_fits_on_line(&flat), if_group_breaks(&expanded)]
            )
        } else {
            expanded.fmt(f)
        }
    }
}

/// Formats an implicit concatenated string where parts are separated by a space or line break.
pub(crate) struct FormatImplicitConcatenatedStringExpanded<'a> {
    string: StringLike<'a>,
    layout: ImplicitConcatenatedLayout,
}

impl<'a> FormatImplicitConcatenatedStringExpanded<'a> {
    pub(crate) fn new(string: StringLike<'a>, layout: ImplicitConcatenatedLayout) -> Self {
        assert!(string.is_implicit_concatenated());

        Self { string, layout }
    }
}

impl Format<PyFormatContext<'_>> for FormatImplicitConcatenatedStringExpanded<'_> {
    fn fmt(&self, f: &mut Formatter<PyFormatContext<'_>>) -> FormatResult<()> {
        let comments = f.context().comments().clone();

        // Keep implicit concatenated strings expanded unless they're already written on a single line.
        if matches!(self.layout, ImplicitConcatenatedLayout::Multipart)
            && self.string.parts().tuple_windows().any(|(a, b)| {
                f.context()
                    .source()
                    .contains_line_break(TextRange::new(a.end(), b.start()))
            })
        {
            expand_parent().fmt(f)?;
        }

        let mut joiner = f.join_with(in_parentheses_only_soft_line_break_or_space());

        for part in self.string.parts() {
            let format_part = format_with(|f: &mut PyFormatter| match part {
                StringLikePart::String(part) => part.format().fmt(f),
                StringLikePart::Bytes(bytes_literal) => bytes_literal.format().fmt(f),
                StringLikePart::FString(part) => part.format().fmt(f),
                StringLikePart::TString(part) => part.format().fmt(f),
            });

            let part_comments = comments.leading_dangling_trailing(part);
            joiner.entry(&format_args![
                leading_comments(part_comments.leading),
                format_part,
                trailing_comments(part_comments.trailing)
            ]);
        }

        joiner.finish()
    }
}

#[derive(Copy, Clone, Debug)]
pub(crate) enum ImplicitConcatenatedLayout {
    /// The string might get joined into a single string if it fits on a single line.
    MaybeFlat,
    /// The string will remain a multipart string.
    Multipart,
}

/// Formats an implicit concatenated string where parts are joined into a single string if possible.
pub(crate) struct FormatImplicitConcatenatedStringFlat<'a> {
    string: StringLike<'a>,
    flags: AnyStringFlags,
    docstring: bool,
}

impl<'a> FormatImplicitConcatenatedStringFlat<'a> {
    /// Creates a new formatter. Returns `None` if the string can't be merged into a single string.
    pub(crate) fn new(string: StringLike<'a>, context: &PyFormatContext) -> Option<Self> {
        fn merge_flags(string: StringLike, context: &PyFormatContext) -> Option<AnyStringFlags> {
            // Multiline strings can never fit on a single line.
            if string.is_multiline(context) {
                return None;
            }

            let first_part = string.parts().next()?;

            // The string is either a regular string, f-string, t-string, or bytes string.
            let normalizer = StringNormalizer::from_context(context);

            // Some if a part requires preserving its quotes.
            let mut preserve_quotes_requirement: Option<Quote> = None;

            // Early exit if it's known that this string can't be joined
            for part in string.parts() {
                // Similar to Black, don't collapse triple quoted and raw strings.
                // We could technically join strings that are raw-strings and use the same quotes but lets not do this for now.
                // Joining triple quoted strings is more complicated because an
                // implicit concatenated string could become a docstring (if it's the first string in a block).
                // That means the joined string formatting would have to call into
                // the docstring formatting or otherwise guarantee that the output
                // won't change on a second run.
                if part.flags().is_triple_quoted() || part.flags().is_raw_string() {
                    return None;
                }

                // For now, preserve comments documenting a specific part over possibly
                // collapsing onto a single line. Collapsing could result in pragma comments
                // now covering more code.
                if context.comments().leading_trailing(&part).next().is_some() {
                    return None;
                }

                match part {
                    StringLikePart::FString(fstring) => {
                        if matches!(string, StringLike::TString(_)) {
                            // Don't concatenate t-strings and f-strings
                            return None;
                        }
                        if context.options().target_version().supports_pep_701() {
                            if is_interpolated_string_with_quoted_format_spec_and_debug(
                                &fstring.elements,
                                fstring.flags.into(),
                                context,
                            ) {
                                if preserve_quotes_requirement
                                    .is_some_and(|quote| quote != part.flags().quote_style())
                                {
                                    return None;
                                }
                                preserve_quotes_requirement = Some(part.flags().quote_style());
                            }
                        }
                        // Avoid invalid syntax for pre Python 312:
                        // * When joining parts that have debug expressions with quotes: `f"{10 + len('bar')=}" f'{10 + len("bar")=}'
                        // * When joining parts that contain triple quoted strings with quotes: `f"{'''test ' '''}" f'{"""other " """}'`
                        else if is_fstring_with_quoted_debug_expression(fstring, context)
                            || is_fstring_with_triple_quoted_literal_expression_containing_quotes(
                                fstring, context,
                            )
                        {
                            if preserve_quotes_requirement
                                .is_some_and(|quote| quote != part.flags().quote_style())
                            {
                                return None;
                            }
                            preserve_quotes_requirement = Some(part.flags().quote_style());
                        }
                    }
                    StringLikePart::TString(tstring) => {
                        if is_interpolated_string_with_quoted_format_spec_and_debug(
                            &tstring.elements,
                            tstring.flags.into(),
                            context,
                        ) {
                            if preserve_quotes_requirement
                                .is_some_and(|quote| quote != part.flags().quote_style())
                            {
                                return None;
                            }
                            preserve_quotes_requirement = Some(part.flags().quote_style());
                        }
                    }
                    StringLikePart::Bytes(_) | StringLikePart::String(_) => {}
                }
            }

            // The string is either a regular string, f-string, or bytes string.
            let mut merged_quotes: Option<QuoteMetadata> = None;

            // Only preserve the string type but disregard the `u` and `r` prefixes.
            // * It's not necessary to preserve the `r` prefix because Ruff doesn't support joining raw strings (we shouldn't get here).
            // * It's not necessary to preserve the `u` prefix because Ruff discards the `u` prefix (it's meaningless in Python 3+)
            let prefix = match string {
                StringLike::String(_) => AnyStringPrefix::Regular(StringLiteralPrefix::Empty),
                StringLike::Bytes(_) => AnyStringPrefix::Bytes(ByteStringPrefix::Regular),
                StringLike::FString(_) => AnyStringPrefix::Format(FStringPrefix::Regular),
                StringLike::TString(_) => AnyStringPrefix::Template(TStringPrefix::Regular),
            };

            let quote = if let Some(quote) = preserve_quotes_requirement {
                quote
            } else {
                // Only determining the preferred quote for the first string is sufficient
                // because we don't support joining triple quoted strings with non triple quoted strings.
                if let Ok(preferred_quote) =
                    Quote::try_from(normalizer.preferred_quote_style(first_part))
                {
                    for part in string.parts() {
                        let part_quote_metadata =
                            QuoteMetadata::from_part(part, context, preferred_quote);

                        if let Some(merged) = merged_quotes.as_mut() {
                            *merged = part_quote_metadata.merge(merged)?;
                        } else {
                            merged_quotes = Some(part_quote_metadata);
                        }
                    }

                    merged_quotes?.choose(preferred_quote)
                } else {
                    // Use the quotes of the first part if the quotes should be preserved.
                    first_part.flags().quote_style()
                }
            };

            // CPython resolves the escape sequences of each part before concatenating them, so an
            // octal escape at the end of one part can't absorb the digits that the next part starts
            // with: `"\1" "2"` is `"\x012"`, whereas the joined `"\12"` is `"\n"`. Keep such
            // strings split.
            if merge_changes_value(string, context) {
                return None;
            }

            Some(AnyStringFlags::new(prefix, quote, TripleQuotes::No))
        }

        if !string.is_implicit_concatenated() {
            return None;
        }

        Some(Self {
            flags: merge_flags(string, context)?,
            string,
            docstring: false,
        })
    }

    pub(crate) fn set_docstring(&mut self, is_docstring: bool) {
        self.docstring = is_docstring;
    }

    pub(crate) fn string(&self) -> StringLike<'a> {
        self.string
    }
}

impl Format<PyFormatContext<'_>> for FormatImplicitConcatenatedStringFlat<'_> {
    fn fmt(&self, f: &mut Formatter<PyFormatContext<'_>>) -> FormatResult<()> {
        // Merges all string parts into a single string.
        let quotes = StringQuotes::from(self.flags);

        write!(f, [self.flags.prefix(), quotes])?;

        let mut parts = self.string.parts().peekable();

        // Trim implicit concatenated strings in docstring positions.
        // Skip over any trailing parts that are all whitespace.
        // Leading parts are handled as part of the formatting loop below.
        if self.docstring {
            for part in self.string.parts().rev() {
                assert!(part.is_string_literal());

                if f.context().source()[part.content_range()].trim().is_empty() {
                    // Don't format the part.
                    parts.next_back();
                } else {
                    break;
                }
            }
        }

        let mut first_non_empty = self.docstring;

        while let Some(part) = parts.next() {
            match part {
                StringLikePart::String(_) | StringLikePart::Bytes(_) => {
                    FormatLiteralContent {
                        range: part.content_range(),
                        flags: self.flags,
                        is_interpolated_string: false,
                        trim_start: first_non_empty && self.docstring,
                        trim_end: self.docstring && parts.peek().is_none(),
                    }
                    .fmt(f)?;

                    if first_non_empty {
                        first_non_empty = f.context().source()[part.content_range()]
                            .trim_start()
                            .is_empty();
                    }
                }

                StringLikePart::FString(FString { elements, .. })
                | StringLikePart::TString(TString { elements, .. }) => {
                    let context = InterpolatedStringContext::new(
                        self.flags,
                        InterpolatedStringLayout::from_interpolated_string_elements(
                            elements,
                            f.context().source(),
                        ),
                    );
                    let state = f
                        .context()
                        .interpolated_string_state()
                        .enter_string(context);
                    let f = &mut WithInterpolatedStringState::new(state, &mut *f);
                    for element in elements {
                        match element {
                            InterpolatedStringElement::Literal(literal) => {
                                FormatLiteralContent {
                                    range: literal.range(),
                                    flags: self.flags,
                                    is_interpolated_string: true,
                                    trim_end: false,
                                    trim_start: false,
                                }
                                .fmt(f)?;
                            }
                            // Formatting the expression here and in the expanded version is safe **only**
                            // because we assert that the f/t-string never contains any comments.
                            InterpolatedStringElement::Interpolation(expression) => {
                                FormatInterpolatedElement::new(expression, context).fmt(f)?;
                            }
                        }
                    }
                }
            }
        }

        quotes.fmt(f)
    }
}

struct FormatLiteralContent {
    range: TextRange,
    flags: AnyStringFlags,
    is_interpolated_string: bool,
    trim_start: bool,
    trim_end: bool,
}

impl Format<PyFormatContext<'_>> for FormatLiteralContent {
    fn fmt(&self, f: &mut PyFormatter) -> FormatResult<()> {
        let content = &f.context().source()[self.range];
        let mut normalized = normalize_string(
            content,
            0,
            self.flags,
            self.flags.is_interpolated_string() && !self.is_interpolated_string,
        );

        // Trim the start and end of the string if it's the first or last part of a docstring.
        // This is rare, so don't bother with optimizing to use `Cow`.
        if self.trim_start {
            let trimmed = normalized.trim_start();
            if trimmed.len() < normalized.len() {
                normalized = trimmed.to_string().into();
            }
        }

        if self.trim_end {
            let trimmed = normalized.trim_end();
            if trimmed.len() < normalized.len() {
                normalized = trimmed.to_string().into();
            }
        }

        if !normalized.is_empty() {
            match &normalized {
                Cow::Borrowed(_) => source_text_slice(self.range).fmt(f)?,
                Cow::Owned(normalized) => text(normalized).fmt(f)?,
            }

            if self.trim_end && needs_chaperone_space(self.flags, &normalized) {
                space().fmt(f)?;
            }
        }
        Ok(())
    }
}

/// Returns `true` if merging the parts of `string` into a single literal would change its value
/// because an escape sequence at the end of one part absorbs a digit from the next part.
fn merge_changes_value(string: StringLike<'_>, context: &PyFormatContext) -> bool {
    let source = context.source();
    let mut ends_with = None;

    for part in string.parts() {
        let Some((starts_with, new_ends_with)) = text_edges(part, source) else {
            // Parts that contribute no text don't create a boundary of their own.
            continue;
        };

        if ends_with.is_some_and(|ends_with| escape_absorbs(ends_with, starts_with)) {
            return true;
        }

        ends_with = Some(new_ends_with);
    }

    false
}

/// The character the text a part contributes starts with and the text it ends with, or `None` if
/// the part contributes no text at all. Replacement fields contribute braces.
fn text_edges<'a>(part: StringLikePart<'_>, source: &'a str) -> Option<(char, &'a str)> {
    match part {
        StringLikePart::String(_) | StringLikePart::Bytes(_) => {
            let content = &source[part.content_range()];
            Some((content.chars().next()?, content))
        }
        StringLikePart::FString(FString { elements, .. })
        | StringLikePart::TString(TString { elements, .. }) => {
            let text = |element: &InterpolatedStringElement| match element {
                InterpolatedStringElement::Literal(literal) => &source[literal.range()],
                InterpolatedStringElement::Interpolation(_) => "{}",
            };

            let starts_with = elements.iter().map(text).find(|text| !text.is_empty())?;
            let ends_with = elements
                .iter()
                .rev()
                .map(text)
                .find(|text| !text.is_empty())?;

            Some((starts_with.chars().next()?, ends_with))
        }
    }
}

/// Returns `true` if `ends_with` ends with an octal escape that `starts_with` extends, changing
/// the literal's value once the parts are joined.
///
/// Octal escapes are the only ones that can span the boundary: `\ooo` takes one to three digits,
/// so a part can end with an escape that is still hungry for digits. The other escapes can't:
/// `\xhh`, `\uhhhh`, `\Uhhhhhhhh` and `\N{...}` demand their full syntax, which means a part
/// ending with them parses as a complete escape on its own, and a plain `\c` escape is a single
/// character that no following digit can join.
fn escape_absorbs(ends_with: &str, starts_with: char) -> bool {
    if !matches!(starts_with, '0'..='7') {
        return false;
    }

    let bytes = ends_with.as_bytes();

    // The digits the escape at the end of the part has consumed so far.
    let mut digits = bytes.len();
    while digits > 0 && matches!(bytes[digits - 1], b'0'..=b'7') {
        digits -= 1;
    }

    matches!(bytes.len() - digits, 1 | 2) && trailing_backslashes(&ends_with[..digits]) % 2 == 1
}

/// The number of backslashes `text` ends with. A backslash only opens an escape sequence if the
/// number of backslashes preceding it is even; an odd number means it escapes another backslash.
fn trailing_backslashes(text: &str) -> usize {
    let bytes = text.as_bytes();

    let mut backslashes = 0;
    while backslashes < bytes.len() && bytes[bytes.len() - 1 - backslashes] == b'\\' {
        backslashes += 1;
    }

    backslashes
}
