use icu_normalizer::ComposingNormalizer;
use icu_properties::{
    CodePointSetData,
    props::{DefaultIgnorableCodePoint, NfcInert},
};
use itertools::Either;
use ruff_python_ast::{
    BytesLiteralFlags, StringFlags, StringLiteralFlags,
    str::{Quote, TripleQuotes},
};

pub struct EscapeLayout {
    pub quote: Quote,
    pub len: Option<usize>,
}

pub trait Escape {
    fn source_len(&self) -> usize;
    fn layout(&self) -> &EscapeLayout;
    fn changed(&self) -> bool {
        self.layout().len != Some(self.source_len())
    }

    fn write_source(&self, formatter: &mut impl std::fmt::Write) -> std::fmt::Result;
    fn write_body_slow(&self, formatter: &mut impl std::fmt::Write) -> std::fmt::Result;
    fn write_body(&self, formatter: &mut impl std::fmt::Write) -> std::fmt::Result {
        if self.changed() {
            self.write_body_slow(formatter)
        } else {
            self.write_source(formatter)
        }
    }
}

/// Returns the outer quotes to use and the number of quotes that need to be
/// escaped.
pub(crate) const fn choose_quote(
    single_count: usize,
    double_count: usize,
    preferred_quote: Quote,
) -> (Quote, usize) {
    let (primary_count, secondary_count) = match preferred_quote {
        Quote::Single => (single_count, double_count),
        Quote::Double => (double_count, single_count),
    };

    // always use primary unless we have primary but no secondary
    let use_secondary = primary_count > 0 && secondary_count == 0;
    if use_secondary {
        (preferred_quote.opposite(), secondary_count)
    } else {
        (preferred_quote, primary_count)
    }
}

pub struct UnicodeEscape<'a> {
    source: &'a str,
    layout: EscapeLayout,
    escape_for_display: bool,
}

impl<'a> UnicodeEscape<'a> {
    #[inline]
    pub fn with_preferred_quote(source: &'a str, quote: Quote) -> Self {
        let layout = Self::repr_layout(source, quote);
        Self {
            source,
            layout,
            escape_for_display: false,
        }
    }

    /// Configures the representation to escape [default-ignorable characters], which may be
    /// invisible. It also escapes combining marks that would otherwise attach to the opening quote
    /// or an escape sequence in the output.
    ///
    /// To distinguish [canonically equivalent] strings, it escapes non-ASCII characters that can
    /// participate in normalization in parts of the string that are not in [NFC]. Characters
    /// with the [`NfcInert`] property cannot interact with adjacent characters during normalization
    /// and separate these parts.
    ///
    /// [default-ignorable characters]: https://www.unicode.org/reports/tr44/#Default_Ignorable_Code_Point
    /// [NFC]: https://www.unicode.org/reports/tr15/#Norm_Forms
    /// [canonically equivalent]: https://www.unicode.org/reports/tr15/#Canon_Compat_Equivalence
    /// [`NfcInert`]: https://docs.rs/icu_properties/latest/icu_properties/props/struct.NfcInert.html
    #[must_use]
    pub fn escape_for_display(mut self) -> Self {
        // ASCII is already NFC and has no combining marks or default-ignorable characters, so it
        // needs no additional escaping for display.
        if self.escape_for_display || self.source.is_ascii() {
            return self;
        }
        self.escape_for_display = true;
        let mut follows_syntax = true;
        for (ch, escape_for_normalization) in
            Self::display_chars(self.source, self.escape_for_display)
        {
            let escape = self.display_escape(ch, follows_syntax, escape_for_normalization);
            follows_syntax = escape.next_follows_syntax;
            if escape.should_escape {
                let extra = Self::escaped_codepoint_len(ch) - ch.len_utf8();
                self.layout.len = self
                    .layout
                    .len
                    .and_then(|len| len.checked_add(extra))
                    .filter(|&len| len <= isize::MAX as usize - Self::REPR_RESERVED_LEN);
            }
        }
        self
    }

    #[inline]
    pub fn new_repr(source: &'a str) -> Self {
        Self::with_preferred_quote(source, Quote::Single)
    }

    #[inline]
    pub fn str_repr<'r>(&'a self, triple_quotes: TripleQuotes) -> StrRepr<'r, 'a> {
        StrRepr {
            escape: self,
            triple_quotes,
        }
    }
}

