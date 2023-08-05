//! Implementation of Printf-Style string formatting
//! as per the [Python Docs](https://docs.python.org/3/library/stdtypes.html#printf-style-string-formatting).
use bitflags::bitflags;
use num_traits::Signed;
use std::{
    cmp, fmt,
    iter::{Enumerate, Peekable},
    str::FromStr,
};

use crate::{float, Case};
use num_bigint::{BigInt, Sign};

#[derive(Debug, PartialEq)]
pub enum CFormatErrorType {
    UnmatchedKeyParentheses,
    MissingModuloSign,
    UnsupportedFormatChar(char),
    IncompleteFormat,
    IntTooBig,
    // Unimplemented,
}

// also contains how many chars the parsing function consumed
pub type ParsingError = (CFormatErrorType, usize);

#[derive(Debug, PartialEq)]
pub struct CFormatError {
    pub typ: CFormatErrorType, // FIXME
    pub index: usize,
}

impl fmt::Display for CFormatError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        use CFormatErrorType::{
            IncompleteFormat, IntTooBig, UnmatchedKeyParentheses, UnsupportedFormatChar,
        };
        match self.typ {
            UnmatchedKeyParentheses => write!(f, "incomplete format key"),
            IncompleteFormat => write!(f, "incomplete format"),
            UnsupportedFormatChar(c) => write!(
                f,
                "unsupported format character '{}' ({:#x}) at index {}",
                c, c as u32, self.index
            ),
            IntTooBig => write!(f, "width/precision too big"),
            CFormatErrorType::MissingModuloSign => {
                write!(f, "unexpected error parsing format string")
            }
        }
    }
}

pub type CFormatConversion = super::format::FormatConversion;

#[derive(Debug, PartialEq)]
pub enum CNumberType {
    Decimal,
    Octal,
    Hex(Case),
}

#[derive(Debug, PartialEq)]
pub enum CFloatType {
    Exponent(Case),
    PointDecimal(Case),
    General(Case),
}

#[derive(Debug, PartialEq)]
pub enum CFormatType {
    Number(CNumberType),
    Float(CFloatType),
    Character,
    String(CFormatConversion),
}

#[derive(Debug, PartialEq)]
pub enum CFormatPrecision {
    Quantity(CFormatQuantity),
    Dot,
}

impl From<CFormatQuantity> for CFormatPrecision {
    fn from(quantity: CFormatQuantity) -> Self {
        CFormatPrecision::Quantity(quantity)
    }
}

bitflags! {
    #[derive(Copy, Clone, Debug, PartialEq)]
    pub struct CConversionFlags: u32 {
        const ALTERNATE_FORM = 0b0000_0001;
        const ZERO_PAD = 0b0000_0010;
        const LEFT_ADJUST = 0b0000_0100;
        const BLANK_SIGN = 0b0000_1000;
        const SIGN_CHAR = 0b0001_0000;
    }
}

impl CConversionFlags {
    #[inline]
    pub fn sign_string(&self) -> &'static str {
        if self.contains(CConversionFlags::SIGN_CHAR) {
            "+"
        } else if self.contains(CConversionFlags::BLANK_SIGN) {
            " "
        } else {
            ""
        }
    }
}

#[derive(Debug, PartialEq)]
pub enum CFormatQuantity {
    Amount(usize),
    FromValuesTuple,
}

#[derive(Debug, PartialEq)]
pub struct CFormatSpec {
    pub mapping_key: Option<String>,
    pub flags: CConversionFlags,
    pub min_field_width: Option<CFormatQuantity>,
    pub precision: Option<CFormatPrecision>,
    pub format_type: CFormatType,
    pub format_char: char,
    // chars_consumed: usize,
}

impl FromStr for CFormatSpec {
    type Err = ParsingError;

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        let mut chars = text.chars().enumerate().peekable();
        if chars.next().map(|x| x.1) != Some('%') {
            return Err((CFormatErrorType::MissingModuloSign, 1));
        }

