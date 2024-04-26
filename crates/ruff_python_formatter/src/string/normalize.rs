use std::borrow::Cow;
use std::iter::FusedIterator;

use ruff_formatter::FormatContext;
use ruff_python_ast::{str::Quote, AnyStringKind};
use ruff_source_file::Locator;
use ruff_text_size::{Ranged, TextRange};

use crate::context::FStringState;
use crate::options::PythonVersion;
use crate::prelude::*;
use crate::preview::is_f_string_formatting_enabled;
use crate::string::{Quoting, StringPart, StringQuotes};
use crate::QuoteStyle;

pub(crate) struct StringNormalizer {
    quoting: Quoting,
    preferred_quote_style: QuoteStyle,
    parent_docstring_quote_char: Option<Quote>,
    f_string_state: FStringState,
    target_version: PythonVersion,
    format_fstring: bool,
}

impl StringNormalizer {
    pub(crate) fn from_context(context: &PyFormatContext<'_>) -> Self {
        Self {
            quoting: Quoting::default(),
            preferred_quote_style: QuoteStyle::default(),
            parent_docstring_quote_char: context.docstring(),
            f_string_state: context.f_string_state(),
            target_version: context.options().target_version(),
            format_fstring: is_f_string_formatting_enabled(context),
        }
    }

    pub(crate) fn with_preferred_quote_style(mut self, quote_style: QuoteStyle) -> Self {
        self.preferred_quote_style = quote_style;
        self
    }

    pub(crate) fn with_quoting(mut self, quoting: Quoting) -> Self {
        self.quoting = quoting;
        self
    }

    fn quoting(&self, string: StringPart) -> Quoting {
        if let FStringState::InsideExpressionElement(context) = self.f_string_state {
            // If we're inside an f-string, we need to make sure to preserve the
            // existing quotes unless we're inside a triple-quoted f-string and
            // the inner string itself isn't triple-quoted. For example:
            //
            // ```python
            // f"""outer {"inner"}"""  # Valid
            // f"""outer {"""inner"""}"""  # Invalid
            // ```
            //
            // Or, if the target version supports PEP 701.
            //
            // The reason to preserve the quotes is based on the assumption that
            // the original f-string is valid in terms of quoting, and we don't
            // want to change that to make it invalid.
            if (context.f_string().kind().is_triple_quoted() && !string.kind().is_triple_quoted())
                || self.target_version.supports_pep_701()
            {
                self.quoting
            } else {
                Quoting::Preserve
            }
        } else {
            self.quoting
        }
    }

    /// Computes the strings preferred quotes.
    pub(crate) fn choose_quotes(&self, string: StringPart, locator: &Locator) -> QuoteSelection {
        let raw_content = locator.slice(string.content_range());
        let first_quote_or_normalized_char_offset = raw_content
            .bytes()
            .position(|b| matches!(b, b'\\' | b'"' | b'\'' | b'\r' | b'{'));
        let string_kind = string.kind();

        let new_kind = match self.quoting(string) {
            Quoting::Preserve => string_kind,
            Quoting::CanChange => {
                // Per PEP 8, always prefer double quotes for triple-quoted strings.
                // Except when using quote-style-preserve.
                let preferred_style = if string_kind.is_triple_quoted() {
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
                    if let Some(quote) = self.parent_docstring_quote_char {
                        QuoteStyle::from(quote.opposite())
                    } else if self.preferred_quote_style.is_preserve() {
                        QuoteStyle::Preserve
                    } else {
                        QuoteStyle::Double
                    }
                } else {
                    self.preferred_quote_style
                };

                if let Ok(preferred_quote) = Quote::try_from(preferred_style) {
                    if let Some(first_quote_or_normalized_char_offset) =
                        first_quote_or_normalized_char_offset
                    {
                        if string_kind.is_raw_string() {
                            choose_quotes_for_raw_string(
                                &raw_content[first_quote_or_normalized_char_offset..],
                                string_kind,
                                preferred_quote,
                            )
                        } else {
                            choose_quotes_impl(
                                &raw_content[first_quote_or_normalized_char_offset..],
                                string_kind,
                                preferred_quote,
                            )
                        }
                    } else {
                        string_kind.with_quote_style(preferred_quote)
                    }
                } else {
                    string_kind
                }
            }
        };

        QuoteSelection {
            kind: new_kind,
            first_quote_or_normalized_char_offset,
        }
    }