pub struct StrRepr<'r, 'a> {
    escape: &'r UnicodeEscape<'a>,
    triple_quotes: TripleQuotes,
}

impl StrRepr<'_, '_> {
    pub fn write(&self, formatter: &mut impl std::fmt::Write) -> std::fmt::Result {
        let flags = StringLiteralFlags::empty()
            .with_quote_style(self.escape.layout().quote)
            .with_triple_quotes(self.triple_quotes);
        formatter.write_str(flags.quote_str())?;
        self.escape.write_body(formatter)?;
        formatter.write_str(flags.quote_str())?;
        Ok(())
    }

    pub fn to_string(&self) -> Option<String> {
        let mut s = String::with_capacity(self.escape.layout().len?);
        self.write(&mut s).unwrap();
        Some(s)
    }
}

impl std::fmt::Display for StrRepr<'_, '_> {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.write(formatter)
    }
}

/// Whether to apply additional escaping to the current character, and, when display escaping is
/// enabled, whether the next character would immediately follow an escape sequence in the output.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct DisplayEscape {
    should_escape: bool,
    next_follows_syntax: bool,
}

impl UnicodeEscape<'_> {
    const REPR_RESERVED_LEN: usize = 2; // for quotes

    #[expect(clippy::cast_possible_wrap, clippy::cast_sign_loss)]
    pub fn repr_layout(source: &str, preferred_quote: Quote) -> EscapeLayout {
        Self::output_layout_with_checker(source, preferred_quote, |a, b| {
            Some((a as isize).checked_add(b as isize)? as usize)
        })
    }

    fn output_layout_with_checker(
        source: &str,
        preferred_quote: Quote,
        length_add: impl Fn(usize, usize) -> Option<usize>,
    ) -> EscapeLayout {
        let mut out_len = Self::REPR_RESERVED_LEN;
        let mut single_count = 0;
        let mut double_count = 0;

        for ch in source.chars() {
            let incr = match ch {
                '\'' => {
                    single_count += 1;
                    1
                }
                '"' => {
                    double_count += 1;
                    1
                }
                c => Self::escaped_char_len(c),
            };
            let Some(new_len) = length_add(out_len, incr) else {
                #[cold]
                fn stop(
                    single_count: usize,
                    double_count: usize,
                    preferred_quote: Quote,
                ) -> EscapeLayout {
                    EscapeLayout {
                        quote: choose_quote(single_count, double_count, preferred_quote).0,
                        len: None,
                    }
                }
                return stop(single_count, double_count, preferred_quote);
            };
            out_len = new_len;
        }

        let (quote, num_escaped_quotes) = choose_quote(single_count, double_count, preferred_quote);
        // we'll be adding backslashes in front of the existing inner quotes
        let Some(out_len) = length_add(out_len, num_escaped_quotes) else {
            return EscapeLayout { quote, len: None };
        };

        EscapeLayout {
            quote,
            len: Some(out_len - Self::REPR_RESERVED_LEN),
        }
    }

    fn escaped_char_len(ch: char) -> usize {
        match ch {
            '\\' | '\t' | '\r' | '\n' => 2,
            ch if ch < ' ' || ch as u32 == 0x7f => 4, // \xHH
            ch if ch.is_ascii() => 1,
            ch if crate::char::is_printable(ch) => {
                // max = std::cmp::max(ch, max);
                ch.len_utf8()
            }
            ch => Self::escaped_codepoint_len(ch),
        }
    }

    /// Returns the length of a hexadecimal Python escape for the character.
    const fn escaped_codepoint_len(ch: char) -> usize {
        match ch as u32 {
            0..=0xff => 4,       // \xHH
            0x100..=0xffff => 6, // \uHHHH
            _ => 10,             // \UHHHHHHHH
        }
    }

    /// Yields characters and whether to escape them for display to distinguish canonically
    /// equivalent strings, when display escaping is enabled.
    ///
    /// Characters with the [`NfcInert`] property cannot interact with adjacent characters during
    /// normalization, so they provide boundaries between independently normalized segments.
    ///
    /// [`NfcInert`]: https://docs.rs/icu_properties/latest/icu_properties/props/struct.NfcInert.html
    fn display_chars(source: &str, escape_for_display: bool) -> impl Iterator<Item = (char, bool)> {
        if !escape_for_display || ComposingNormalizer::new_nfc().is_normalized(source) {
            return Either::Left(source.chars().map(|ch| (ch, false)));
        }

        let inert = CodePointSetData::new::<NfcInert>();
        Either::Right(
            source
                .split_inclusive(move |ch| inert.contains(ch))
                .flat_map(move |segment| {
                    let escape = !ComposingNormalizer::new_nfc().is_normalized(segment);
                    segment.chars().map(move |ch| {
                        let escape = escape
                            && !ch.is_ascii()
                            && !inert.contains(ch)
                            && crate::char::is_printable(ch);
                        (ch, escape)
                    })
                }),
        )
    }

    /// Returns whether `ch` needs additional escaping and, when display escaping is enabled,
    /// whether the next character would immediately follow an escape sequence in the output.
    ///
    /// When display escaping is enabled, printable default-ignorable characters are escaped.
    /// `follows_syntax` is true if `ch` would immediately follow the opening quote or an escape
    /// sequence; in that case, combining marks are also escaped. `escape_for_normalization` marks
    /// characters in a non-NFC segment that can participate in normalization.
    fn display_escape(
        &self,
        ch: char,
        follows_syntax: bool,
        escape_for_normalization: bool,
    ) -> DisplayEscape {
        if !self.escape_for_display {
            return DisplayEscape {
                should_escape: false,
                next_follows_syntax: follows_syntax,
            };
        }
        let should_escape = escape_for_normalization
            || (!ch.is_ascii()
                && ((follows_syntax && crate::char::is_combining_mark(ch))
                    || (CodePointSetData::new::<DefaultIgnorableCodePoint>().contains(ch)
                        && crate::char::is_printable(ch))));
        DisplayEscape {
            should_escape,
            next_follows_syntax: should_escape
                || ch == self.layout.quote.as_char()
                || Self::escaped_char_len(ch) != ch.len_utf8(),
        }
    }

    fn write_char(
        ch: char,
        quote: Quote,
        force_escape: bool,
        formatter: &mut impl std::fmt::Write,
    ) -> std::fmt::Result {
        match ch {
            '\n' => formatter.write_str("\\n"),
            '\t' => formatter.write_str("\\t"),
            '\r' => formatter.write_str("\\r"),
            // these 2 branches *would* be handled below, but we shouldn't have to do a
            // unicodedata lookup just for ascii characters
            '\x20'..='\x7e' => {
                // printable ascii range
                if ch == quote.as_char() || ch == '\\' {
                    formatter.write_char('\\')?;
                }
                formatter.write_char(ch)
            }
            ch if ch.is_ascii() => {
                write!(formatter, "\\x{:02x}", ch as u8)
            }
            ch if !force_escape && crate::char::is_printable(ch) => formatter.write_char(ch),
            '\0'..='\u{ff}' => {
                write!(formatter, "\\x{:02x}", ch as u32)
            }
            '\0'..='\u{ffff}' => {
                write!(formatter, "\\u{:04x}", ch as u32)
            }
            _ => {
                write!(formatter, "\\U{:08x}", ch as u32)
            }
        }
    }
}

