use std::borrow::Cow;
use std::cmp::Ordering;
use std::iter::FusedIterator;

use ruff_formatter::FormatContext;
use ruff_python_ast::visitor::source_order::SourceOrderVisitor;
use ruff_python_ast::{
    str::{Quote, TripleQuotes},
    AnyStringFlags, BytesLiteral, FString, FStringElement, FStringElements, FStringFlags,
    StringFlags, StringLikePart, StringLiteral,
};
use ruff_text_size::{Ranged, TextRange, TextSlice};

use crate::context::FStringState;
use crate::prelude::*;
use crate::string::StringQuotes;
use crate::QuoteStyle;

pub(crate) struct StringNormalizer<'a, 'src> {
    preferred_quote_style: Option<QuoteStyle>,
    context: &'a PyFormatContext<'src>,
}

impl<'a, 'src> StringNormalizer<'a, 'src> {
    pub(crate) fn from_context(context: &'a PyFormatContext<'src>) -> Self {
        Self {
            preferred_quote_style: None,
            context,
        }
    }

    pub(crate) fn with_preferred_quote_style(mut self, quote_style: QuoteStyle) -> Self {
        self.preferred_quote_style = Some(quote_style);
        self
    }

    /// Determines the preferred quote style for `string`.
    /// The formatter should use the preferred quote style unless
    /// it can't because the string contains the preferred quotes OR
    /// it leads to more escaping.
    ///
    /// Note: If you add more cases here where we return `QuoteStyle::Preserve`,
    /// make sure to also add them to [`FormatImplicitConcatenatedStringFlat::new`].
    pub(super) fn preferred_quote_style(&self, string: StringLikePart) -> QuoteStyle {
        let preferred_quote_style = self
            .preferred_quote_style
            .unwrap_or(self.context.options().quote_style());
        let supports_pep_701 = self.context.options().target_version().supports_pep_701();

        // For f-strings prefer alternating the quotes unless The outer string is triple quoted and the inner isn't.
        if let FStringState::InsideExpressionElement(parent_context) = self.context.f_string_state()
        {
            let parent_flags = parent_context.f_string().flags();

            if !parent_flags.is_triple_quoted() || string.flags().is_triple_quoted() {
                // This logic is even necessary when using preserve and the target python version doesn't support PEP701 because
                // we might end up joining two f-strings that have different quote styles, in which case we need to alternate the quotes
                // for inner strings to avoid a syntax error: `string = "this is my string with " f'"{params.get("mine")}"'`
                if !preferred_quote_style.is_preserve() || !supports_pep_701 {
                    return QuoteStyle::from(parent_flags.quote_style().opposite());
                }
            }
        }

        // Leave the quotes unchanged for all other strings.
        if preferred_quote_style.is_preserve() {
            return QuoteStyle::Preserve;
        }

        // There are cases where it is necessary to preserve the quotes to prevent an invalid f-string.
        if let StringLikePart::FString(fstring) = string {
            // There are two cases where it's necessary to preserve the quotes if the
            // target version is pre 3.12 and the part is an f-string.
            if !supports_pep_701 {
                // An f-string expression contains a debug text with a quote character
                // because the formatter will emit the debug expression **exactly** the
                // same as in the source text.
                if is_fstring_with_quoted_debug_expression(fstring, self.context) {
                    return QuoteStyle::Preserve;
                }

                // An f-string expression that contains a triple quoted string literal
                // expression that contains a quote.
                if is_fstring_with_triple_quoted_literal_expression_containing_quotes(
                    fstring,
                    self.context,
                ) {
                    return QuoteStyle::Preserve;
                }
            }

            // An f-string expression element contains a debug text and the corresponding
            // format specifier has a literal element with a quote character.
            if is_fstring_with_quoted_format_spec_and_debug(fstring, self.context) {
                return QuoteStyle::Preserve;
            }
        }

        // Per PEP 8, always prefer double quotes for triple-quoted strings.
        if string.flags().is_triple_quoted() {
            // ... unless we're formatting a code snippet inside a docstring,
            // then we specifically want to invert our quote style to avoid
            // writing out invalid Python.
            //
            // It's worth pointing out that we can actually wind up being
            // somewhat out of sync with PEP8 in this case. Consider this
            // example:
            //
            //     def foo():
            //         '''
            //         Something.
            //
            //         >>> """tricksy"""
            //         '''
            //         pass
            //
            // Ideally, this would be reformatted as:
            //
            //     def foo():
            //         """
            //         Something.
            //
            //         >>> '''tricksy'''
            //         """
            //         pass
            //
            // But the logic here results in the original quoting being
            // preserved. This is because the quoting style of the outer
            // docstring is determined, in part, by looking at its contents. In
            // this case, it notices that it contains a `"""` and thus infers
            // that using `'''` would overall read better because it avoids
            // the need to escape the interior `"""`. Except... in this case,
            // the `"""` is actually part of a code snippet that could get
            // reformatted to using a different quoting style itself.
            //
            // Fixing this would, I believe, require some fairly seismic
            // changes to how formatting strings works. Namely, we would need
            // to look for code snippets before normalizing the docstring, and
            // then figure out the quoting style more holistically by looking
            // at the various kinds of quotes used in the code snippets and
            // what reformatting them might look like.
            //
            // Overall this is a bit of a corner case and just inverting the
            // style from what the parent ultimately decided upon works, even
            // if it doesn't have perfect alignment with PEP8.
            if let Some(quote) = self.context.docstring() {
                QuoteStyle::from(quote.opposite())
            } else {
                QuoteStyle::Double
            }
        } else {
            preferred_quote_style
        }
    }