    /// Computes the strings preferred quotes and normalizes its content.
    pub(crate) fn normalize<'a>(
        &self,
        string: StringPart,
        locator: &'a Locator,
    ) -> NormalizedString<'a> {
        let raw_content = locator.slice(string.content_range());
        let quote_selection = self.choose_quotes(string, locator);

        let normalized = if let Some(first_quote_or_escape_offset) =
            quote_selection.first_quote_or_normalized_char_offset
        {
            normalize_string(
                raw_content,
                first_quote_or_escape_offset,
                quote_selection.kind,
                // TODO: Remove the `b'{'` in `choose_quotes` when promoting the
                // `format_fstring` preview style
                self.format_fstring,
            )
        } else {
            Cow::Borrowed(raw_content)
        };

        NormalizedString {
            kind: quote_selection.kind,
            content_range: string.content_range(),
            text: normalized,
        }
    }
}

#[derive(Debug)]
pub(crate) struct QuoteSelection {
    kind: AnyStringKind,

    /// Offset to the first quote character or character that needs special handling in [`normalize_string`].
    first_quote_or_normalized_char_offset: Option<usize>,
}

impl QuoteSelection {
    pub(crate) fn kind(&self) -> AnyStringKind {
        self.kind
    }
}

#[derive(Debug)]
pub(crate) struct NormalizedString<'a> {
    /// Holds data about the quotes and prefix of the string
    kind: AnyStringKind,

    /// The range of the string's content in the source (minus prefix and quotes).
    content_range: TextRange,

    /// The normalized text
    text: Cow<'a, str>,
}

impl<'a> NormalizedString<'a> {
    pub(crate) fn text(&self) -> &Cow<'a, str> {
        &self.text
    }

    pub(crate) fn kind(&self) -> AnyStringKind {
        self.kind
    }
}

impl Ranged for NormalizedString<'_> {
    fn range(&self) -> TextRange {
        self.content_range
    }
}

impl Format<PyFormatContext<'_>> for NormalizedString<'_> {
    fn fmt(&self, f: &mut Formatter<PyFormatContext<'_>>) -> FormatResult<()> {
        let quotes = StringQuotes::from(self.kind);
        ruff_formatter::write!(f, [self.kind.prefix(), quotes])?;
        match &self.text {
            Cow::Borrowed(_) => {
                source_text_slice(self.range()).fmt(f)?;
            }
            Cow::Owned(normalized) => {
                text(normalized).fmt(f)?;
            }
        }
        quotes.fmt(f)
    }
}

