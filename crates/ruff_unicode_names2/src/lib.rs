//! Convert between characters and their standard names.
//!
//! This crate provides two functions for mapping from a `char` to the
//! name given by the Unicode standard (17.0). There are no runtime
//! requirements so this is usable with only `core` (this requires
//! specifying the `no_std` cargo feature). The tables are heavily
//! compressed, but still large (500KB), and still offer efficient
//! `O(1)` look-ups in both directions (more precisely, `O(length of
//! name)`).
//!
//! ```rust
//!     println!("☃ is called {:?}", unicode_names2::name('☃')); // SNOWMAN
//!     println!("{:?} is happy", unicode_names2::character("white smiling face")); // ☺
//!     // (NB. case insensitivity)
//! ```
//!
//! [**Source**](https://github.com/ProgVal/unicode_names2).
//!
//! # Macros
//!
//! The associated `unicode_names2_macros` crate provides two macros
//! for converting at compile-time, giving named literals similar to
//! Python's `"\N{...}"`.
//!
//! - `named_char!(name)` takes a single string `name` and creates a
//!   `char` literal.
//! - `named!(string)` takes a string and replaces any `\\N{name}`
//!   sequences with the character with that name. NB. String escape
//!   sequences cannot be customised, so the extra backslash (or a raw
//!   string) is required, unless you use a raw string.
//!
//! ```rust,ignore
//! #![feature(proc_macro_hygiene)]
//!
//! #[macro_use]
//! extern crate unicode_names2_macros;
//!
//! fn main() {
//!     let x: char = named_char!("snowman");
//!     assert_eq!(x, '☃');
//!
//!     let y: &str = named!("foo bar \\N{BLACK STAR} baz qux");
//!     assert_eq!(y, "foo bar ★ baz qux");
//!
//!     let y: &str = named!(r"foo bar \N{BLACK STAR} baz qux");
//!     assert_eq!(y, "foo bar ★ baz qux");
//! }
//! ```

#![cfg_attr(feature = "no_std", no_std)]
#![cfg_attr(test, feature(test))]
#![deny(missing_docs, unsafe_code)]

#[cfg(all(test, feature = "no_std"))]
#[macro_use]
extern crate std;

use core::{char, fmt};
use generated::{
    MAX_NAME_LENGTH, PHRASEBOOK_OFFSETS1, PHRASEBOOK_OFFSETS2, PHRASEBOOK_OFFSET_SHIFT,
};

#[allow(dead_code)]
#[rustfmt::skip]
#[allow(clippy::all)]
mod generated {
    include!(concat!(env!("OUT_DIR"), "/generated.rs"));
}
#[allow(dead_code)]
#[rustfmt::skip]
#[allow(clippy::all)]
mod generated_phf {
    include!(concat!(env!("OUT_DIR"), "/generated_phf.rs"));
}
#[allow(dead_code)]
mod jamo;

/// A map of unicode aliases to their corresponding values.
/// Generated in generator
#[allow(dead_code)]
static ALIASES: phf::Map<&'static [u8], char> =
    include!(concat!(env!("OUT_DIR"), "/generated_alias.rs"));

mod iter_str;

static HANGUL_SYLLABLE_PREFIX: &str = "HANGUL SYLLABLE ";
static CJK_UNIFIED_IDEOGRAPH_PREFIX: &str = "CJK UNIFIED IDEOGRAPH-";

fn is_cjk_unified_ideograph(ch: char) -> bool {
    generated::CJK_IDEOGRAPH_RANGES
        .iter()
        .any(|&(lo, hi)| lo <= ch && ch <= hi)
}

/// An iterator over the components of a code point's name, it also
/// implements `Show`.
///
/// The size hint is exact for the number of pieces, but iterates
/// (although iteration is cheap and all names are short).
#[derive(Clone)]
pub struct Name {
    data: Name_,
}
#[allow(clippy::upper_case_acronyms)]
#[derive(Clone)]
enum Name_ {
    Plain(iter_str::IterStr),
    CJK(CJK),
    Hangul(Hangul),
}