    /// Computes the strings preferred quotes.
    pub(crate) fn choose_quotes(&self, string: StringLikePart) -> QuoteSelection {
        let raw_content = &self.context.source()[string.content_range()];
        let first_quote_or_normalized_char_offset = raw_content
            .bytes()
            .position(|b| matches!(b, b'\\' | b'"' | b'\'' | b'\r'));
        let string_flags = string.flags();
        let preferred_style = self.preferred_quote_style(string);

        let new_kind = match (
            Quote::try_from(preferred_style),
            first_quote_or_normalized_char_offset,
        ) {
            // The string contains no quotes so it's safe to use the preferred quote style
            (Ok(preferred_quote), None) => string_flags.with_quote_style(preferred_quote),

            // The preferred quote style is single or double quotes, and the string contains a quote or
            // another character that may require escaping
            (Ok(preferred_quote), Some(first_quote_or_normalized_char_offset)) => {
                let metadata = if string.is_fstring() {
                    QuoteMetadata::from_part(string, self.context, preferred_quote)
                } else {
                    QuoteMetadata::from_str(
                        &raw_content[first_quote_or_normalized_char_offset..],
                        string.flags(),
                        preferred_quote,
                    )
                };

                let quote = metadata.choose(preferred_quote);

                string_flags.with_quote_style(quote)
            }

            // The preferred quote style is to preserve the quotes, so let's do that.
            (Err(()), _) => string_flags,
        };

        QuoteSelection {
            flags: new_kind,
            first_quote_or_normalized_char_offset,
        }
    }

    /// Computes the strings preferred quotes and normalizes its content.
    pub(crate) fn normalize(&self, string: StringLikePart) -> NormalizedString<'src> {
        let raw_content = &self.context.source()[string.content_range()];
        let quote_selection = self.choose_quotes(string);

        let normalized = if let Some(first_quote_or_escape_offset) =
            quote_selection.first_quote_or_normalized_char_offset
        {
            normalize_string(
                raw_content,
                first_quote_or_escape_offset,
                quote_selection.flags,
                false,
            )
        } else {
            Cow::Borrowed(raw_content)
        };

        NormalizedString {
            flags: quote_selection.flags,
            content_range: string.content_range(),
            text: normalized,
        }
    }
}

#[derive(Debug)]
pub(crate) struct QuoteSelection {
    flags: AnyStringFlags,

    /// Offset to the first quote character or character that needs special handling in [`normalize_string`].
    first_quote_or_normalized_char_offset: Option<usize>,
}

impl QuoteSelection {
    pub(crate) fn flags(&self) -> AnyStringFlags {
        self.flags
    }
}

