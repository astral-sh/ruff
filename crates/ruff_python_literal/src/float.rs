use crate::Case;
use num_traits::{Float, Zero};
use std::f64;

pub fn parse_str(literal: &str) -> Option<f64> {
    parse_inner(literal.trim().as_bytes())
}

pub fn parse_bytes(literal: &[u8]) -> Option<f64> {
    parse_inner(trim_slice(literal, u8::is_ascii_whitespace))
}

fn trim_slice<T>(v: &[T], mut trim: impl FnMut(&T) -> bool) -> &[T] {
    let mut it = v.iter();
    // it.take_while_ref(&mut trim).for_each(drop);
    // hmm.. `&mut slice::Iter<_>` is not `Clone`
    // it.by_ref().rev().take_while_ref(&mut trim).for_each(drop);
    while it.clone().next().is_some_and(&mut trim) {
        it.next();
    }
    while it.clone().next_back().is_some_and(&mut trim) {
        it.next_back();
    }
    it.as_slice()
}

fn parse_inner(literal: &[u8]) -> Option<f64> {
    use lexical_parse_float::{
        format::PYTHON3_LITERAL, FromLexicalWithOptions, NumberFormatBuilder, Options,
    };
    // lexical-core's format::PYTHON_STRING is inaccurate
    const PYTHON_STRING: u128 = NumberFormatBuilder::rebuild(PYTHON3_LITERAL)
        .no_special(false)
        .build();
    f64::from_lexical_with_options::<PYTHON_STRING>(literal, &Options::new()).ok()
}

pub fn is_integer(v: f64) -> bool {
    (v - v.round()).abs() < f64::EPSILON
}

fn format_nan(case: Case) -> String {
    let nan = match case {
        Case::Lower => "nan",
        Case::Upper => "NAN",
    };

    nan.to_string()
}

fn format_inf(case: Case) -> String {
    let inf = match case {
        Case::Lower => "inf",
        Case::Upper => "INF",
    };

    inf.to_string()
}

pub fn decimal_point_or_empty(precision: usize, alternate_form: bool) -> &'static str {
    match (precision, alternate_form) {
        (0, true) => ".",
        _ => "",
    }
}

pub fn format_fixed(precision: usize, magnitude: f64, case: Case, alternate_form: bool) -> String {
    match magnitude {
        magnitude if magnitude.is_finite() => {
            let point = decimal_point_or_empty(precision, alternate_form);
            format!("{magnitude:.precision$}{point}")
        }
        magnitude if magnitude.is_nan() => format_nan(case),
        magnitude if magnitude.is_infinite() => format_inf(case),
        _ => String::new(),
    }
}

// Formats floats into Python style exponent notation, by first formatting in Rust style
// exponent notation (`1.0000e0`), then convert to Python style (`1.0000e+00`).
pub fn format_exponent(
    precision: usize,
    magnitude: f64,
    case: Case,
    alternate_form: bool,
) -> String {
    match magnitude {
        magnitude if magnitude.is_finite() => {
            let r_exp = format!("{magnitude:.precision$e}");
            let mut parts = r_exp.splitn(2, 'e');
            let base = parts.next().unwrap();
            let exponent = parts.next().unwrap().parse::<i64>().unwrap();
            let e = match case {
                Case::Lower => 'e',
                Case::Upper => 'E',
            };
            let point = decimal_point_or_empty(precision, alternate_form);
            format!("{base}{point}{e}{exponent:+#03}")
        }
        magnitude if magnitude.is_nan() => format_nan(case),
        magnitude if magnitude.is_infinite() => format_inf(case),
        _ => String::new(),
    }
}

/// If s represents a floating point value, trailing zeros and a possibly trailing
/// decimal point will be removed.
/// This function does NOT work with decimal commas.
fn maybe_remove_trailing_redundant_chars(s: String, alternate_form: bool) -> String {
    if !alternate_form && s.contains('.') {
        // only truncate floating point values when not in alternate form
        let s = remove_trailing_zeros(s);
        remove_trailing_decimal_point(s)
    } else {
        s
    }
}

fn remove_trailing_zeros(s: String) -> String {
    let mut s = s;
    while s.ends_with('0') {
        s.pop();
    }
    s
}

fn remove_trailing_decimal_point(s: String) -> String {
    let mut s = s;
    if s.ends_with('.') {
        s.pop();
    }
    s
}

#[allow(
    clippy::cast_sign_loss,
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap
)]
pub fn format_general(
    precision: usize,
    magnitude: f64,
    case: Case,
    alternate_form: bool,
    always_shows_fract: bool,
) -> String {
    match magnitude {
        magnitude if magnitude.is_finite() => {
            let r_exp = format!("{:.*e}", precision.saturating_sub(1), magnitude);
            let mut parts = r_exp.splitn(2, 'e');
            let base = parts.next().unwrap();
            let exponent = parts.next().unwrap().parse::<i64>().unwrap();
            if exponent < -4 || exponent + i64::from(always_shows_fract) >= (precision as i64) {
                let e = match case {
                    Case::Lower => 'e',
                    Case::Upper => 'E',
                };
                let magnitude = format!("{:.*}", precision + 1, base);
                let base = maybe_remove_trailing_redundant_chars(magnitude, alternate_form);
                let point = decimal_point_or_empty(precision.saturating_sub(1), alternate_form);
                format!("{base}{point}{e}{exponent:+#03}")
            } else {
                let precision = ((precision as i64) - 1 - exponent) as usize;
                let magnitude = format!("{magnitude:.precision$}");
                let base = maybe_remove_trailing_redundant_chars(magnitude, alternate_form);
                let point = decimal_point_or_empty(precision, alternate_form);
                format!("{base}{point}")
            }
        }
        magnitude if magnitude.is_nan() => format_nan(case),
        magnitude if magnitude.is_infinite() => format_inf(case),
        _ => String::new(),
    }
}