        CFormatSpec::parse(&mut chars)
    }
}

pub type ParseIter<I> = Peekable<Enumerate<I>>;

impl CFormatSpec {
    pub fn parse<T, I>(iter: &mut ParseIter<I>) -> Result<Self, ParsingError>
    where
        T: Into<char> + Copy,
        I: Iterator<Item = T>,
    {
        let mapping_key = parse_spec_mapping_key(iter)?;
        let flags = parse_flags(iter);
        let min_field_width = parse_quantity(iter)?;
        let precision = parse_precision(iter)?;
        consume_length(iter);
        let (format_type, format_char) = parse_format_type(iter)?;

        Ok(CFormatSpec {
            mapping_key,
            flags,
            min_field_width,
            precision,
            format_type,
            format_char,
        })
    }

    fn compute_fill_string(fill_char: char, fill_chars_needed: usize) -> String {
        (0..fill_chars_needed)
            .map(|_| fill_char)
            .collect::<String>()
    }

    fn fill_string(
        &self,
        string: String,
        fill_char: char,
        num_prefix_chars: Option<usize>,
    ) -> String {
        let mut num_chars = string.chars().count();
        if let Some(num_prefix_chars) = num_prefix_chars {
            num_chars += num_prefix_chars;
        }
        let num_chars = num_chars;

        let width = match &self.min_field_width {
            Some(CFormatQuantity::Amount(width)) => cmp::max(width, &num_chars),
            _ => &num_chars,
        };
        let fill_chars_needed = width.saturating_sub(num_chars);
        let fill_string = CFormatSpec::compute_fill_string(fill_char, fill_chars_needed);

        if fill_string.is_empty() {
            string
        } else {
            if self.flags.contains(CConversionFlags::LEFT_ADJUST) {
                format!("{string}{fill_string}")
            } else {
                format!("{fill_string}{string}")
            }
        }
    }

    fn fill_string_with_precision(&self, string: String, fill_char: char) -> String {
        let num_chars = string.chars().count();

        let width = match &self.precision {
            Some(CFormatPrecision::Quantity(CFormatQuantity::Amount(width))) => {
                cmp::max(width, &num_chars)
            }
            _ => &num_chars,
        };
        let fill_chars_needed = width.saturating_sub(num_chars);
        let fill_string = CFormatSpec::compute_fill_string(fill_char, fill_chars_needed);

        if fill_string.is_empty() {
            string
        } else {
            // Don't left-adjust if precision-filling: that will always be prepending 0s to %d
            // arguments, the LEFT_ADJUST flag will be used by a later call to fill_string with
            // the 0-filled string as the string param.
            format!("{fill_string}{string}")
        }
    }

    fn format_string_with_precision(
        &self,
        string: String,
        precision: Option<&CFormatPrecision>,
    ) -> String {
        // truncate if needed
        let string = match precision {
            Some(CFormatPrecision::Quantity(CFormatQuantity::Amount(precision)))
                if string.chars().count() > *precision =>
            {
                string.chars().take(*precision).collect::<String>()
            }
            Some(CFormatPrecision::Dot) => {
                // truncate to 0
                String::new()
            }
            _ => string,
        };
        self.fill_string(string, ' ', None)
    }

    #[inline]
    pub fn format_string(&self, string: String) -> String {
        self.format_string_with_precision(string, self.precision.as_ref())
    }

    #[inline]
    pub fn format_char(&self, ch: char) -> String {
        self.format_string_with_precision(
            ch.to_string(),
            Some(&(CFormatQuantity::Amount(1).into())),
        )
    }