#[derive(Clone, Debug)]
pub(crate) struct QuoteMetadata {
    kind: QuoteMetadataKind,

    /// The quote style in the source.
    source_style: Quote,
}

/// Tracks information about the used quotes in a string which is used
/// to choose the quotes for a part.
impl QuoteMetadata {
    pub(crate) fn from_part(
        part: StringLikePart,
        context: &PyFormatContext,
        preferred_quote: Quote,
    ) -> Self {
        match part {
            StringLikePart::String(_) | StringLikePart::Bytes(_) => {
                let text = &context.source()[part.content_range()];

                Self::from_str(text, part.flags(), preferred_quote)
            }
            StringLikePart::FString(fstring) => {
                let metadata = QuoteMetadata::from_str("", part.flags(), preferred_quote);

                metadata.merge_fstring_elements(
                    &fstring.elements,
                    fstring.flags,
                    context,
                    preferred_quote,
                )
            }
        }
    }

    pub(crate) fn from_str(text: &str, flags: AnyStringFlags, preferred_quote: Quote) -> Self {
        let kind = if flags.is_raw_string() {
            QuoteMetadataKind::raw(text, preferred_quote, flags.triple_quotes())
        } else if flags.is_triple_quoted() {
            QuoteMetadataKind::triple_quoted(text, preferred_quote)
        } else {
            QuoteMetadataKind::regular(text)
        };

        Self {
            kind,
            source_style: flags.quote_style(),
        }
    }

    pub(super) fn choose(&self, preferred_quote: Quote) -> Quote {
        match self.kind {
            QuoteMetadataKind::Raw { contains_preferred } => {
                if contains_preferred {
                    self.source_style
                } else {
                    preferred_quote
                }
            }
            QuoteMetadataKind::Triple { contains_preferred } => {
                if contains_preferred {
                    self.source_style
                } else {
                    preferred_quote
                }
            }
            QuoteMetadataKind::Regular {
                single_quotes,
                double_quotes,
            } => match single_quotes.cmp(&double_quotes) {
                Ordering::Less => Quote::Single,
                Ordering::Equal => preferred_quote,
                Ordering::Greater => Quote::Double,
            },
        }
    }

    /// Merges the quotes metadata of different literals.
    ///
    /// ## Raw and triple quoted strings
    /// Merging raw and triple quoted strings is only correct if all literals are from the same part.
    /// E.g. it's okay to merge triple and raw strings from a single `FString` part's literals
    /// but it isn't safe to merge raw and triple quoted strings from different parts of an implicit
    /// concatenated string. Where safe means, it may lead to incorrect results.
    pub(super) fn merge(self, other: &QuoteMetadata) -> Option<QuoteMetadata> {
        let kind = match (self.kind, other.kind) {
            (
                QuoteMetadataKind::Regular {
                    single_quotes: self_single,
                    double_quotes: self_double,
                },
                QuoteMetadataKind::Regular {
                    single_quotes: other_single,
                    double_quotes: other_double,
                },
            ) => QuoteMetadataKind::Regular {
                single_quotes: self_single + other_single,
                double_quotes: self_double + other_double,
            },

            // Can't merge quotes from raw strings (even when both strings are raw)
            (
                QuoteMetadataKind::Raw {
                    contains_preferred: self_contains_preferred,
                },
                QuoteMetadataKind::Raw {
                    contains_preferred: other_contains_preferred,
                },
            ) => QuoteMetadataKind::Raw {
                contains_preferred: self_contains_preferred || other_contains_preferred,
            },

            (
                QuoteMetadataKind::Triple {
                    contains_preferred: self_contains_preferred,
                },
                QuoteMetadataKind::Triple {
                    contains_preferred: other_contains_preferred,
                },
            ) => QuoteMetadataKind::Triple {
                contains_preferred: self_contains_preferred || other_contains_preferred,
            },

            (_, _) => return None,
        };

        Some(Self {
            kind,
            source_style: self.source_style,
        })
    }

