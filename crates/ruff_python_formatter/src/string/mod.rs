use std::borrow::Cow;
use std::iter::FusedIterator;

use bitflags::bitflags;
use memchr::memchr2;

use ruff_formatter::{format_args, write};
use ruff_python_ast::{
    self as ast, Expr, ExprBytesLiteral, ExprFString, ExprStringLiteral, ExpressionRef,
};
use ruff_python_ast::{AnyNodeRef, StringLiteral};
use ruff_source_file::Locator;
use ruff_text_size::{Ranged, TextLen, TextRange, TextSize};

use crate::comments::{leading_comments, trailing_comments};
use crate::expression::expr_f_string::f_string_quoting;
use crate::expression::parentheses::in_parentheses_only_soft_line_break_or_space;
use crate::other::f_string::FormatFString;
use crate::other::string_literal::{FormatStringLiteral, StringLiteralKind};
use crate::prelude::*;
use crate::QuoteStyle;

pub(crate) mod docstring;

#[derive(Copy, Clone, Debug, Default)]
pub(crate) enum Quoting {
    #[default]
    CanChange,
    Preserve,
}

/// Represents any kind of string expression. This could be either a string,
/// bytes or f-string.
#[derive(Copy, Clone, Debug)]
pub(crate) enum AnyString<'a> {
    String(&'a ExprStringLiteral),
    Bytes(&'a ExprBytesLiteral),
    FString(&'a ExprFString),
}

impl<'a> AnyString<'a> {
    /// Creates a new [`AnyString`] from the given [`Expr`].
    ///
    /// Returns `None` if the expression is not either a string, bytes or f-string.
    pub(crate) fn from_expression(expression: &'a Expr) -> Option<AnyString<'a>> {
        match expression {
            Expr::StringLiteral(string) => Some(AnyString::String(string)),
            Expr::BytesLiteral(bytes) => Some(AnyString::Bytes(bytes)),
            Expr::FString(fstring) => Some(AnyString::FString(fstring)),
            _ => None,
        }
    }

    /// Returns `true` if the string is implicitly concatenated.
    pub(crate) fn is_implicit_concatenated(self) -> bool {
        match self {
            Self::String(ExprStringLiteral { value, .. }) => value.is_implicit_concatenated(),
            Self::Bytes(ExprBytesLiteral { value, .. }) => value.is_implicit_concatenated(),
            Self::FString(ExprFString { value, .. }) => value.is_implicit_concatenated(),
        }
    }

    /// Returns the quoting to be used for this string.
    fn quoting(self, locator: &Locator<'_>) -> Quoting {
        match self {
            Self::String(_) | Self::Bytes(_) => Quoting::CanChange,
            Self::FString(f_string) => f_string_quoting(f_string, locator),
        }
    }

    /// Returns a vector of all the [`AnyStringPart`] of this string.
    fn parts(self, quoting: Quoting) -> AnyStringPartsIter<'a> {
        match self {
            Self::String(ExprStringLiteral { value, .. }) => {
                AnyStringPartsIter::String(value.iter())
            }
            Self::Bytes(ExprBytesLiteral { value, .. }) => AnyStringPartsIter::Bytes(value.iter()),
            Self::FString(ExprFString { value, .. }) => {
                AnyStringPartsIter::FString(value.iter(), quoting)
            }
        }
    }

    pub(crate) fn is_multiline(self, source: &str) -> bool {
        match self {
            AnyString::String(_) | AnyString::Bytes(_) => {
                let contents = &source[self.range()];
                let prefix = StringPrefix::parse(contents);
                let quotes = StringQuotes::parse(
                    &contents[TextRange::new(prefix.text_len(), contents.text_len())],
                );

                quotes.is_some_and(StringQuotes::is_triple)
                    && memchr2(b'\n', b'\r', contents.as_bytes()).is_some()
            }
            AnyString::FString(fstring) => {
                memchr2(b'\n', b'\r', source[fstring.range].as_bytes()).is_some()
            }
        }
    }
}

impl Ranged for AnyString<'_> {
    fn range(&self) -> TextRange {
        match self {
            Self::String(expr) => expr.range(),
            Self::Bytes(expr) => expr.range(),
            Self::FString(expr) => expr.range(),
        }
    }
}

impl<'a> From<&AnyString<'a>> for AnyNodeRef<'a> {
    fn from(value: &AnyString<'a>) -> Self {
        match value {
            AnyString::String(expr) => AnyNodeRef::ExprStringLiteral(expr),
            AnyString::Bytes(expr) => AnyNodeRef::ExprBytesLiteral(expr),
            AnyString::FString(expr) => AnyNodeRef::ExprFString(expr),
        }
    }
}