    pub fn format_bytes(&self, bytes: &[u8]) -> Vec<u8> {
        let bytes = if let Some(CFormatPrecision::Quantity(CFormatQuantity::Amount(precision))) =
            self.precision
        {
            &bytes[..cmp::min(bytes.len(), precision)]
        } else {
            bytes
        };
        if let Some(CFormatQuantity::Amount(width)) = self.min_field_width {
            let fill = cmp::max(0, width - bytes.len());
            let mut v = Vec::with_capacity(bytes.len() + fill);
            if self.flags.contains(CConversionFlags::LEFT_ADJUST) {
                v.extend_from_slice(bytes);
                v.append(&mut vec![b' '; fill]);
            } else {
                v.append(&mut vec![b' '; fill]);
                v.extend_from_slice(bytes);
            }
            v
        } else {
            bytes.to_vec()
        }
    }

    pub fn format_number(&self, num: &BigInt) -> String {
        use CNumberType::{Decimal, Hex, Octal};
        let magnitude = num.abs();
        let prefix = if self.flags.contains(CConversionFlags::ALTERNATE_FORM) {
            match self.format_type {
                CFormatType::Number(Octal) => "0o",
                CFormatType::Number(Hex(Case::Lower)) => "0x",
                CFormatType::Number(Hex(Case::Upper)) => "0X",
                _ => "",
            }
        } else {
            ""
        };

        let magnitude_string: String = match self.format_type {
            CFormatType::Number(Decimal) => magnitude.to_str_radix(10),
            CFormatType::Number(Octal) => magnitude.to_str_radix(8),
            CFormatType::Number(Hex(Case::Lower)) => magnitude.to_str_radix(16),
            CFormatType::Number(Hex(Case::Upper)) => {
                let mut result = magnitude.to_str_radix(16);
                result.make_ascii_uppercase();
                result
            }
            _ => unreachable!(), // Should not happen because caller has to make sure that this is a number
        };

        let sign_string = match num.sign() {
            Sign::Minus => "-",
            _ => self.flags.sign_string(),
        };

        let padded_magnitude_string = self.fill_string_with_precision(magnitude_string, '0');

        if self.flags.contains(CConversionFlags::ZERO_PAD) {
            let fill_char = if self.flags.contains(CConversionFlags::LEFT_ADJUST) {
                ' ' // '-' overrides the '0' conversion if both are given
            } else {
                '0'
            };
            let signed_prefix = format!("{sign_string}{prefix}");
            format!(
                "{}{}",
                signed_prefix,
                self.fill_string(
                    padded_magnitude_string,
                    fill_char,
                    Some(signed_prefix.chars().count()),
                ),
            )
        } else {
            self.fill_string(
                format!("{sign_string}{prefix}{padded_magnitude_string}"),
                ' ',
                None,
            )
        }
    }

    pub fn format_float(&self, num: f64) -> String {
        let sign_string = if num.is_sign_negative() && !num.is_nan() {
            "-"
        } else {
            self.flags.sign_string()
        };

        let precision = match &self.precision {
            Some(CFormatPrecision::Quantity(quantity)) => match quantity {
                CFormatQuantity::Amount(amount) => *amount,
                CFormatQuantity::FromValuesTuple => 6,
            },
            Some(CFormatPrecision::Dot) => 0,
            None => 6,
        };

        let magnitude_string = match &self.format_type {
            CFormatType::Float(CFloatType::PointDecimal(case)) => {
                let magnitude = num.abs();
                float::format_fixed(
                    precision,
                    magnitude,
                    *case,
                    self.flags.contains(CConversionFlags::ALTERNATE_FORM),
                )
            }
            CFormatType::Float(CFloatType::Exponent(case)) => {
                let magnitude = num.abs();
                float::format_exponent(
                    precision,
                    magnitude,
                    *case,
                    self.flags.contains(CConversionFlags::ALTERNATE_FORM),
                )
            }
            CFormatType::Float(CFloatType::General(case)) => {
                let precision = if precision == 0 { 1 } else { precision };
                let magnitude = num.abs();
                float::format_general(
                    precision,
                    magnitude,
                    *case,
                    self.flags.contains(CConversionFlags::ALTERNATE_FORM),
                    false,
                )
            }
            _ => unreachable!(),
        };

        if self.flags.contains(CConversionFlags::ZERO_PAD) {
            let fill_char = if self.flags.contains(CConversionFlags::LEFT_ADJUST) {
                ' '
            } else {
                '0'
            };
            format!(
                "{}{}",
                sign_string,
                self.fill_string(
                    magnitude_string,
                    fill_char,
                    Some(sign_string.chars().count()),
                )
            )
        } else {
            self.fill_string(format!("{sign_string}{magnitude_string}"), ' ', None)
        }
    }
}