    /// For f-strings, only consider the quotes inside string-literals but ignore
    /// quotes inside expressions (except inside the format spec). This allows both the outer and the nested literals
    /// to make the optimal local-choice to reduce the total number of quotes necessary.
    /// This doesn't require any pre 312 special handling because an expression
    /// can never contain the outer quote character, not even escaped:
    /// ```python
    /// f"{'escaping a quote like this \" is a syntax error pre 312'}"
    /// ```
    fn merge_fstring_elements(
        self,
        elements: &FStringElements,
        flags: FStringFlags,
        context: &PyFormatContext,
        preferred_quote: Quote,
    ) -> Self {
        let mut merged = self;

        for element in elements {
            match element {
                FStringElement::Literal(literal) => {
                    merged = merged
                        .merge(&QuoteMetadata::from_str(
                            context.source().slice(literal),
                            flags.into(),
                            preferred_quote,
                        ))
                        .expect("Merge to succeed because all parts have the same flags");
                }
                FStringElement::Expression(expression) => {
                    if let Some(spec) = expression.format_spec.as_deref() {
                        if expression.debug_text.is_none() {
                            merged = merged.merge_fstring_elements(
                                &spec.elements,
                                flags,
                                context,
                                preferred_quote,
                            );
                        }
                    }
                }
            }
        }

        merged
    }
}

#[derive(Copy, Clone, Debug)]
enum QuoteMetadataKind {
    /// A raw string.
    ///
    /// For raw strings it's only possible to change the quotes if the preferred quote style
    /// isn't used inside the string.
    Raw { contains_preferred: bool },

    /// Regular (non raw) triple quoted string.
    ///
    /// For triple quoted strings it's only possible to change the quotes if no
    /// triple of the preferred quotes is used inside the string.
    Triple { contains_preferred: bool },

    /// A single quoted string that uses either double or single quotes.
    ///
    /// For regular strings it's desired to pick the quote style that requires the least escaping.
    /// E.g. pick single quotes for `'A "dog"'` because using single quotes would require escaping
    /// the two `"`.
    Regular {
        single_quotes: u32,
        double_quotes: u32,
    },
}

impl QuoteMetadataKind {
    /// For triple quoted strings, the preferred quote style can't be used if the string contains
    /// a tripled of the quote character (e.g., if double quotes are preferred, double quotes will be
    /// used unless the string contains `"""`).
    fn triple_quoted(content: &str, preferred_quote: Quote) -> Self {
        // True if the string contains a triple quote sequence of the configured quote style.
        let mut uses_triple_quotes = false;
        let mut chars = content.chars().peekable();

        while let Some(c) = chars.next() {
            let preferred_quote_char = preferred_quote.as_char();
            match c {
                '\\' => {
                    if matches!(chars.peek(), Some('"' | '\\')) {
                        chars.next();
                    }
                }
                // `"` or `'`
                c if c == preferred_quote_char => {
                    match chars.peek().copied() {
                        Some(c) if c == preferred_quote_char => {
                            // `""` or `''`
                            chars.next();

                            match chars.peek().copied() {
                                Some(c) if c == preferred_quote_char => {
                                    // `"""` or `'''`
                                    chars.next();
                                    uses_triple_quotes = true;
                                    break;
                                }
                                Some(_) => {}
                                None => {
                                    // Handle `''' ""'''`. At this point we have consumed both
                                    // double quotes, so on the next iteration the iterator is empty
                                    // and we'd miss the string ending with a preferred quote
                                    uses_triple_quotes = true;
                                    break;
                                }
                            }
                        }
                        Some(_) => {
                            // A single quote char, this is ok
                        }
                        None => {
                            // Trailing quote at the end of the comment
                            uses_triple_quotes = true;
                            break;
                        }
                    }
                }
                _ => continue,
            }
        }

        Self::Triple {
            contains_preferred: uses_triple_quotes,
        }
    }