impl Escape for UnicodeEscape<'_> {
    fn source_len(&self) -> usize {
        self.source.len()
    }

    fn layout(&self) -> &EscapeLayout {
        &self.layout
    }

    fn write_source(&self, formatter: &mut impl std::fmt::Write) -> std::fmt::Result {
        formatter.write_str(self.source)
    }

    #[cold]
    fn write_body_slow(&self, formatter: &mut impl std::fmt::Write) -> std::fmt::Result {
        let mut follows_syntax = true;
        for (ch, escape_for_normalization) in
            Self::display_chars(self.source, self.escape_for_display)
        {
            let display_escape = self.display_escape(ch, follows_syntax, escape_for_normalization);
            follows_syntax = display_escape.next_follows_syntax;
            Self::write_char(
                ch,
                self.layout.quote,
                display_escape.should_escape,
                formatter,
            )?;
        }
        Ok(())
    }
}

pub struct AsciiEscape<'a> {
    source: &'a [u8],
    layout: EscapeLayout,
}

impl<'a> AsciiEscape<'a> {
    #[inline]
    pub fn new(source: &'a [u8], layout: EscapeLayout) -> Self {
        Self { source, layout }
    }
    #[inline]
    pub fn with_preferred_quote(source: &'a [u8], quote: Quote) -> Self {
        let layout = Self::repr_layout(source, quote);
        Self { source, layout }
    }
    #[inline]
    pub fn new_repr(source: &'a [u8]) -> Self {
        Self::with_preferred_quote(source, Quote::Single)
    }
    #[inline]
    pub fn bytes_repr<'r>(&'a self, triple_quotes: TripleQuotes) -> BytesRepr<'r, 'a> {
        BytesRepr {
            escape: self,
            triple_quotes,
        }
    }
}

