//! Vendored from [bytes.rs in rustpython-common](https://github.com/RustPython/RustPython/blob/1d8269fb729c91fc56064e975172d3a11bd62d07/common/src/bytes.rs).
//! The only changes we make are to remove dead code and make the default quote
//! type configurable.

use crate::vendor;
use crate::vendor::str::Quote;

pub fn repr(b: &[u8], quote: Quote) -> String {
    repr_with(b, &[], "", quote)
}

pub fn repr_with(b: &[u8], prefixes: &[&str], suffix: &str, quote: Quote) -> String {
    use std::fmt::Write;

    let mut out_len = 0usize;
    let mut squote = 0;
    let mut dquote = 0;

    for &ch in b {
        let incr = match ch {
            b'\'' => {
                squote += 1;
                1
            }
            b'"' => {
                dquote += 1;
                1
            }
            b'\\' | b'\t' | b'\r' | b'\n' => 2,
            0x20..=0x7e => 1,
            _ => 4, // \xHH
        };
        // TODO: OverflowError
        out_len = out_len.checked_add(incr).unwrap();
    }

    let (quote, num_escaped_quotes) = vendor::str::choose_quotes_for_repr(squote, dquote, quote);
    // we'll be adding backslashes in front of the existing inner quotes
    out_len += num_escaped_quotes;

    // 3 is for b prefix + outer quotes
    out_len += 3 + prefixes.iter().map(|s| s.len()).sum::<usize>() + suffix.len();

    let mut res = String::with_capacity(out_len);
    res.extend(prefixes.iter().copied());
    res.push('b');
    res.push(quote);
    for &ch in b {
        match ch {
            b'\t' => res.push_str("\\t"),
            b'\n' => res.push_str("\\n"),
            b'\r' => res.push_str("\\r"),
            // printable ascii range
            0x20..=0x7e => {
                let ch = ch as char;
                if ch == quote || ch == '\\' {
                    res.push('\\');
                }
                res.push(ch);
            }
            _ => write!(res, "\\x{ch:02x}").unwrap(),
        }
    }
    res.push(quote);
    res.push_str(suffix);

    res
}