    /// For single quoted strings, the preferred quote style is used, unless the alternative quote style
    /// would require fewer escapes.
    fn regular(text: &str) -> Self {
        let mut single_quotes = 0u32;
        let mut double_quotes = 0u32;

        for c in text.chars() {
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

        Self::Regular {
            single_quotes,
            double_quotes,
        }
    }

    /// Computes if a raw string uses the preferred quote. If it does, then it's not possible
    /// to change the quote style because it would require escaping which isn't possible in raw strings.
    fn raw(text: &str, preferred: Quote, triple_quotes: TripleQuotes) -> Self {
        let mut chars = text.chars().peekable();
        let preferred_quote_char = preferred.as_char();

        let contains_unescaped_configured_quotes = loop {
            match chars.next() {
                Some('\\') => {
                    // Ignore escaped characters
                    chars.next();
                }
                // `"` or `'`
                Some(c) if c == preferred_quote_char => {
                    if triple_quotes.is_no() {
                        break true;
                    }

                    match chars.peek() {
                        // We can't turn `r'''\""'''` into `r"""\"""""`, this would confuse the parser
                        // about where the closing triple quotes start
                        None => break true,
                        Some(next) if *next == preferred_quote_char => {
                            // `""` or `''`
                            chars.next();

                            // We can't turn `r'''""'''` into `r""""""""`, nor can we have
                            // `"""` or `'''` respectively inside the string
                            if chars.peek().is_none() || chars.peek() == Some(&preferred_quote_char)
                            {
                                break true;
                            }
                        }
                        _ => {}
                    }
                }
                Some(_) => continue,
                None => break false,
            }
        };

        Self::Raw {
            contains_preferred: contains_unescaped_configured_quotes,
        }
    }
}

#[derive(Debug)]
pub(crate) struct NormalizedString<'a> {
    /// Holds data about the quotes and prefix of the string
    flags: AnyStringFlags,

    /// The range of the string's content in the source (minus prefix and quotes).
    content_range: TextRange,

    /// The normalized text
    text: Cow<'a, str>,
}

impl<'a> NormalizedString<'a> {
    pub(crate) fn text(&self) -> &Cow<'a, str> {
        &self.text
    }

    pub(crate) fn flags(&self) -> AnyStringFlags {
        self.flags
    }
}

impl Ranged for NormalizedString<'_> {
    fn range(&self) -> TextRange {
        self.content_range
    }
}

impl Format<PyFormatContext<'_>> for NormalizedString<'_> {
    fn fmt(&self, f: &mut Formatter<PyFormatContext<'_>>) -> FormatResult<()> {
        let quotes = StringQuotes::from(self.flags);
        ruff_formatter::write!(f, [self.flags.prefix(), quotes])?;
        match &self.text {
            Cow::Borrowed(_) => source_text_slice(self.range()).fmt(f)?,
            Cow::Owned(normalized) => text(normalized).fmt(f)?,
        }

        quotes.fmt(f)
    }
}

