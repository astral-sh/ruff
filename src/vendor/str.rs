//! Vendored from [str.rs in rustpython-common](https://github.com/RustPython/RustPython/blob/1d8269fb729c91fc56064e975172d3a11bd62d07/common/src/str.rs).
//! The only changes we make are to remove dead code and make the default quote
//! type configurable.

use std::fmt;

use once_cell::unsync::OnceCell;

#[derive(Debug, Clone, Copy)]
pub enum Quote {
    Single,
    Double,
}

/// Get a Display-able type that formats to the python `repr()` of the string
/// value.
#[inline]
pub fn repr(s: &str, quote: Quote) -> Repr<'_> {
    Repr {
        s,
        quote,
        info: OnceCell::new(),
    }
}

#[derive(Debug, Copy, Clone)]
#[non_exhaustive]
pub struct ReprOverflowError;

impl fmt::Display for ReprOverflowError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_str("string is too long to generate repr")
    }
}

#[derive(Copy, Clone)]
struct ReprInfo {
    dquoted: bool,
    out_len: usize,
}

impl ReprInfo {
    fn get(s: &str, quote: Quote) -> Result<Self, ReprOverflowError> {
        let mut out_len = 0usize;
        let mut squote = 0;
        let mut dquote = 0;

        for ch in s.chars() {
            let incr = match ch {
                '\'' => {
                    squote += 1;
                    1
                }
                '"' => {
                    dquote += 1;
                    1
                }
                '\\' | '\t' | '\r' | '\n' => 2,
                ch if ch < ' ' || ch as u32 == 0x7f => 4, // \xHH
                ch if ch.is_ascii() => 1,
                ch if rustpython_common::char::is_printable(ch) => {
                    // max = std::cmp::max(ch, max);
                    ch.len_utf8()
                }
                ch if (ch as u32) < 0x100 => 4,   // \xHH
                ch if (ch as u32) < 0x10000 => 6, // \uHHHH
                _ => 10,                          // \uHHHHHHHH
            };
            out_len += incr;
            if out_len > std::isize::MAX as usize {
                return Err(ReprOverflowError);
            }
        }

        let (quote, num_escaped_quotes) = choose_quotes_for_repr(squote, dquote, quote);
        // we'll be adding backslashes in front of the existing inner quotes
        out_len += num_escaped_quotes;

        // start and ending quotes
        out_len += 2;

        let dquoted = quote == '"';

        Ok(ReprInfo { dquoted, out_len })
    }
}

pub struct Repr<'a> {
    s: &'a str,
    // the quote type we prefer to use
    quote: Quote,
    // the tuple is dquouted, out_len
    info: OnceCell<Result<ReprInfo, ReprOverflowError>>,
}

impl Repr<'_> {
    fn get_info(&self) -> Result<ReprInfo, ReprOverflowError> {
        *self.info.get_or_init(|| ReprInfo::get(self.s, self.quote))
    }

    fn _fmt<W: fmt::Write>(&self, repr: &mut W, info: ReprInfo) -> fmt::Result {
        let s = self.s;
        let in_len = s.len();
        let ReprInfo { dquoted, out_len } = info;

        let quote = if dquoted { '"' } else { '\'' };
        // if we don't need to escape anything we can just copy
        let unchanged = out_len == in_len;

        repr.write_char(quote)?;
        if unchanged {
            repr.write_str(s)?;
        } else {
            for ch in s.chars() {
                match ch {
                    '\n' => repr.write_str("\\n"),
                    '\t' => repr.write_str("\\t"),
                    '\r' => repr.write_str("\\r"),
                    // these 2 branches *would* be handled below, but we shouldn't have to do a
                    // unicodedata lookup just for ascii characters
                    '\x20'..='\x7e' => {
                        // printable ascii range
                        if ch == quote || ch == '\\' {
                            repr.write_char('\\')?;
                        }
                        repr.write_char(ch)
                    }
                    ch if ch.is_ascii() => {
                        write!(repr, "\\x{:02x}", ch as u8)
                    }
                    ch if rustpython_common::char::is_printable(ch) => repr.write_char(ch),
                    '\0'..='\u{ff}' => {
                        write!(repr, "\\x{:02x}", ch as u32)
                    }
                    '\0'..='\u{ffff}' => {
                        write!(repr, "\\u{:04x}", ch as u32)
                    }
                    _ => {
                        write!(repr, "\\U{:08x}", ch as u32)
                    }
                }?;
            }
        }
        repr.write_char(quote)
    }
}

impl fmt::Display for Repr<'_> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let info = self.get_info().unwrap();
        self._fmt(f, info)
    }
}

/// Returns the outer quotes to use and the number of quotes that need to be
/// escaped.
pub(crate) const fn choose_quotes_for_repr(
    num_squotes: usize,
    num_dquotes: usize,
    quote: Quote,
) -> (char, usize) {
    match quote {
        Quote::Single => {
            // always use squote unless we have squotes but no dquotes
            let use_dquote = num_squotes > 0 && num_dquotes == 0;
            if use_dquote {
                ('"', num_dquotes)
            } else {
                ('\'', num_squotes)
            }
        }
        Quote::Double => {
            // always use dquote unless we have dquotes but no squotes
            let use_squote = num_dquotes > 0 && num_squotes == 0;
            if use_squote {
                ('\'', num_squotes)
            } else {
                ('"', num_dquotes)
            }
        }
    }
}