fn parse_spec_mapping_key<T, I>(iter: &mut ParseIter<I>) -> Result<Option<String>, ParsingError>
where
    T: Into<char> + Copy,
    I: Iterator<Item = T>,
{
    if let Some(&(index, c)) = iter.peek() {
        if c.into() == '(' {
            iter.next().unwrap();
            return match parse_text_inside_parentheses(iter) {
                Some(key) => Ok(Some(key)),
                None => Err((CFormatErrorType::UnmatchedKeyParentheses, index)),
            };
        }
    }
    Ok(None)
}

fn parse_flags<T, I>(iter: &mut ParseIter<I>) -> CConversionFlags
where
    T: Into<char> + Copy,
    I: Iterator<Item = T>,
{
    let mut flags = CConversionFlags::empty();
    while let Some(&(_, c)) = iter.peek() {
        let flag = match c.into() {
            '#' => CConversionFlags::ALTERNATE_FORM,
            '0' => CConversionFlags::ZERO_PAD,
            '-' => CConversionFlags::LEFT_ADJUST,
            ' ' => CConversionFlags::BLANK_SIGN,
            '+' => CConversionFlags::SIGN_CHAR,
            _ => break,
        };
        iter.next().unwrap();
        flags |= flag;
    }
    flags
}

fn consume_length<T, I>(iter: &mut ParseIter<I>)
where
    T: Into<char> + Copy,
    I: Iterator<Item = T>,
{
    if let Some(&(_, c)) = iter.peek() {
        let c = c.into();
        if c == 'h' || c == 'l' || c == 'L' {
            iter.next().unwrap();
        }
    }
}

fn parse_format_type<T, I>(iter: &mut ParseIter<I>) -> Result<(CFormatType, char), ParsingError>
where
    T: Into<char>,
    I: Iterator<Item = T>,
{
    use CFloatType::{Exponent, General, PointDecimal};
    use CNumberType::{Decimal, Hex, Octal};
    let (index, c) = match iter.next() {
        Some((index, c)) => (index, c.into()),
        None => {
            return Err((
                CFormatErrorType::IncompleteFormat,
                iter.peek().map_or(0, |x| x.0),
            ));
        }
    };
    let format_type = match c {
        'd' | 'i' | 'u' => CFormatType::Number(Decimal),
        'o' => CFormatType::Number(Octal),
        'x' => CFormatType::Number(Hex(Case::Lower)),
        'X' => CFormatType::Number(Hex(Case::Upper)),
        'e' => CFormatType::Float(Exponent(Case::Lower)),
        'E' => CFormatType::Float(Exponent(Case::Upper)),
        'f' => CFormatType::Float(PointDecimal(Case::Lower)),
        'F' => CFormatType::Float(PointDecimal(Case::Upper)),
        'g' => CFormatType::Float(General(Case::Lower)),
        'G' => CFormatType::Float(General(Case::Upper)),
        'c' => CFormatType::Character,
        'r' => CFormatType::String(CFormatConversion::Repr),
        's' => CFormatType::String(CFormatConversion::Str),
        'b' => CFormatType::String(CFormatConversion::Bytes),
        'a' => CFormatType::String(CFormatConversion::Ascii),
        _ => return Err((CFormatErrorType::UnsupportedFormatChar(c), index)),
    };
    Ok((format_type, c))
}

