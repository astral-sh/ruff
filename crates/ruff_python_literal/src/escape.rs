use icu_normalizer::ComposingNormalizer;
use icu_properties::{
    CodePointSetData,
    props::{DefaultIgnorableCodePoint, EnumeratedProperty, GraphemeClusterBreak},
};
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
    display_escapes: Option<Vec<usize>>,
}

impl<'a> UnicodeEscape<'a> {
    #[inline]
    pub fn with_preferred_quote(source: &'a str, quote: Quote) -> Self {
        let layout = Self::repr_layout(source, quote);
        Self {
            source,
            layout,
            display_escapes: None,
        }
    }

    /// Configures the representation to distinguish [canonically equivalent] strings and escape
    /// [default-ignorable characters] and other potentially invisible characters. It also prevents
    /// characters from appearing as part of the quotes or escape sequences.
    ///
    /// The resulting representation is in [NFC], but still evaluates to the original string.
    /// For example, `"é"` and `"e\u0301"` are shown differently, even though the strings have
    /// the same NFC form.
    ///
    /// [default-ignorable characters]: https://www.unicode.org/reports/tr44/#Default_Ignorable_Code_Point
    /// [NFC]: https://www.unicode.org/reports/tr15/#Norm_Forms
    /// [canonically equivalent]: https://www.unicode.org/reports/tr15/#Canon_Compat_Equivalence
    #[must_use]
    pub fn escape_for_display(mut self) -> Self {
        if self.display_escapes.is_some() || self.source.is_ascii() {
            return self;
        }
        let display_escapes = Self::display_escapes(self.source, self.layout.quote);
        for &index in &display_escapes {
            if let Some(ch) = self.source[index..].chars().next() {
                let extra = Self::escaped_codepoint_len(ch) - ch.len_utf8();
                self.layout.len = self
                    .layout
                    .len
                    .and_then(|len| len.checked_add(extra))
                    .filter(|&len| len <= isize::MAX as usize - Self::REPR_RESERVED_LEN);
            }
        }
        self.display_escapes = Some(display_escapes);
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

    /// Finds the byte offsets of characters that need escaping beyond Python's ordinary repr.
    ///
    /// ASCII escapes separate runs of characters written without escaping. Within each run, escape
    /// a character if adding it would make the run non-NFC, then start a new run. Characters that
    /// would attach to the resulting escape also need escaping. This leaves unrelated characters
    /// visible even when another part of the string needs an escape.
    fn display_escapes(source: &str, quote: Quote) -> Vec<usize> {
        let normalizer = ComposingNormalizer::new_nfc();
        let check_normalization = !normalizer.is_normalized(source);
        let default_ignorables = CodePointSetData::new::<DefaultIgnorableCodePoint>();

        // The braille blank is printable but often looks like an ordinary space.
        let invisible = |ch| ch == '\u{2800}' || default_ignorables.contains(ch);

        // Characters escaped by the ordinary Python representation and invisible characters end a
        // run regardless of whether it is in NFC.
        let always_escaped = |ch: char| {
            ch == quote.as_char()
                || Self::escaped_char_len(ch) != ch.len_utf8()
                || (!ch.is_ascii() && invisible(ch))
        };

        let mut escapes = Vec::new();
        let mut position = 0;

        // The first character follows the opening quote. After an escape, the next character can
        // likewise attach to the escape's final character rather than to a character in the string.
        let mut follows_syntax = true;

        while let Some(ch) = source[position..].chars().next() {
            let end = position + ch.len_utf8();

            let additional = !ch.is_ascii()
                && crate::char::is_printable(ch)
                && (invisible(ch) || (follows_syntax && Self::attaches_to_syntax(ch)));

            if additional || always_escaped(ch) {
                Self::escape_preceding_prepends(source, position, &mut escapes);

                // The writer handles ordinary Python escapes. Record only the additional escapes
                // needed for display, so their lengths can be accounted for separately.
                if additional {
                    escapes.push(position);
                }

                follows_syntax = true;
                position = end;
                continue;
            }

            let run_end = source[position..]
                .char_indices()
                .find(|(_, ch)| always_escaped(*ch))
                .map(|(index, _)| position + index)
                .unwrap_or(source.len());

            // A substring of an NFC string is also NFC, so an NFC source cannot contain a run
            // that needs an escape for normalization.
            if !check_normalization {
                position = run_end;
                follows_syntax = false;
                continue;
            }

            // In the Rust string `"e\u{301}x"`, `e` is NFC but adding the accent is not. Escaping
            // the accent leaves `e` visible, so with double quotes the display is `"e\u0301x"`.
            while position < run_end {
                let run = &source[position..run_end];

                let Some(index) = Self::first_non_nfc_char(run) else {
                    position = run_end;
                    follows_syntax = false;
                    break;
                };

                position += index;

                if let Some(ch) = source[position..].chars().next() {
                    Self::escape_preceding_prepends(source, position, &mut escapes);
                    escapes.push(position);
                    position += ch.len_utf8();
                    follows_syntax = true;
                }

                // A character immediately following the new escape could attach to the escape's
                // final ASCII character. Escape it as well if it can do so.
                while let Some(ch) = source[position..run_end].chars().next() {
                    if !Self::attaches_to_syntax(ch) {
                        break;
                    }
                    escapes.push(position);
                    position += ch.len_utf8();
                }
            }
        }

        // A trailing Prepend character could attach to the closing quote, even if the final run
        // is already NFC.
        Self::escape_preceding_prepends(source, source.len(), &mut escapes);
        escapes
    }

    /// Records offsets of characters that would attach to the following escape or closing quote.
    fn escape_preceding_prepends(source: &str, end: usize, escapes: &mut Vec<usize>) {
        // Grapheme_Cluster_Break=Prepend characters join the following character. Once one is
        // escaped, a preceding Prepend would join the escape's backslash, so escape that too.
        let start = source[..end]
            .char_indices()
            .rev()
            .take_while(|(index, ch)| {
                GraphemeClusterBreak::for_char(*ch) == GraphemeClusterBreak::Prepend
                    && crate::char::is_printable(*ch)
                    && escapes.last().is_none_or(|last| index > last)
            })
            .last()
            .map(|(index, _)| index)
            .unwrap_or(end);

        escapes.extend(
            source[start..end]
                .char_indices()
                .map(|(index, _)| start + index),
        );
    }

    /// Returns whether a character can join a preceding ASCII quote or escape sequence.
    fn attaches_to_syntax(ch: char) -> bool {
        matches!(
            GraphemeClusterBreak::for_char(ch),
            GraphemeClusterBreak::Extend
                | GraphemeClusterBreak::SpacingMark
                | GraphemeClusterBreak::ZWJ
        )
    }

    /// Returns the byte offset of the first character that makes its prefix of `source` non-NFC,
    /// or `None` if `source` is in NFC.
    fn first_non_nfc_char(source: &str) -> Option<usize> {
        let normalizer = ComposingNormalizer::new_nfc();

        if normalizer.is_normalized(source) {
            return None;
        }

        // NFC is closed under substringing, so every prefix after the first non-NFC prefix is
        // also non-NFC. Search at character boundaries to identify the character that changes it.
        let mut normalized = 0;
        let mut not_normalized = source.len();

        loop {
            let middle = source.floor_char_boundary(normalized + (not_normalized - normalized) / 2);

            if middle == normalized {
                return Some(normalized);
            }

            if normalizer.is_normalized(&source[..middle]) {
                normalized = middle;
            } else {
                not_normalized = middle;
            }
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
        let mut display_escapes = self
            .display_escapes
            .as_deref()
            .unwrap_or_default()
            .iter()
            .peekable();

        for (index, ch) in self.source.char_indices() {
            let force_escape = display_escapes
                .peek()
                .is_some_and(|&&escape| escape == index);
            if force_escape {
                display_escapes.next();
            }
            Self::write_char(ch, self.layout.quote, force_escape, formatter)?;
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
    fn display_representations_are_normalized() {
        // Check that every scalar has an NFC display both after `e` and before `e` followed by a
        // combining acute accent, which must be escaped.
        let nfc = icu_normalizer::ComposingNormalizer::new_nfc();
        for codepoint in 0..=0x0010_ffff {
            let Some(ch) = char::from_u32(codepoint) else {
                continue;
            };
            for source in [format!("e{ch}"), format!("{ch}e\u{0301}")] {
                let escaped = UnicodeEscape::new_repr(&source).escape_for_display();
                let display = format!("{}", escaped.str_repr(TripleQuotes::No));
                assert!(nfc.is_normalized(&display), "{source:?} -> {display:?}");
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