pub(crate) fn normalize_string(
    input: &str,
    start_offset: usize,
    new_flags: AnyStringFlags,
    escape_braces: bool,
) -> Cow<str> {
    // The normalized string if `input` is not yet normalized.
    // `output` must remain empty if `input` is already normalized.
    let mut output = String::new();
    // Tracks the last index of `input` that has been written to `output`.
    // If `last_index` is `0` at the end, then the input is already normalized and can be returned as is.
    let mut last_index = 0;

    let quote = new_flags.quote_style();
    let preferred_quote = quote.as_char();
    let opposite_quote = quote.opposite().as_char();

    let mut chars = CharIndicesWithOffset::new(input, start_offset).peekable();

    let is_raw = new_flags.is_raw_string();

    while let Some((index, c)) = chars.next() {
        if matches!(c, '{' | '}') {
            if escape_braces {
                // Escape `{` and `}` when converting a regular string literal to an f-string literal.
                output.push_str(&input[last_index..=index]);
                output.push(c);
                last_index = index + c.len_utf8();
                continue;
            }
        }

        if c == '\r' {
            output.push_str(&input[last_index..index]);

            // Skip over the '\r' character, keep the `\n`
            if chars.peek().copied().is_some_and(|(_, next)| next == '\n') {
                chars.next();
            }
            // Replace the `\r` with a `\n`
            else {
                output.push('\n');
            }

            last_index = index + '\r'.len_utf8();
        } else if !is_raw {
            if c == '\\' {
                if let Some((_, next)) = chars.clone().next() {
                    if next == '\\' {
                        // Skip over escaped backslashes
                        chars.next();
                    } else {
                        // Length of the `\` plus the length of the escape sequence character (`u` | `U` | `x`)
                        let escape_start_len = '\\'.len_utf8() + next.len_utf8();
                        if let Some(normalised) =
                            UnicodeEscape::new(next, !new_flags.is_byte_string()).and_then(
                                |escape| escape.normalize(&input[index + escape_start_len..]),
                            )
                        {
                            let escape_start_offset = index + escape_start_len;
                            if let Cow::Owned(normalised) = &normalised {
                                output.push_str(&input[last_index..escape_start_offset]);
                                output.push_str(normalised);
                                last_index = escape_start_offset + normalised.len();
                            }

                            // Move the `chars` iterator passed the escape sequence.
                            // Simply reassigning `chars` doesn't work because the indices` would
                            // then be off.
                            for _ in 0..next.len_utf8() + normalised.len() {
                                chars.next();
                            }
                        }
                    }

                    if !new_flags.is_triple_quoted() {
                        #[allow(clippy::if_same_then_else)]
                        if next == opposite_quote {
                            // Remove the escape by ending before the backslash and starting again with the quote
                            chars.next();
                            output.push_str(&input[last_index..index]);
                            last_index = index + '\\'.len_utf8();
                        } else if next == preferred_quote {
                            // Quote is already escaped, skip over it.
                            chars.next();
                        }
                    }
                }
            } else if !new_flags.is_triple_quoted() && c == preferred_quote {
                // Escape the quote
                output.push_str(&input[last_index..index]);
                output.push('\\');
                output.push(c);
                last_index = index + preferred_quote.len_utf8();
            }
        }
    }

    if last_index == 0 {
        Cow::Borrowed(input)
    } else {
        output.push_str(&input[last_index..]);
        Cow::Owned(output)
    }
}

#[derive(Clone, Debug)]
struct CharIndicesWithOffset<'str> {
    chars: std::str::Chars<'str>,
    next_offset: usize,
}

impl<'str> CharIndicesWithOffset<'str> {
    fn new(input: &'str str, start_offset: usize) -> Self {
        Self {
            chars: input[start_offset..].chars(),
            next_offset: start_offset,
        }
    }
}

impl Iterator for CharIndicesWithOffset<'_> {
    type Item = (usize, char);

    fn next(&mut self) -> Option<Self::Item> {
        self.chars.next().map(|c| {
            let index = self.next_offset;
            self.next_offset += c.len_utf8();
            (index, c)
        })
    }
}

impl FusedIterator for CharIndicesWithOffset<'_> {}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
enum UnicodeEscape {
    /// A hex escape sequence of either 2 (`\x`), 4 (`\u`) or 8 (`\U`) hex characters.
    Hex(usize),

    /// An escaped unicode name (`\N{name}`)
    CharacterName,
}

impl UnicodeEscape {
    fn new(first: char, allow_unicode: bool) -> Option<UnicodeEscape> {
        Some(match first {
            'x' => UnicodeEscape::Hex(2),
            'u' if allow_unicode => UnicodeEscape::Hex(4),
            'U' if allow_unicode => UnicodeEscape::Hex(8),
            'N' if allow_unicode => UnicodeEscape::CharacterName,
            _ => return None,
        })
    }