#[allow(clippy::cast_possible_wrap)]
fn parse_quantity<T, I>(iter: &mut ParseIter<I>) -> Result<Option<CFormatQuantity>, ParsingError>
where
    T: Into<char> + Copy,
    I: Iterator<Item = T>,
{
    if let Some(&(_, c)) = iter.peek() {
        let c: char = c.into();
        if c == '*' {
            iter.next().unwrap();
            return Ok(Some(CFormatQuantity::FromValuesTuple));
        }
        if let Some(i) = c.to_digit(10) {
            let mut num = i as i32;
            iter.next().unwrap();
            while let Some(&(index, c)) = iter.peek() {
                if let Some(i) = c.into().to_digit(10) {
                    num = num
                        .checked_mul(10)
                        .and_then(|num| num.checked_add(i as i32))
                        .ok_or((CFormatErrorType::IntTooBig, index))?;
                    iter.next().unwrap();
                } else {
                    break;
                }
            }
            return Ok(Some(CFormatQuantity::Amount(num.unsigned_abs() as usize)));
        }
    }
    Ok(None)
}

fn parse_precision<T, I>(iter: &mut ParseIter<I>) -> Result<Option<CFormatPrecision>, ParsingError>
where
    T: Into<char> + Copy,
    I: Iterator<Item = T>,
{
    if let Some(&(_, c)) = iter.peek() {
        if c.into() == '.' {
            iter.next().unwrap();
            let quantity = parse_quantity(iter)?;
            let precision = quantity.map_or(CFormatPrecision::Dot, CFormatPrecision::Quantity);
            return Ok(Some(precision));
        }
    }
    Ok(None)
}

fn parse_text_inside_parentheses<T, I>(iter: &mut ParseIter<I>) -> Option<String>
where
    T: Into<char>,
    I: Iterator<Item = T>,
{
    let mut counter: i32 = 1;
    let mut contained_text = String::new();
    loop {
        let (_, c) = iter.next()?;
        let c = c.into();
        match c {
            _ if c == '(' => {
                counter += 1;
            }
            _ if c == ')' => {
                counter -= 1;
            }
            _ => (),
        }

        if counter > 0 {
            contained_text.push(c);
        } else {
            break;
        }
    }

    Some(contained_text)
}

#[derive(Debug, PartialEq)]
pub enum CFormatPart<T> {
    Literal(T),
    Spec(CFormatSpec),
}

impl<T> CFormatPart<T> {
    #[inline]
    pub fn is_specifier(&self) -> bool {
        matches!(self, CFormatPart::Spec(_))
    }

    #[inline]
    pub fn has_key(&self) -> bool {
        match self {
            CFormatPart::Spec(s) => s.mapping_key.is_some(),
            CFormatPart::Literal(_) => false,
        }
    }
}

#[derive(Debug, PartialEq)]
pub struct CFormatStrOrBytes<S> {
    parts: Vec<(usize, CFormatPart<S>)>,
}

impl<S> CFormatStrOrBytes<S> {
    pub fn check_specifiers(&self) -> Option<(usize, bool)> {
        let mut count = 0;
        let mut mapping_required = false;
        for (_, part) in &self.parts {
            if part.is_specifier() {
                let has_key = part.has_key();
                if count == 0 {
                    mapping_required = has_key;
                } else if mapping_required != has_key {
                    return None;
                }
                count += 1;
            }
        }
        Some((count, mapping_required))
    }

    #[inline]
    pub fn iter(&self) -> impl Iterator<Item = &(usize, CFormatPart<S>)> {
        self.parts.iter()
    }

    #[inline]
    pub fn iter_mut(&mut self) -> impl Iterator<Item = &mut (usize, CFormatPart<S>)> {
        self.parts.iter_mut()
    }
}