impl AsciiEscape<'_> {
    #[expect(clippy::cast_possible_wrap, clippy::cast_sign_loss)]
    pub fn repr_layout(source: &[u8], preferred_quote: Quote) -> EscapeLayout {
        Self::output_layout_with_checker(source, preferred_quote, 3, |a, b| {
            Some((a as isize).checked_add(b as isize)? as usize)
        })
    }

    fn output_layout_with_checker(
        source: &[u8],
        preferred_quote: Quote,
        reserved_len: usize,
        length_add: impl Fn(usize, usize) -> Option<usize>,
    ) -> EscapeLayout {
        let mut out_len = reserved_len;
        let mut single_count = 0;
        let mut double_count = 0;

        for ch in source {
            let incr = match ch {
                b'\'' => {
                    single_count += 1;
                    1
                }
                b'"' => {
                    double_count += 1;
                    1
                }
                c => Self::escaped_char_len(*c),
            };
            let Some(new_len) = length_add(out_len, incr) else {
                #[cold]
                fn stop(
                    single_count: usize,
                    double_count: usize,
                    preferred_quote: Quote,
                ) -> EscapeLayout {
                    EscapeLayout {
                        quote: choose_quote(single_count, double_count, preferred_quote).0,
                        len: None,
                    }
                }
                return stop(single_count, double_count, preferred_quote);
            };
            out_len = new_len;
        }

        let (quote, num_escaped_quotes) = choose_quote(single_count, double_count, preferred_quote);
        // we'll be adding backslashes in front of the existing inner quotes
        let Some(out_len) = length_add(out_len, num_escaped_quotes) else {
            return EscapeLayout { quote, len: None };
        };

        EscapeLayout {
            quote,
            len: Some(out_len - reserved_len),
        }
    }

    fn escaped_char_len(ch: u8) -> usize {
        match ch {
            b'\\' | b'\t' | b'\r' | b'\n' => 2,
            0x20..=0x7e => 1,
            _ => 4, // \xHH
        }
    }

    fn write_char(ch: u8, quote: Quote, formatter: &mut impl std::fmt::Write) -> std::fmt::Result {
        match ch {
            b'\t' => formatter.write_str("\\t"),
            b'\n' => formatter.write_str("\\n"),
            b'\r' => formatter.write_str("\\r"),
            0x20..=0x7e => {
                // printable ascii range
                if ch == quote.as_byte() || ch == b'\\' {
                    formatter.write_char('\\')?;
                }
                formatter.write_char(ch as char)
            }
            ch => write!(formatter, "\\x{ch:02x}"),
        }
    }
}

impl Escape for AsciiEscape<'_> {
    fn source_len(&self) -> usize {
        self.source.len()
    }

    fn layout(&self) -> &EscapeLayout {
        &self.layout
    }
    fn write_source(&self, formatter: &mut impl std::fmt::Write) -> std::fmt::Result {
        // OK because function must be called only when source is printable ascii characters.
        let string = std::str::from_utf8(self.source).expect("ASCII bytes");
        formatter.write_str(string)
    }

    #[cold]
    fn write_body_slow(&self, formatter: &mut impl std::fmt::Write) -> std::fmt::Result {
        for ch in self.source {
            Self::write_char(*ch, self.layout().quote, formatter)?;
        }
        Ok(())
    }
}

pub struct BytesRepr<'r, 'a> {
    escape: &'r AsciiEscape<'a>,
    triple_quotes: TripleQuotes,
}

impl BytesRepr<'_, '_> {
    pub fn write(&self, formatter: &mut impl std::fmt::Write) -> std::fmt::Result {
        let flags = BytesLiteralFlags::empty()
            .with_quote_style(self.escape.layout().quote)
            .with_triple_quotes(self.triple_quotes);

        formatter.write_char('b')?;
        formatter.write_str(flags.quote_str())?;
        self.escape.write_body(formatter)?;
        formatter.write_str(flags.quote_str())?;
        Ok(())
    }