#[allow(clippy::upper_case_acronyms)]
#[derive(Copy)]
struct CJK {
    emit_prefix: bool,
    idx: u8,
    // the longest character is 0x10FFFF
    data: [u8; 6],
}
#[derive(Copy)]
struct Hangul {
    emit_prefix: bool,
    idx: u8,
    // stores the choseong, jungseong, jongseong syllable numbers (in
    // that order)
    data: [u8; 3],
}
impl Clone for CJK {
    fn clone(&self) -> CJK {
        *self
    }
}
impl Clone for Hangul {
    fn clone(&self) -> Hangul {
        *self
    }
}

#[allow(clippy::len_without_is_empty)]
impl Name {
    /// The number of bytes in the name.
    ///
    /// All names are plain ASCII, so this is also the number of
    /// Unicode codepoints and the number of graphemes.
    pub fn len(&self) -> usize {
        let counted = self.clone();
        counted.fold(0, |a, s| a + s.len())
    }
}

impl Iterator for Name {
    type Item = &'static str;
    fn next(&mut self) -> Option<&'static str> {
        match self.data {
            Name_::Plain(ref mut s) => s.next(),
            Name_::CJK(ref mut state) => {
                // we're a CJK unified ideograph
                if state.emit_prefix {
                    state.emit_prefix = false;
                    return Some(CJK_UNIFIED_IDEOGRAPH_PREFIX);
                }
                // run until we've run out of array: the construction
                // of the data means this is exactly when we have
                // finished emitting the number.
                state
                    .data
                    .get(state.idx as usize)
                    // (avoid conflicting mutable borrow problems)
                    .map(|digit| *digit as usize)
                    .map(|d| {
                        state.idx += 1;
                        static DIGITS: &str = "0123456789ABCDEF";
                        &DIGITS[d..d + 1]
                    })
            }
            Name_::Hangul(ref mut state) => {
                if state.emit_prefix {
                    state.emit_prefix = false;
                    return Some(HANGUL_SYLLABLE_PREFIX);
                }

                let idx = state.idx as usize;
                state.data.get(idx).map(|x| *x as usize).map(|x| {
                    // progressively walk through the syllables
                    state.idx += 1;
                    [jamo::CHOSEONG, jamo::JUNGSEONG, jamo::JONGSEONG][idx][x]
                })
            }
        }
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        // we can estimate exactly by just iterating and summing up.
        let counted = self.clone();
        let n = counted.count();
        (n, Some(n))
    }
}

impl fmt::Debug for Name {
    fn fmt(&self, fmtr: &mut fmt::Formatter) -> fmt::Result {
        fmt::Display::fmt(self, fmtr)
    }
}
impl fmt::Display for Name {
    fn fmt(&self, fmtr: &mut fmt::Formatter) -> fmt::Result {
        self.clone().try_for_each(|s| fmtr.write_str(s))
    }
}