    /// Normalises `\u..`, `\U..`, `\x..` and `\N{..}` escape sequences to:
    ///
    /// * `\u`, `\U'` and `\x`: To use lower case for the characters `a-f`.
    /// * `\N`: To use uppercase letters
    fn normalize(self, input: &str) -> Option<Cow<str>> {
        let mut normalised = String::new();

        let len = match self {
            UnicodeEscape::Hex(len) => {
                // It's not a valid escape sequence if the input string has fewer characters
                // left than required by the escape sequence.
                if input.len() < len {
                    return None;
                }

                for (index, c) in input.char_indices().take(len) {
                    match c {
                        '0'..='9' | 'a'..='f' => {
                            if !normalised.is_empty() {
                                normalised.push(c);
                            }
                        }
                        'A'..='F' => {
                            if normalised.is_empty() {
                                normalised.reserve(len);
                                normalised.push_str(&input[..index]);
                                normalised.push(c.to_ascii_lowercase());
                            } else {
                                normalised.push(c.to_ascii_lowercase());
                            }
                        }
                        _ => {
                            // not a valid escape sequence
                            return None;
                        }
                    }
                }

                len
            }
            UnicodeEscape::CharacterName => {
                let mut char_indices = input.char_indices();

                if !matches!(char_indices.next(), Some((_, '{'))) {
                    return None;
                }

                loop {
                    if let Some((index, c)) = char_indices.next() {
                        match c {
                            '}' => {
                                if !normalised.is_empty() {
                                    normalised.push('}');
                                }

                                // Name must be at least two characters long.
                                if index < 3 {
                                    return None;
                                }

                                break index + '}'.len_utf8();
                            }
                            '0'..='9' | 'A'..='Z' | ' ' | '-' => {
                                if !normalised.is_empty() {
                                    normalised.push(c);
                                }
                            }
                            'a'..='z' => {
                                if normalised.is_empty() {
                                    normalised.reserve(c.len_utf8() + '}'.len_utf8());
                                    normalised.push_str(&input[..index]);
                                    normalised.push(c.to_ascii_uppercase());
                                } else {
                                    normalised.push(c.to_ascii_uppercase());
                                }
                            }
                            _ => {
                                // Seems like an invalid escape sequence, don't normalise it.
                                return None;
                            }
                        }
                    } else {
                        // Unterminated escape sequence, don't normalise it.
                        return None;
                    }
                }
            }
        };

        Some(if normalised.is_empty() {
            Cow::Borrowed(&input[..len])
        } else {
            Cow::Owned(normalised)
        })
    }
}

/// Returns `true` if `string` is an f-string part that contains a debug expression that uses quotes
/// and the format target is pre Python 312
/// We can't join f-strings where:
///
/// ```python
/// f"{10 + len('bar')=}"
/// f'{10 + len("bar")=}'
/// f""""{10 + len('''bar''')=}"""
/// ```
pub(super) fn is_fstring_with_quoted_debug_expression(
    fstring: &FString,
    context: &PyFormatContext,
) -> bool {
    fstring.elements.expressions().any(|expression| {
        if expression.debug_text.is_some() {
            let content = context.source().slice(expression);
            contains_opposite_quote(content, fstring.flags.into())
        } else {
            false
        }
    })
}

/// Returns `true` if `string` has any f-string expression element (direct or nested) with a debug expression and a format spec
/// that contains the opposite quote. It's important to preserve the quote style for those f-strings
/// because changing the quote style would result in invalid syntax.
///
/// ```python
/// f'{1=: "abcd \'\'}'
/// f'{x=:a{y:"abcd"}}'
/// f'{x=:a{y:{z:"abcd"}}}'
/// ```
pub(super) fn is_fstring_with_quoted_format_spec_and_debug(
    fstring: &FString,
    context: &PyFormatContext,
) -> bool {
    fn has_format_spec_with_opposite_quote(
        elements: &FStringElements,
        flags: FStringFlags,
        context: &PyFormatContext,
        in_debug: bool,
    ) -> bool {
        elements.iter().any(|element| match element {
            FStringElement::Literal(literal) => {
                let content = context.source().slice(literal);

                in_debug && contains_opposite_quote(content, flags.into())
            }
            FStringElement::Expression(expression) => {
                expression.format_spec.as_deref().is_some_and(|spec| {
                    has_format_spec_with_opposite_quote(
                        &spec.elements,
                        flags,
                        context,
                        in_debug || expression.debug_text.is_some(),
                    )
                })
            }
        })
    }

    fstring.elements.expressions().any(|expression| {
        if let Some(spec) = expression.format_spec.as_deref() {
            return has_format_spec_with_opposite_quote(
                &spec.elements,
                fstring.flags,
                context,
                expression.debug_text.is_some(),
            );
        }

        false
    })
}