    pub fn to_string(&self) -> Option<String> {
        let mut s = String::with_capacity(self.escape.layout().len?);
        self.write(&mut s).unwrap();
        Some(s)
    }
}

impl std::fmt::Display for BytesRepr<'_, '_> {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.write(formatter)
    }
}

#[cfg(test)]
mod unicode_escape_tests {
    use super::*;

    #[test]
    fn changed() {
        fn test(s: &str) -> bool {
            UnicodeEscape::new_repr(s).changed()
        }
        assert!(!test("hello"));
        assert!(!test("'hello'"));
        assert!(!test("\"hello\""));

        assert!(test("'\"hello"));
        assert!(test("hello\n"));
    }

    #[test]
    fn unattached_combining_marks() {
        let source = "\u{0301}q\u{0301}";
        let original = UnicodeEscape::new_repr(source);
        assert!(!original.changed());
        assert_eq!(
            original.str_repr(TripleQuotes::No).to_string().as_deref(),
            Some("'\u{0301}q\u{0301}'")
        );

        let escaped = UnicodeEscape::new_repr(source).escape_for_display();
        assert!(escaped.changed());
        assert_eq!(escaped.layout().len, Some("\\u0301q\u{0301}".len()));
        assert_eq!(
            escaped.str_repr(TripleQuotes::No).to_string().as_deref(),
            Some("'\\u0301q\u{0301}'")
        );
    }

    #[test]
    fn canonically_equivalent_strings() {
        let source = "q\u{0301} e\u{0301}\u{200b}";
        let original = UnicodeEscape::new_repr(source);
        assert_eq!(
            original.str_repr(TripleQuotes::No).to_string().as_deref(),
            Some("'q\u{0301} e\u{0301}\\u200b'")
        );

        let escaped = UnicodeEscape::new_repr(source).escape_for_display();
        assert_eq!(
            escaped.layout().len,
            Some("q\u{0301} e\\u0301\\u200b".len())
        );
        assert_eq!(
            escaped.str_repr(TripleQuotes::No).to_string().as_deref(),
            Some("'q\u{0301} e\\u0301\\u200b'")
        );
    }

    #[test]
    fn canonical_equivalents_have_distinct_normalized_displays() {
        let nfc = icu_normalizer::ComposingNormalizer::new_nfc();
        let nfd = icu_normalizer::DecomposingNormalizer::new_nfd();
        for codepoint in 0..=0x0010_ffff {
            let Some(ch) = char::from_u32(codepoint) else {
                continue;
            };
            let original = ch.to_string();
            let composed = nfc.normalize(&original);
            let decomposed = nfd.normalize(&original);
            let forms = [original.as_str(), composed.as_ref(), decomposed.as_ref()];
            for (i, first) in forms.iter().enumerate() {
                for second in &forms[i + 1..] {
                    if first != second {
                        let first = UnicodeEscape::new_repr(first).escape_for_display();
                        let second = UnicodeEscape::new_repr(second).escape_for_display();
                        let first_repr = format!("{}", first.str_repr(TripleQuotes::No));
                        let second_repr = format!("{}", second.str_repr(TripleQuotes::No));
                        assert_ne!(
                            nfc.normalize(&first_repr),
                            nfc.normalize(&second_repr),
                            "U+{codepoint:04X}: {first_repr:?} and {second_repr:?}"
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn default_ignorable_characters() {
        let source = "a\u{034f}\u{0301}\u{115f}\u{200b}";
        let original = UnicodeEscape::new_repr(source);
        assert_eq!(
            original.str_repr(TripleQuotes::No).to_string().as_deref(),
            Some("'a\u{034f}\u{0301}\u{115f}\\u200b'")
        );

        let escaped = UnicodeEscape::new_repr(source)
            .escape_for_display()
            .escape_for_display();
        assert_eq!(
            escaped.layout().len,
            Some("a\\u034f\\u0301\\u115f\\u200b".len())
        );
        assert_eq!(
            escaped.str_repr(TripleQuotes::No).to_string().as_deref(),
            Some("'a\\u034f\\u0301\\u115f\\u200b'")
        );
    }
}