// TODO: rewrite using format_general
pub fn to_string(value: f64) -> String {
    let lit = format!("{value:e}");
    if let Some(position) = lit.find('e') {
        let significand = &lit[..position];
        let exponent = &lit[position + 1..];
        let exponent = exponent.parse::<i32>().unwrap();
        if exponent < 16 && exponent > -5 {
            if is_integer(value) {
                format!("{value:.1?}")
            } else {
                value.to_string()
            }
        } else {
            format!("{significand}e{exponent:+#03}")
        }
    } else {
        let mut s = value.to_string();
        s.make_ascii_lowercase();
        s
    }
}

pub fn from_hex(s: &str) -> Option<f64> {
    if let Ok(f) = hexf_parse::parse_hexf64(s, false) {
        return Some(f);
    }
    match s.to_ascii_lowercase().as_str() {
        "nan" | "+nan" | "-nan" => Some(f64::NAN),
        "inf" | "infinity" | "+inf" | "+infinity" => Some(f64::INFINITY),
        "-inf" | "-infinity" => Some(f64::NEG_INFINITY),
        value => {
            let mut hex = String::with_capacity(value.len());
            let has_0x = value.contains("0x");
            let has_p = value.contains('p');
            let has_dot = value.contains('.');
            let mut start = 0;

            if !has_0x && value.starts_with('-') {
                hex.push_str("-0x");
                start += 1;
            } else if !has_0x {
                hex.push_str("0x");
                if value.starts_with('+') {
                    start += 1;
                }
            }

            for (index, ch) in value.chars().enumerate() {
                if ch == 'p' {
                    if has_dot {
                        hex.push('p');
                    } else {
                        hex.push_str(".p");
                    }
                } else if index >= start {
                    hex.push(ch);
                }
            }

            if !has_p && has_dot {
                hex.push_str("p0");
            } else if !has_p && !has_dot {
                hex.push_str(".p0");
            }

            hexf_parse::parse_hexf64(hex.as_str(), false).ok()
        }
    }
}

pub fn to_hex(value: f64) -> String {
    let (mantissa, exponent, sign) = value.integer_decode();
    let sign_fmt = if sign < 0 { "-" } else { "" };
    match value {
        value if value.is_zero() => format!("{sign_fmt}0x0.0p+0"),
        value if value.is_infinite() => format!("{sign_fmt}inf"),
        value if value.is_nan() => "nan".to_owned(),
        _ => {
            const BITS: i16 = 52;
            const FRACT_MASK: u64 = 0xf_ffff_ffff_ffff;
            format!(
                "{}{:#x}.{:013x}p{:+}",
                sign_fmt,
                mantissa >> BITS,
                mantissa & FRACT_MASK,
                exponent + BITS
            )
        }
    }
}

#[test]
#[allow(clippy::float_cmp)]
fn test_to_hex() {
    use rand::Rng;
    for _ in 0..20000 {
        let bytes = rand::thread_rng().gen::<[u64; 1]>();
        let f = f64::from_bits(bytes[0]);
        if !f.is_finite() {
            continue;
        }
        let hex = to_hex(f);
        // println!("{} -> {}", f, hex);
        let roundtrip = hexf_parse::parse_hexf64(&hex, false).unwrap();
        // println!("  -> {}", roundtrip);

        assert_eq!(f, roundtrip, "{f} {hex} {roundtrip}");
    }
}

#[test]
fn test_remove_trailing_zeros() {
    assert!(remove_trailing_zeros(String::from("100")) == *"1");
    assert!(remove_trailing_zeros(String::from("100.00")) == *"100.");

    // leave leading zeros untouched
    assert!(remove_trailing_zeros(String::from("001")) == *"001");

    // leave strings untouched if they don't end with 0
    assert!(remove_trailing_zeros(String::from("101")) == *"101");
}

#[test]
fn test_remove_trailing_decimal_point() {
    assert!(remove_trailing_decimal_point(String::from("100.")) == *"100");
    assert!(remove_trailing_decimal_point(String::from("1.")) == *"1");

    // leave leading decimal points untouched
    assert!(remove_trailing_decimal_point(String::from(".5")) == *".5");
}

#[test]
fn test_maybe_remove_trailing_redundant_chars() {
    assert!(maybe_remove_trailing_redundant_chars(String::from("100."), true) == *"100.");
    assert!(maybe_remove_trailing_redundant_chars(String::from("100."), false) == *"100");
    assert!(maybe_remove_trailing_redundant_chars(String::from("1."), false) == *"1");
    assert!(maybe_remove_trailing_redundant_chars(String::from("10.0"), false) == *"10");

    // don't truncate integers
    assert!(maybe_remove_trailing_redundant_chars(String::from("1000"), false) == *"1000");
}