/// Tests if the `fstring` contains any triple quoted string, byte, or f-string literal that
/// contains a quote character opposite to its own quote character.
///
/// ```python
/// f'{"""other " """}'
/// ```
///
/// We can't flip the quote of the outer f-string because it would result in invalid syntax:
/// ```python
/// f"{'''other " '''}'
/// ```
pub(super) fn is_fstring_with_triple_quoted_literal_expression_containing_quotes(
    fstring: &FString,
    context: &PyFormatContext,
) -> bool {
    struct Visitor<'a> {
        context: &'a PyFormatContext<'a>,
        found: bool,
    }

    impl Visitor<'_> {
        fn visit_string_like_part(&mut self, part: StringLikePart) {
            if !part.flags().is_triple_quoted() || self.found {
                return;
            }

            let contains_quotes = match part {
                StringLikePart::String(_) | StringLikePart::Bytes(_) => {
                    self.contains_quote(part.content_range(), part.flags())
                }
                StringLikePart::FString(fstring) => {
                    let mut contains_quotes = false;
                    for literal in fstring.elements.literals() {
                        if self.contains_quote(literal.range(), fstring.flags.into()) {
                            contains_quotes = true;
                            break;
                        }
                    }

                    contains_quotes
                }
            };

            if contains_quotes {
                self.found = true;
            }
        }

        fn contains_quote(&self, range: TextRange, flags: AnyStringFlags) -> bool {
            self.context.source()[range].contains(flags.quote_style().as_char())
        }
    }

    impl SourceOrderVisitor<'_> for Visitor<'_> {
        fn visit_f_string(&mut self, f_string: &FString) {
            self.visit_string_like_part(StringLikePart::FString(f_string));
        }

        fn visit_string_literal(&mut self, string_literal: &StringLiteral) {
            self.visit_string_like_part(StringLikePart::String(string_literal));
        }

        fn visit_bytes_literal(&mut self, bytes_literal: &BytesLiteral) {
            self.visit_string_like_part(StringLikePart::Bytes(bytes_literal));
        }
    }

    let mut visitor = Visitor {
        context,
        found: false,
    };

    ruff_python_ast::visitor::source_order::walk_f_string(&mut visitor, fstring);

    visitor.found
}

fn contains_opposite_quote(content: &str, flags: AnyStringFlags) -> bool {
    if flags.is_triple_quoted() {
        match flags.quote_style() {
            Quote::Single => content.contains(r#"""""#),
            Quote::Double => content.contains("'''"),
        }
    } else {
        let mut rest = content;

        while let Some(index) = rest.find(flags.quote_style().opposite().as_char()) {
            // Quotes in raw strings can't be escaped
            if flags.is_raw_string() {
                return true;
            }

            // Only if the quote isn't escaped
            if rest[..index]
                .chars()
                .rev()
                .take_while(|c| *c == '\\')
                .count()
                % 2
                == 0
            {
                return true;
            }

            rest = &rest[index + flags.quote_style().opposite().as_char().len_utf8()..];
        }

        false
    }
}

#[cfg(test)]
mod tests {
    use std::borrow::Cow;

    use ruff_python_ast::{
        str::{Quote, TripleQuotes},
        str_prefix::{AnyStringPrefix, ByteStringPrefix},
        AnyStringFlags,
    };

    use crate::string::normalize_string;

    use super::UnicodeEscape;

    #[test]
    fn normalize_32_escape() {
        let escape_sequence = UnicodeEscape::new('U', true).unwrap();

        assert_eq!(
            Some(Cow::Owned("0001f60e".to_string())),
            escape_sequence.normalize("0001F60E")
        );
    }

    #[test]
    fn normalize_hex_in_byte_string() {
        let input = r"\x89\x50\x4E\x47\x0D\x0A\x1A\x0A";

        let normalized = normalize_string(
            input,
            0,
            AnyStringFlags::new(
                AnyStringPrefix::Bytes(ByteStringPrefix::Regular),
                Quote::Double,
                TripleQuotes::No,
            ),
            false,
        );

        assert_eq!(r"\x89\x50\x4e\x47\x0d\x0a\x1a\x0a", &normalized);
    }
}