/// Find the name of `c`, or `None` if `c` has no name.
///
/// The return value is an iterator that yields `&str` components of
/// the name successively (including spaces and hyphens). It
/// implements `Show`, and thus can be used naturally to build
/// `String`s, or be printed, etc.
///
/// # Example
///
/// ```rust
/// assert_eq!(unicode_names2::name('a').map(|n| n.to_string()),
///            Some("LATIN SMALL LETTER A".to_string()));
/// assert_eq!(unicode_names2::name('\u{2605}').map(|n| n.to_string()),
///            Some("BLACK STAR".to_string()));
/// assert_eq!(unicode_names2::name('☃').map(|n| n.to_string()),
///            Some("SNOWMAN".to_string()));
///
/// // control code
/// assert!(unicode_names2::name('\x00').is_none());
/// // unassigned
/// assert!(unicode_names2::name('\u{10FFFF}').is_none());
/// ```
pub fn name(c: char) -> Option<Name> {
    let cc = c as usize;
    let offset =
        (PHRASEBOOK_OFFSETS1[cc >> PHRASEBOOK_OFFSET_SHIFT] as usize) << PHRASEBOOK_OFFSET_SHIFT;

    let mask = (1 << PHRASEBOOK_OFFSET_SHIFT) - 1;
    let offset = PHRASEBOOK_OFFSETS2[offset + (cc & mask)];
    if offset == 0 {
        if is_cjk_unified_ideograph(c) {
            // write the hex number out right aligned in this array.
            let mut data = [b'0'; 6];
            let mut number = c as u32;
            let mut data_start = 6;
            for place in data.iter_mut().rev() {
                // this would be incorrect if U+0000 was CJK unified
                // ideograph, but it's not, so it's fine.
                if number == 0 {
                    break;
                }
                *place = (number % 16) as u8;
                number /= 16;
                data_start -= 1;
            }
            Some(Name {
                data: Name_::CJK(CJK {
                    emit_prefix: true,
                    idx: data_start,
                    data,
                }),
            })
        } else {
            // maybe it is a hangul syllable?
            jamo::syllable_decomposition(c).map(|(ch, ju, jo)| Name {
                data: Name_::Hangul(Hangul {
                    emit_prefix: true,
                    idx: 0,
                    data: [ch, ju, jo],
                }),
            })
        }
    } else {
        Some(Name {
            data: Name_::Plain(iter_str::IterStr::new(offset)),
        })
    }
}

fn fnv_hash<I: Iterator<Item = u8>>(x: I) -> u64 {
    let mut g = 0xcbf29ce484222325 ^ generated_phf::NAME2CODE_N;
    for b in x {
        g ^= b as u64;
        g = g.wrapping_mul(0x100000001b3);
    }
    g
}
fn displace(f1: u32, f2: u32, d1: u32, d2: u32) -> u32 {
    d2.wrapping_add(f1.wrapping_mul(d1)).wrapping_add(f2)
}
fn split(hash: u64) -> (u32, u32, u32) {
    let bits = 21;
    let mask = (1 << bits) - 1;
    (
        (hash & mask) as u32,
        ((hash >> bits) & mask) as u32,
        ((hash >> (2 * bits)) & mask) as u32,
    )
}

/// Get alias value from alias name, returns `None` if the alias is not found.
fn character_by_alias(name: &[u8]) -> Option<char> {
    ALIASES.get(name).copied()
}