impl<'a> From<AnyString<'a>> for AnyNodeRef<'a> {
    fn from(value: AnyString<'a>) -> Self {
        AnyNodeRef::from(&value)
    }
}

impl<'a> From<&AnyString<'a>> for ExpressionRef<'a> {
    fn from(value: &AnyString<'a>) -> Self {
        match value {
            AnyString::String(expr) => ExpressionRef::StringLiteral(expr),
            AnyString::Bytes(expr) => ExpressionRef::BytesLiteral(expr),
            AnyString::FString(expr) => ExpressionRef::FString(expr),
        }
    }
}

enum AnyStringPartsIter<'a> {
    String(std::slice::Iter<'a, StringLiteral>),
    Bytes(std::slice::Iter<'a, ast::BytesLiteral>),
    FString(std::slice::Iter<'a, ast::FStringPart>, Quoting),
}

impl<'a> Iterator for AnyStringPartsIter<'a> {
    type Item = AnyStringPart<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        let part = match self {
            Self::String(inner) => {
                let part = inner.next()?;
                AnyStringPart::String {
                    part,
                    layout: StringLiteralKind::String,
                }
            }
            Self::Bytes(inner) => AnyStringPart::Bytes(inner.next()?),
            Self::FString(inner, quoting) => {
                let part = inner.next()?;
                match part {
                    ast::FStringPart::Literal(string_literal) => AnyStringPart::String {
                        part: string_literal,
                        layout: StringLiteralKind::InImplicitlyConcatenatedFString(*quoting),
                    },
                    ast::FStringPart::FString(f_string) => AnyStringPart::FString {
                        part: f_string,
                        quoting: *quoting,
                    },
                }
            }
        };

        Some(part)
    }
}

impl FusedIterator for AnyStringPartsIter<'_> {}

/// Represents any kind of string which is part of an implicitly concatenated
/// string. This could be either a string, bytes or f-string.
///
/// This is constructed from the [`AnyString::parts`] method on [`AnyString`].
#[derive(Clone, Debug)]
enum AnyStringPart<'a> {
    String {
        part: &'a ast::StringLiteral,
        layout: StringLiteralKind,
    },
    Bytes(&'a ast::BytesLiteral),
    FString {
        part: &'a ast::FString,
        quoting: Quoting,
    },
}

impl<'a> From<&AnyStringPart<'a>> for AnyNodeRef<'a> {
    fn from(value: &AnyStringPart<'a>) -> Self {
        match value {
            AnyStringPart::String { part, .. } => AnyNodeRef::StringLiteral(part),
            AnyStringPart::Bytes(part) => AnyNodeRef::BytesLiteral(part),
            AnyStringPart::FString { part, .. } => AnyNodeRef::FString(part),
        }
    }
}

impl Ranged for AnyStringPart<'_> {
    fn range(&self) -> TextRange {
        match self {
            Self::String { part, .. } => part.range(),
            Self::Bytes(part) => part.range(),
            Self::FString { part, .. } => part.range(),
        }
    }
}

impl Format<PyFormatContext<'_>> for AnyStringPart<'_> {
    fn fmt(&self, f: &mut PyFormatter) -> FormatResult<()> {
        match self {
            AnyStringPart::String { part, layout } => {
                FormatStringLiteral::new(part, *layout).fmt(f)
            }
            AnyStringPart::Bytes(bytes_literal) => bytes_literal.format().fmt(f),
            AnyStringPart::FString { part, quoting } => FormatFString::new(part, *quoting).fmt(f),
        }
    }
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

    /// Computes the strings preferred quotes and normalizes its content.
    ///
    /// The parent docstring quote style should be set when formatting a code
    /// snippet within the docstring. The quote style should correspond to the
    /// style of quotes used by said docstring. Normalization will ensure the
    /// quoting styles don't conflict.
    pub(crate) fn normalize<'a>(
        self,
        quoting: Quoting,
        locator: &'a Locator,
        configured_style: QuoteStyle,
        parent_docstring_quote_char: Option<QuoteChar>,
        normalize_hex: bool,
    ) -> NormalizedString<'a> {
        // Per PEP 8, always prefer double quotes for triple-quoted strings.
        let preferred_style = if self.quotes.triple {
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
            if let Some(quote) = parent_docstring_quote_char {
                QuoteStyle::from(quote.invert())
            } else {
                QuoteStyle::Double
            }
        } else {
            configured_style
        };

        let raw_content = &locator.slice(self.content_range);

        let quotes = match quoting {
            Quoting::Preserve => self.quotes,
            Quoting::CanChange => {
                if let Some(preferred_quote) = QuoteChar::from_style(preferred_style) {
                    if self.prefix.is_raw_string() {
                        choose_quotes_raw(raw_content, self.quotes, preferred_quote)
                    } else {
                        choose_quotes(raw_content, self.quotes, preferred_quote)
                    }
                } else {
                    self.quotes
                }
            }
        };

        let normalized = normalize_string(raw_content, quotes, self.prefix, normalize_hex);

        NormalizedString {
            prefix: self.prefix,
            content_range: self.content_range,
            text: normalized,
            quotes,
        }
    }
}

