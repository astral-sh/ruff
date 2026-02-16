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
}

impl<'a> UnicodeEscape<'a> {
    #[inline]
    pub fn with_preferred_quote(source: &'a str, quote: Quote) -> Self {
        let layout = Self::repr_layout(source, quote);
        Self { source, layout }
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
            ch if (ch as u32) < 0x100 => 4,   // \xHH
            ch if (ch as u32) < 0x10000 => 6, // \uHHHH
            _ => 10,                          // \uHHHHHHHH
        }
    }

    fn write_char(
        ch: char,
        quote: Quote,
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
            ch if crate::char::is_printable(ch) => formatter.write_char(ch),
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
        for ch in self.source.chars() {
            Self::write_char(ch, self.layout().quote, formatter)?;
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
    use ruff_python_ast::str::TripleQuotes;

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
    fn test_choose_quote_prefers_primary() {
        // No quotes in string => use preferred
        assert_eq!(choose_quote(0, 0, Quote::Single), (Quote::Single, 0));
        assert_eq!(choose_quote(0, 0, Quote::Double), (Quote::Double, 0));
    }

    #[test]
    fn test_choose_quote_switches_when_only_primary() {
        // Only single quotes, preferred single => switch to double
        assert_eq!(choose_quote(3, 0, Quote::Single), (Quote::Double, 0));
        // Only double quotes, preferred double => switch to single
        assert_eq!(choose_quote(0, 3, Quote::Double), (Quote::Single, 0));
    }

    #[test]
    fn test_choose_quote_keeps_primary_when_both() {
        // Both quote types present => keep preferred, escape primary count
        assert_eq!(choose_quote(2, 3, Quote::Single), (Quote::Single, 2));
        assert_eq!(choose_quote(2, 3, Quote::Double), (Quote::Double, 3));
    }

    #[test]
    fn test_unicode_escape_empty_string() {
        let escape = UnicodeEscape::new_repr("");
        assert!(!escape.changed());
        assert_eq!(escape.layout().len, Some(0));
    }

    #[test]
    fn test_unicode_escape_ascii_printable() {
        let escape = UnicodeEscape::new_repr("hello world");
        assert!(!escape.changed());
        assert_eq!(escape.layout().len, Some(11));
    }

    #[test]
    fn test_unicode_escape_with_newline() {
        let escape = UnicodeEscape::new_repr("line1\nline2");
        assert!(escape.changed());
        // "line1" (5) + "\\n" (2) + "line2" (5) = 12
        assert_eq!(escape.layout().len, Some(12));
    }

    #[test]
    fn test_unicode_escape_with_tab() {
        let escape = UnicodeEscape::new_repr("a\tb");
        assert!(escape.changed());
        // "a" (1) + "\\t" (2) + "b" (1) = 4
        assert_eq!(escape.layout().len, Some(4));
    }

    #[test]
    fn test_unicode_escape_with_backslash() {
        let escape = UnicodeEscape::new_repr("a\\b");
        assert!(escape.changed());
        // "a" (1) + "\\\\" (2) + "b" (1) = 4
        assert_eq!(escape.layout().len, Some(4));
    }

    #[test]
    fn test_unicode_escape_control_characters() {
        // Control char (< 0x20) gets \xHH encoding (4 chars)
        let escape = UnicodeEscape::new_repr("\x01");
        assert!(escape.changed());
        assert_eq!(escape.layout().len, Some(4));
    }

    #[test]
    fn test_unicode_escape_write_body() {
        let escape = UnicodeEscape::new_repr("hello\nworld");
        let mut output = String::new();
        escape.write_body(&mut output).unwrap();
        assert_eq!(output, "hello\\nworld");
    }

    #[test]
    fn test_unicode_str_repr_display() {
        let escape = UnicodeEscape::new_repr("hello");
        let repr = escape.str_repr(TripleQuotes::No);
        assert_eq!(repr.to_string(), Some("'hello'".to_string()));
    }

    #[test]
    fn test_unicode_str_repr_with_escapes() {
        let escape = UnicodeEscape::new_repr("line1\nline2");
        let repr = escape.str_repr(TripleQuotes::No);
        assert_eq!(repr.to_string(), Some("'line1\\nline2'".to_string()));
    }

    #[test]
    fn test_unicode_escape_preferred_double_quote() {
        let escape = UnicodeEscape::with_preferred_quote("hello", Quote::Double);
        assert_eq!(escape.layout().quote, Quote::Double);
    }

    #[test]
    fn test_unicode_escape_quote_selection_with_singles() {
        // String with only single quotes, preferred single => switches to double
        let escape = UnicodeEscape::with_preferred_quote("it's", Quote::Single);
        assert_eq!(escape.layout().quote, Quote::Double);
    }
}

#[cfg(test)]
mod ascii_escape_tests {
    use super::*;
    use ruff_python_ast::str::TripleQuotes;

    #[test]
    fn test_ascii_escape_empty() {
        let escape = AsciiEscape::new_repr(b"");
        assert!(!escape.changed());
        assert_eq!(escape.layout().len, Some(0));
    }

    #[test]
    fn test_ascii_escape_printable() {
        let escape = AsciiEscape::new_repr(b"hello");
        assert!(!escape.changed());
        assert_eq!(escape.layout().len, Some(5));
    }

    #[test]
    fn test_ascii_escape_with_newline() {
        let escape = AsciiEscape::new_repr(b"a\nb");
        assert!(escape.changed());
        // "a" (1) + "\\n" (2) + "b" (1) = 4
        assert_eq!(escape.layout().len, Some(4));
    }

    #[test]
    fn test_ascii_escape_non_printable_byte() {
        let escape = AsciiEscape::new_repr(&[0x80]);
        assert!(escape.changed());
        // \xHH = 4 chars
        assert_eq!(escape.layout().len, Some(4));
    }

    #[test]
    fn test_ascii_escape_write_body() {
        let escape = AsciiEscape::new_repr(b"hello\tworld");
        let mut output = String::new();
        escape.write_body(&mut output).unwrap();
        assert_eq!(output, "hello\\tworld");
    }

    #[test]
    fn test_bytes_repr_display() {
        let escape = AsciiEscape::new_repr(b"hello");
        let repr = escape.bytes_repr(TripleQuotes::No);
        assert_eq!(repr.to_string(), Some("b'hello'".to_string()));
    }

    #[test]
    fn test_bytes_repr_with_escapes() {
        let escape = AsciiEscape::new_repr(b"\x00\xff");
        let repr = escape.bytes_repr(TripleQuotes::No);
        assert_eq!(repr.to_string(), Some("b'\\x00\\xff'".to_string()));
    }

    #[test]
    fn test_ascii_escape_quote_selection() {
        // Only single quotes => switch to double
        let escape = AsciiEscape::with_preferred_quote(b"it's", Quote::Single);
        assert_eq!(escape.layout().quote, Quote::Double);
    }

    #[test]
    fn test_ascii_escape_backslash() {
        let escape = AsciiEscape::new_repr(b"a\\b");
        assert!(escape.changed());
        assert_eq!(escape.layout().len, Some(4));
    }

    #[test]
    fn test_ascii_escape_carriage_return() {
        let escape = AsciiEscape::new_repr(b"a\rb");
        assert!(escape.changed());
        assert_eq!(escape.layout().len, Some(4));
    }
}