/// Find the character called `name`, or `None` if no such character
/// exists.
///
/// This searches case-insensitively.
///
/// # Example
///
/// ```rust
/// assert_eq!(unicode_names2::character("LATIN SMALL LETTER A"), Some('a'));
/// assert_eq!(unicode_names2::character("latin SMALL letter A"), Some('a'));
/// assert_eq!(unicode_names2::character("latin small letter a"), Some('a'));
/// assert_eq!(unicode_names2::character("BLACK STAR"), Some('★'));
/// assert_eq!(unicode_names2::character("SNOWMAN"), Some('☃'));
/// assert_eq!(unicode_names2::character("BACKSPACE"), Some('\x08'));
///
/// assert_eq!(unicode_names2::character("nonsense"), None);
/// ```
pub fn character(search_name: &str) -> Option<char> {
    // + 1 so that we properly handle the case when `name` has a
    // prefix of the longest name, but isn't exactly equal.
    let mut buf = [0; MAX_NAME_LENGTH + 1];
    for (place, byte) in buf.iter_mut().zip(search_name.bytes()) {
        *place = byte.to_ascii_uppercase();
    }
    let search_name = buf.get(..search_name.len())?;

    // try `HANGUL SYLLABLE <choseong><jungseong><jongseong>`
    if search_name.starts_with(HANGUL_SYLLABLE_PREFIX.as_bytes()) {
        let remaining = &search_name[HANGUL_SYLLABLE_PREFIX.len()..];
        let (choseong, remaining) = jamo::slice_shift_choseong(remaining);
        let (jungseong, remaining) = jamo::slice_shift_jungseong(remaining);
        let (jongseong, remaining) = jamo::slice_shift_jongseong(remaining);
        match (choseong, jungseong, jongseong, remaining) {
            (Some(choseong), Some(jungseong), Some(jongseong), b"") => {
                let c = 0xac00 + (choseong * 21 + jungseong) * 28 + jongseong;
                return char::from_u32(c);
            }
            (_, _, _, _) => {
                // there are no other names starting with `HANGUL SYLLABLE `
                // (verified by `generator/...`).
                return None;
            }
        }
    }

    // try `CJK UNIFIED IDEOGRAPH-<digits>`
    if search_name.starts_with(CJK_UNIFIED_IDEOGRAPH_PREFIX.as_bytes()) {
        let remaining = &search_name[CJK_UNIFIED_IDEOGRAPH_PREFIX.len()..];
        if remaining.len() > 5 {
            return None;
        } // avoid overflow

        let mut v = 0u32;
        for &c in remaining {
            v = match c {
                b'0'..=b'9' => (v << 4) | (c - b'0') as u32,
                b'A'..=b'F' => (v << 4) | (c - b'A' + 10) as u32,
                _ => return None,
            }
        }
        let ch = match char::from_u32(v) {
            Some(ch) => ch,
            None => return None,
        };

        // check if the resulting code is indeed in the known ranges
        if is_cjk_unified_ideograph(ch) {
            return Some(ch);
        } else {
            // there are no other names starting with `CJK UNIFIED IDEOGRAPH-`
            // (verified by `src/generate.py`).
            return None;
        }
    }

    // get the parts of the hash...
    let (g, f1, f2) = split(fnv_hash(search_name.iter().copied()));
    // ...and the appropriate displacements...
    let (d1, d2) = generated_phf::NAME2CODE_DISP[g as usize % generated_phf::NAME2CODE_DISP.len()];

    // ...to find the right index...
    let idx = displace(f1, f2, d1 as u32, d2 as u32) as usize;
    // ...for looking up the codepoint.
    let codepoint = generated_phf::NAME2CODE_CODE[idx % generated_phf::NAME2CODE_CODE.len()];

    // Now check that this is actually correct. Since this is a
    // perfect hash table, valid names map precisely to their code
    // point (and invalid names map to anything), so we only need to
    // check the name for this codepoint matches the input and we know
    // everything. (i.e. no need for probing)
    let maybe_name = match name(codepoint) {
        None => {
            if true {
                debug_assert!(false)
            }
            return character_by_alias(search_name);
        }
        Some(name) => name,
    };

    // run through the parts of the name, matching them against the
    // parts of the input.
    let mut passed_name = search_name;
    for part in maybe_name {
        let part = part.as_bytes();
        let part_l = part.len();
        if passed_name.len() < part_l || &passed_name[..part_l] != part {
            return character_by_alias(search_name);
        }
        passed_name = &passed_name[part_l..]
    }

    Some(codepoint)
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand::{
        distributions::{Distribution, Standard},
        prelude::{SeedableRng, StdRng},
    };
    use std::char;
    use std::prelude::v1::*;

    extern crate test;

    use test::bench::Bencher;

    static DATA: &str = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/data/UnicodeData.txt"));

    #[test]
    fn exhaustive() {
        // check that gaps have no names (these are unassigned/control
        // codes).
        fn negative_range(from: u32, to: u32) {
            for c in (from..to).filter_map(char::from_u32) {
                if !is_cjk_unified_ideograph(c) && !jamo::is_hangul_syllable(c) {
                    let n = name(c);
                    assert!(
                        n.is_none(),
                        "{} ({}) shouldn't have a name but is called {}",
                        c,
                        c as u32,
                        n.unwrap()
                    );
                }
            }
        }

        let mut last = 0;
        for line in DATA.lines() {
            let mut it = line.split(';');

            let raw_c = it.next();
            let c = match char::from_u32(
                raw_c.and_then(|s| u32::from_str_radix(s, 16).ok()).unwrap(),
            ) {
                Some(c) => c,
                None => continue,
            };

            let n = it.next().unwrap();
            if n.starts_with("<") {
                continue;
            }

            let computed_n = name(c).unwrap();
            let n_str = computed_n.to_string();
            assert_eq!(n_str, n.to_string());
            assert_eq!(computed_n.len(), n_str.len());

            let (hint_low, hint_high) = computed_n.size_hint();
            let number_of_parts = computed_n.count();
            assert_eq!(hint_low, number_of_parts);
            assert_eq!(hint_high, Some(number_of_parts));

            assert_eq!(character(n), Some(c));
            assert_eq!(character(&n.to_ascii_lowercase()), Some(c));

            negative_range(last, c as u32);
            last = c as u32 + 1;
        }
        negative_range(last, 0x10FFFF + 1)
    }

    #[test]
    fn name_to_string() {
        let n = name('a').unwrap();
        assert_eq!(n.to_string(), "LATIN SMALL LETTER A".to_string());
        let n = name('🁣').unwrap();
        assert_eq!(n.to_string(), "DOMINO TILE VERTICAL-00-00".to_string());
    }

    #[test]
    fn character_negative() {
        let long_name = "x".repeat(100);
        assert!(long_name.len() > MAX_NAME_LENGTH); // Otherwise this test is pointless
        let names = ["", "x", "öäå", "SPAACE", &long_name];
        for &n in names.iter() {
            assert_eq!(character(n), None);
        }
    }

    #[test]
    fn name_hangul_syllable() {
        assert_eq!(
            name('\u{ac00}').map(|s| s.to_string()),
            Some("HANGUL SYLLABLE GA".to_string())
        ); // first
        assert_eq!(
            name('\u{bdc1}').map(|s| s.to_string()),
            Some("HANGUL SYLLABLE BWELG".to_string())
        );
        assert_eq!(
            name('\u{d7a3}').map(|s| s.to_string()),
            Some("HANGUL SYLLABLE HIH".to_string())
        ); // last
    }

    #[test]
    fn character_hangul_syllable() {
        assert_eq!(character("HANGUL SYLLABLE GA"), Some('\u{ac00}'));
        assert_eq!(character("HANGUL SYLLABLE BWELG"), Some('\u{bdc1}'));
        assert_eq!(character("HANGUL SYLLABLE HIH"), Some('\u{d7a3}'));
        assert_eq!(character("HANGUL SYLLABLE BLAH"), None);
    }

    #[test]
    fn cjk_unified_ideograph_exhaustive() {
        for &(lo, hi) in generated::CJK_IDEOGRAPH_RANGES.iter() {
            for x in lo as u32..=hi as u32 {
                let c = char::from_u32(x).unwrap();

                let real_name = format!("CJK UNIFIED IDEOGRAPH-{:X}", x);
                let lower_real_name = format!("CJK UNIFIED IDEOGRAPH-{:x}", x);
                assert_eq!(character(&real_name), Some(c));
                assert_eq!(character(&lower_real_name), Some(c));

                assert_eq!(name(c).map(|s| s.to_string()), Some(real_name));
            }
        }
    }
    #[test]
    fn name_cjk_unified_ideograph() {
        assert_eq!(
            name('\u{4e00}').map(|s| s.to_string()),
            Some("CJK UNIFIED IDEOGRAPH-4E00".to_string())
        ); // first in BMP
        assert_eq!(
            name('\u{9fcc}').map(|s| s.to_string()),
            Some("CJK UNIFIED IDEOGRAPH-9FCC".to_string())
        ); // last in BMP (as of 6.1)
        assert_eq!(
            name('\u{20000}').map(|s| s.to_string()),
            Some("CJK UNIFIED IDEOGRAPH-20000".to_string())
        ); // first in SIP
        assert_eq!(
            name('\u{2a6d6}').map(|s| s.to_string()),
            Some("CJK UNIFIED IDEOGRAPH-2A6D6".to_string())
        );
        assert_eq!(
            name('\u{2a700}').map(|s| s.to_string()),
            Some("CJK UNIFIED IDEOGRAPH-2A700".to_string())
        );
        assert_eq!(
            name('\u{2b81d}').map(|s| s.to_string()),
            Some("CJK UNIFIED IDEOGRAPH-2B81D".to_string())
        ); // last in SIP (as of 6.0)
    }

    #[test]
    fn character_cjk_unified_ideograph() {
        assert_eq!(character("CJK UNIFIED IDEOGRAPH-4E00"), Some('\u{4e00}'));
        assert_eq!(character("CJK UNIFIED IDEOGRAPH-9FCC"), Some('\u{9fcc}'));
        assert_eq!(character("CJK UNIFIED IDEOGRAPH-20000"), Some('\u{20000}'));
        assert_eq!(character("CJK UNIFIED IDEOGRAPH-2A6D6"), Some('\u{2a6d6}'));
        assert_eq!(character("CJK UNIFIED IDEOGRAPH-2A700"), Some('\u{2a700}'));
        assert_eq!(character("CJK UNIFIED IDEOGRAPH-2B81D"), Some('\u{2b81d}'));
        assert_eq!(character("CJK UNIFIED IDEOGRAPH-"), None);
        assert_eq!(character("CJK UNIFIED IDEOGRAPH-!@#$"), None);
        assert_eq!(character("CJK UNIFIED IDEOGRAPH-1234"), None);
        assert_eq!(character("CJK UNIFIED IDEOGRAPH-EFGH"), None);
        assert_eq!(character("CJK UNIFIED IDEOGRAPH-12345"), None);
        assert_eq!(character("CJK UNIFIED IDEOGRAPH-2A6FF"), None); // between Ext B and Ext C
        assert_eq!(character("CJK UNIFIED IDEOGRAPH-2A6FF"), None);
    }

    #[test]
    fn character_by_alias() {
        assert_eq!(super::character_by_alias(b"NEW LINE"), Some('\n'));
        assert_eq!(super::character_by_alias(b"BACKSPACE"), Some('\u{8}'));
        assert_eq!(super::character_by_alias(b"NOT AN ALIAS"), None);
    }

    #[bench]
    fn name_basic(b: &mut Bencher) {
        b.iter(|| {
            for s in name('ö').unwrap() {
                test::black_box(s);
            }
        })
    }

    #[bench]
    fn character_basic(b: &mut Bencher) {
        b.iter(|| character("LATIN SMALL LETTER O WITH DIAERESIS"));
    }

    #[bench]
    fn name_10000_invalid(b: &mut Bencher) {
        // be consistent across runs, but avoid sequential/caching.
        let mut rng = StdRng::seed_from_u64(0x12345678);
        let chars: Vec<char> = Standard
            .sample_iter(&mut rng)
            .take(10000)
            .filter_map(|c| match c {
                c if name(c).is_none() => Some(c),
                _ => None,
            })
            .collect();

        b.iter(|| {
            for &c in chars.iter() {
                assert!(name(c).is_none());
            }
        })
    }

    #[bench]
    fn name_all_valid(b: &mut Bencher) {
        let chars = (0u32..0x10FFFF)
            .filter_map(|x| match char::from_u32(x) {
                Some(c) if name(c).is_some() => Some(c),
                _ => None,
            })
            .collect::<Vec<char>>();

        b.iter(|| {
            for c in chars.iter() {
                for s in name(*c).unwrap() {
                    test::black_box(s);
                }
            }
        });
    }

    #[bench]
    fn character_10000(b: &mut Bencher) {
        // be consistent across runs, but avoid sequential/caching.
        let mut rng = StdRng::seed_from_u64(0x12345678);

        let names: Vec<_> = Standard
            .sample_iter(&mut rng)
            .take(10000)
            .filter_map(name)
            .map(|name| name.to_string())
            .collect();

        b.iter(|| {
            for n in names.iter() {
                test::black_box(character(n));
            }
        })
    }
}

#[cfg(all(feature = "no_std", not(test)))]
mod std {
    pub use core::{clone, fmt, marker};
}