#[derive(Debug)]
pub(crate) struct NormalizedString<'a> {
    prefix: StringPrefix,

    /// The quotes of the normalized string (preferred quotes)
    quotes: StringQuotes,

    /// The range of the string's content in the source (minus prefix and quotes).
    content_range: TextRange,

    /// The normalized text
    text: Cow<'a, str>,
}

impl Ranged for NormalizedString<'_> {
    fn range(&self) -> TextRange {
        self.content_range
    }
}

impl Format<PyFormatContext<'_>> for NormalizedString<'_> {
    fn fmt(&self, f: &mut Formatter<PyFormatContext<'_>>) -> FormatResult<()> {
        write!(f, [self.prefix, self.quotes])?;
        match &self.text {
            Cow::Borrowed(_) => {
                source_text_slice(self.range()).fmt(f)?;
            }
            Cow::Owned(normalized) => {
                text(normalized).fmt(f)?;
            }
        }
        self.quotes.fmt(f)
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

/// Choose the appropriate quote style for a raw string.
///
/// The preferred quote style is chosen unless the string contains unescaped quotes of the
/// preferred style. For example, `r"foo"` is chosen over `r'foo'` if the preferred quote
/// style is double quotes.
fn choose_quotes_raw(
    input: &str,
    quotes: StringQuotes,
    preferred_quote: QuoteChar,
) -> StringQuotes {
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
                if !quotes.triple {
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

    StringQuotes {
        triple: quotes.triple,
        quote_char: if contains_unescaped_configured_quotes {
            quotes.quote_char
        } else {
            preferred_quote
        },
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
fn choose_quotes(input: &str, quotes: StringQuotes, preferred_quote: QuoteChar) -> StringQuotes {
    let quote = if quotes.triple {
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
            quotes.quote_char
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
            QuoteChar::Single => {
                if single_quotes > double_quotes {
                    QuoteChar::Double
                } else {
                    QuoteChar::Single
                }
            }
            QuoteChar::Double => {
                if double_quotes > single_quotes {
                    QuoteChar::Single
                } else {
                    QuoteChar::Double
                }
            }
        }
    };

    StringQuotes {
        triple: quotes.triple,
        quote_char: quote,
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

/// Adds the necessary quote escapes and removes unnecessary escape sequences when quoting `input`
/// with the provided [`StringQuotes`] style.
///
/// Returns the normalized string and whether it contains new lines.
fn normalize_string(
    input: &str,
    quotes: StringQuotes,
    prefix: StringPrefix,
    normalize_hex: bool,
) -> Cow<str> {
    // The normalized string if `input` is not yet normalized.
    // `output` must remain empty if `input` is already normalized.
    let mut output = String::new();
    // Tracks the last index of `input` that has been written to `output`.
    // If `last_index` is `0` at the end, then the input is already normalized and can be returned as is.
    let mut last_index = 0;

    let quote = quotes.quote_char;
    let preferred_quote = quote.as_char();
    let opposite_quote = quote.invert().as_char();

    let mut chars = input.char_indices().peekable();

    let is_raw = prefix.is_raw_string();
    let is_fstring = prefix.is_fstring();
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
                    } else if normalize_hex {
                        if let Some(normalised) = UnicodeEscape::new(next, !prefix.is_byte())
                            .and_then(|escape| {
                                escape.normalize(&input[index + c.len_utf8() + next.len_utf8()..])
                            })
                        {
                            // Length of the `\` plus the length of the escape sequence character (`u` | `U` | `x`)
                            let escape_start_len = '\\'.len_utf8() + next.len_utf8();
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

                    if !quotes.triple {
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
            } else if !quotes.triple && c == preferred_quote && formatted_value_nesting == 0 {
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
    use crate::string::{normalize_string, QuoteChar, StringPrefix, StringQuotes, UnicodeEscape};
    use std::borrow::Cow;

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
            StringQuotes {
                triple: false,
                quote_char: QuoteChar::Double,
            },
            StringPrefix::BYTE,
            true,
        );

        assert_eq!(r"\x89\x50\x4e\x47\x0d\x0a\x1a\x0a", &normalized);
    }
}