pub type CFormatBytes = CFormatStrOrBytes<Vec<u8>>;

impl CFormatBytes {
    pub fn parse<I: Iterator<Item = u8>>(iter: &mut ParseIter<I>) -> Result<Self, CFormatError> {
        let mut parts = vec![];
        let mut literal = vec![];
        let mut part_index = 0;
        while let Some((index, c)) = iter.next() {
            if c == b'%' {
                if let Some(&(_, second)) = iter.peek() {
                    if second == b'%' {
                        iter.next().unwrap();
                        literal.push(b'%');
                        continue;
                    }
                    if !literal.is_empty() {
                        parts.push((
                            part_index,
                            CFormatPart::Literal(std::mem::take(&mut literal)),
                        ));
                    }
                    let spec = CFormatSpec::parse(iter).map_err(|err| CFormatError {
                        typ: err.0,
                        index: err.1,
                    })?;
                    parts.push((index, CFormatPart::Spec(spec)));
                    if let Some(&(index, _)) = iter.peek() {
                        part_index = index;
                    }
                } else {
                    return Err(CFormatError {
                        typ: CFormatErrorType::IncompleteFormat,
                        index: index + 1,
                    });
                }
            } else {
                literal.push(c);
            }
        }
        if !literal.is_empty() {
            parts.push((part_index, CFormatPart::Literal(literal)));
        }
        Ok(Self { parts })
    }

    pub fn parse_from_bytes(bytes: &[u8]) -> Result<Self, CFormatError> {
        let mut iter = bytes.iter().copied().enumerate().peekable();
        Self::parse(&mut iter)
    }
}

pub type CFormatString = CFormatStrOrBytes<String>;

impl FromStr for CFormatString {
    type Err = CFormatError;

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        let mut iter = text.chars().enumerate().peekable();
        Self::parse(&mut iter)
    }
}