/// Choose the appropriate quote style for a raw string.
///
/// The preferred quote style is chosen unless the string contains unescaped quotes of the
/// preferred style. For example, `r"foo"` is chosen over `r'foo'` if the preferred quote
/// style is double quotes.
fn choose_quotes_for_raw_string(
    input: &str,
    kind: AnyStringKind,
    preferred_quote: Quote,
) -> AnyStringKind {
    let preferred_quote_char = preferred_quote.as_char();
    let mut chars = input.chars().peekable();
    let contains_unescaped_configured_quotes = loop {
        match chars.next() {
            Some('\\') => {
                // Ignore escaped characters
                chars.next();
            }
            // `"` or `'`
            Some(c) if c == preferred_quote_char => {
                if !kind.is_triple_quoted() {
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
                        if chars.peek().is_none() || chars.peek() == Some(&preferred_quote_char) {
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
    if contains_unescaped_configured_quotes {
        kind
    } else {
        kind.with_quote_style(preferred_quote)
    }
}

/// Choose the appropriate quote style for a string.
///
/// For single quoted strings, the preferred quote style is used, unless the alternative quote style
/// would require fewer escapes.
///
/// For triple quoted strings, the preferred quote style is always used, unless the string contains
/// a triplet of the quote character (e.g., if double quotes are preferred, double quotes will be
/// used unless the string contains `"""`).
fn choose_quotes_impl(input: &str, kind: AnyStringKind, preferred_quote: Quote) -> AnyStringKind {
    let quote = if kind.is_triple_quoted() {
        // True if the string contains a triple quote sequence of the configured quote style.
        let mut uses_triple_quotes = false;
        let mut chars = input.chars().peekable();

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

        if uses_triple_quotes {
            // String contains a triple quote sequence of the configured quote style.
            // Keep the existing quote style.
            kind.quote_style()
        } else {
            preferred_quote
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

        match preferred_quote {
            Quote::Single => {
                if single_quotes > double_quotes {
                    Quote::Double
                } else {
                    Quote::Single
                }
            }
            Quote::Double => {
                if double_quotes > single_quotes {
                    Quote::Single
                } else {
                    Quote::Double
                }
            }
        }
    };

    kind.with_quote_style(quote)
}

/// Adds the necessary quote escapes and removes unnecessary escape sequences when quoting `input`
/// with the provided [`StringQuotes`] style.
///
/// Returns the normalized string and whether it contains new lines.
pub(crate) fn normalize_string(
    input: &str,
    start_offset: usize,
    kind: AnyStringKind,
    format_fstring: bool,
) -> Cow<str> {
    // The normalized string if `input` is not yet normalized.
    // `output` must remain empty if `input` is already normalized.
    let mut output = String::new();
    // Tracks the last index of `input` that has been written to `output`.
    // If `last_index` is `0` at the end, then the input is already normalized and can be returned as is.
    let mut last_index = 0;

    let quote = kind.quote_style();
    let preferred_quote = quote.as_char();
    let opposite_quote = quote.opposite().as_char();

    let mut chars = CharIndicesWithOffset::new(input, start_offset).peekable();

    let is_raw = kind.is_raw_string();
    let is_fstring = !format_fstring && kind.is_f_string();
    let mut formatted_value_nesting = 0u32;

    while let Some((index, c)) = chars.next() {
        if is_fstring && matches!(c, '{' | '}') {
            if chars.peek().copied().is_some_and(|(_, next)| next == c) {
                // Skip over the second character of the double braces
                chars.next();
            } else if c == '{' {
                formatted_value_nesting += 1;
            } else {
                // Safe to assume that `c == '}'` here because of the matched pattern above
                formatted_value_nesting = formatted_value_nesting.saturating_sub(1);
            }
            continue;
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
                        if let Some(normalised) = UnicodeEscape::new(next, !kind.is_byte_string())
                            .and_then(|escape| escape.normalize(&input[index + escape_start_len..]))
                        {
                            let escape_start_offset = index + escape_start_len;
                            if let Cow::Owned(normalised) = &normalised {
                                output.push_str(&input[last_index..escape_start_offset]);
                                output.push_str(normalised);
                                last_index = escape_start_offset + normalised.len();
                            };

                            // Move the `chars` iterator passed the escape sequence.
                            // Simply reassigning `chars` doesn't work because the indices` would
                            // then be off.
                            for _ in 0..next.len_utf8() + normalised.len() {
                                chars.next();
                            }
                        }
                    }

                    if !kind.is_triple_quoted() {
                        #[allow(clippy::if_same_then_else)]
                        if next == opposite_quote && formatted_value_nesting == 0 {
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
            } else if !kind.is_triple_quoted()
                && c == preferred_quote
                && formatted_value_nesting == 0
            {
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

    normalized
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

impl<'str> Iterator for CharIndicesWithOffset<'str> {
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

#[cfg(test)]
mod tests {
    use std::borrow::Cow;

    use ruff_python_ast::{str::Quote, AnyStringKind, AnyStringPrefix, ByteStringPrefix};

    use super::{normalize_string, UnicodeEscape};

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
            AnyStringKind::new(
                AnyStringPrefix::Bytes(ByteStringPrefix::Regular),
                Quote::Double,
                false,
            ),
            true,
        );

        assert_eq!(r"\x89\x50\x4e\x47\x0d\x0a\x1a\x0a", &normalized);
    }
}