impl CFormatString {
    pub fn parse<I: Iterator<Item = char>>(iter: &mut ParseIter<I>) -> Result<Self, CFormatError> {
        let mut parts = vec![];
        let mut literal = String::new();
        let mut part_index = 0;
        while let Some((index, c)) = iter.next() {
            if c == '%' {
                if let Some(&(_, second)) = iter.peek() {
                    if second == '%' {
                        iter.next().unwrap();
                        literal.push('%');
                        continue;
                    }
                    if !literal.is_empty() {
                        parts.push((
                            part_index,
                            CFormatPart::Literal(std::mem::take(&mut literal)),
                        ));
                    }
                    let spec = CFormatSpec::parse(iter).map_err(|err| CFormatError {
                        typ: err.0,
                        index: err.1,
                    })?;
                    parts.push((index, CFormatPart::Spec(spec)));
                    if let Some(&(index, _)) = iter.peek() {
                        part_index = index;
                    }
                } else {
                    return Err(CFormatError {
                        typ: CFormatErrorType::IncompleteFormat,
                        index: index + 1,
                    });
                }
            } else {
                literal.push(c);
            }
        }
        if !literal.is_empty() {
            parts.push((part_index, CFormatPart::Literal(literal)));
        }
        Ok(Self { parts })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_fill_and_align() {
        assert_eq!(
            "%10s"
                .parse::<CFormatSpec>()
                .unwrap()
                .format_string("test".to_owned()),
            "      test".to_owned()
        );
        assert_eq!(
            "%-10s"
                .parse::<CFormatSpec>()
                .unwrap()
                .format_string("test".to_owned()),
            "test      ".to_owned()
        );
        assert_eq!(
            "%#10x"
                .parse::<CFormatSpec>()
                .unwrap()
                .format_number(&BigInt::from(0x1337)),
            "    0x1337".to_owned()
        );
        assert_eq!(
            "%-#10x"
                .parse::<CFormatSpec>()
                .unwrap()
                .format_number(&BigInt::from(0x1337)),
            "0x1337    ".to_owned()
        );
    }

    #[test]
    fn test_parse_key() {
        let expected = Ok(CFormatSpec {
            mapping_key: Some("amount".to_owned()),
            format_type: CFormatType::Number(CNumberType::Decimal),
            format_char: 'd',
            min_field_width: None,
            precision: None,
            flags: CConversionFlags::empty(),
        });
        assert_eq!("%(amount)d".parse::<CFormatSpec>(), expected);

        let expected = Ok(CFormatSpec {
            mapping_key: Some("m((u(((l((((ti))))p)))l))e".to_owned()),
            format_type: CFormatType::Number(CNumberType::Decimal),
            format_char: 'd',
            min_field_width: None,
            precision: None,
            flags: CConversionFlags::empty(),
        });
        assert_eq!(
            "%(m((u(((l((((ti))))p)))l))e)d".parse::<CFormatSpec>(),
            expected
        );
    }

    #[test]
    fn test_format_parse_key_fail() {
        assert_eq!(
            "%(aged".parse::<CFormatString>(),
            Err(CFormatError {
                typ: CFormatErrorType::UnmatchedKeyParentheses,
                index: 1
            })
        );
    }

    #[test]
    fn test_format_parse_type_fail() {
        assert_eq!(
            "Hello %n".parse::<CFormatString>(),
            Err(CFormatError {
                typ: CFormatErrorType::UnsupportedFormatChar('n'),
                index: 7
            })
        );
    }

    #[test]
    fn test_incomplete_format_fail() {
        assert_eq!(
            "Hello %".parse::<CFormatString>(),
            Err(CFormatError {
                typ: CFormatErrorType::IncompleteFormat,
                index: 7
            })
        );
    }

    #[test]
    fn test_parse_flags() {
        let expected = Ok(CFormatSpec {
            format_type: CFormatType::Number(CNumberType::Decimal),
            format_char: 'd',
            min_field_width: Some(CFormatQuantity::Amount(10)),
            precision: None,
            mapping_key: None,
            flags: CConversionFlags::all(),
        });
        let parsed = "%  0   -+++###10d".parse::<CFormatSpec>();
        assert_eq!(parsed, expected);
        assert_eq!(
            parsed.unwrap().format_number(&BigInt::from(12)),
            "+12       ".to_owned()
        );
    }

    #[test]
    fn test_parse_and_format_string() {
        assert_eq!(
            "%5.4s"
                .parse::<CFormatSpec>()
                .unwrap()
                .format_string("Hello, World!".to_owned()),
            " Hell".to_owned()
        );
        assert_eq!(
            "%-5.4s"
                .parse::<CFormatSpec>()
                .unwrap()
                .format_string("Hello, World!".to_owned()),
            "Hell ".to_owned()
        );
        assert_eq!(
            "%.s"
                .parse::<CFormatSpec>()
                .unwrap()
                .format_string("Hello, World!".to_owned()),
            String::new()
        );
        assert_eq!(
            "%5.s"
                .parse::<CFormatSpec>()
                .unwrap()
                .format_string("Hello, World!".to_owned()),
            "     ".to_owned()
        );
    }

    #[test]
    fn test_parse_and_format_unicode_string() {
        assert_eq!(
            "%.2s"
                .parse::<CFormatSpec>()
                .unwrap()
                .format_string("❤❤❤❤❤❤❤❤".to_owned()),
            "❤❤".to_owned()
        );
    }

    #[test]
    fn test_parse_and_format_number() {
        assert_eq!(
            "%5d"
                .parse::<CFormatSpec>()
                .unwrap()
                .format_number(&BigInt::from(27)),
            "   27".to_owned()
        );
        assert_eq!(
            "%05d"
                .parse::<CFormatSpec>()
                .unwrap()
                .format_number(&BigInt::from(27)),
            "00027".to_owned()
        );
        assert_eq!(
            "%.5d"
                .parse::<CFormatSpec>()
                .unwrap()
                .format_number(&BigInt::from(27)),
            "00027".to_owned()
        );
        assert_eq!(
            "%+05d"
                .parse::<CFormatSpec>()
                .unwrap()
                .format_number(&BigInt::from(27)),
            "+0027".to_owned()
        );
        assert_eq!(
            "%-d"
                .parse::<CFormatSpec>()
                .unwrap()
                .format_number(&BigInt::from(-27)),
            "-27".to_owned()
        );
        assert_eq!(
            "% d"
                .parse::<CFormatSpec>()
                .unwrap()
                .format_number(&BigInt::from(27)),
            " 27".to_owned()
        );
        assert_eq!(
            "% d"
                .parse::<CFormatSpec>()
                .unwrap()
                .format_number(&BigInt::from(-27)),
            "-27".to_owned()
        );
        assert_eq!(
            "%08x"
                .parse::<CFormatSpec>()
                .unwrap()
                .format_number(&BigInt::from(0x1337)),
            "00001337".to_owned()
        );
        assert_eq!(
            "%#010x"
                .parse::<CFormatSpec>()
                .unwrap()
                .format_number(&BigInt::from(0x1337)),
            "0x00001337".to_owned()
        );
        assert_eq!(
            "%-#010x"
                .parse::<CFormatSpec>()
                .unwrap()
                .format_number(&BigInt::from(0x1337)),
            "0x1337    ".to_owned()
        );
    }

    #[test]
    fn test_parse_and_format_float() {
        assert_eq!(
            "%f".parse::<CFormatSpec>().unwrap().format_float(1.2345),
            "1.234500"
        );
        assert_eq!(
            "%.2f".parse::<CFormatSpec>().unwrap().format_float(1.2345),
            "1.23"
        );
        assert_eq!(
            "%.f".parse::<CFormatSpec>().unwrap().format_float(1.2345),
            "1"
        );
        assert_eq!(
            "%+.f".parse::<CFormatSpec>().unwrap().format_float(1.2345),
            "+1"
        );
        assert_eq!(
            "%+f".parse::<CFormatSpec>().unwrap().format_float(1.2345),
            "+1.234500"
        );
        assert_eq!(
            "% f".parse::<CFormatSpec>().unwrap().format_float(1.2345),
            " 1.234500"
        );
        assert_eq!(
            "%f".parse::<CFormatSpec>().unwrap().format_float(-1.2345),
            "-1.234500"
        );
        assert_eq!(
            "%f".parse::<CFormatSpec>()
                .unwrap()
                .format_float(1.234_567_890_1),
            "1.234568"
        );
    }

    #[test]
    fn test_format_parse() {
        let fmt = "Hello, my name is %s and I'm %d years old";
        let expected = Ok(CFormatString {
            parts: vec![
                (0, CFormatPart::Literal("Hello, my name is ".to_owned())),
                (
                    18,
                    CFormatPart::Spec(CFormatSpec {
                        format_type: CFormatType::String(CFormatConversion::Str),
                        format_char: 's',
                        mapping_key: None,
                        min_field_width: None,
                        precision: None,
                        flags: CConversionFlags::empty(),
                    }),
                ),
                (20, CFormatPart::Literal(" and I'm ".to_owned())),
                (
                    29,
                    CFormatPart::Spec(CFormatSpec {
                        format_type: CFormatType::Number(CNumberType::Decimal),
                        format_char: 'd',
                        mapping_key: None,
                        min_field_width: None,
                        precision: None,
                        flags: CConversionFlags::empty(),
                    }),
                ),
                (31, CFormatPart::Literal(" years old".to_owned())),
            ],
        });
        let result = fmt.parse::<CFormatString>();
        assert_eq!(
            result, expected,
            "left = {result:#?} \n\n\n right = {expected:#?}"
        );
    }
}
