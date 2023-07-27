use itertools::{Itertools, PeekingNext};

use num_traits::{cast::ToPrimitive, FromPrimitive, Signed};
use std::ops::Deref;
use std::{cmp, str::FromStr};

use crate::{float, Case};
use num_bigint::{BigInt, Sign};

trait FormatParse {
    fn parse(text: &str) -> (Option<Self>, &str)
    where
        Self: Sized;
}

#[derive(Debug, Copy, Clone, PartialEq)]
pub enum FormatConversion {
    Str,
    Repr,
    Ascii,
    Bytes,
}

impl FormatParse for FormatConversion {
    fn parse(text: &str) -> (Option<Self>, &str) {
        let Some(conversion) = Self::from_string(text) else {
            return (None, text);
        };
        let mut chars = text.chars();
        chars.next(); // Consume the bang
        chars.next(); // Consume one r,s,a char
        (Some(conversion), chars.as_str())
    }
}

impl FormatConversion {
    pub fn from_char(c: char) -> Option<FormatConversion> {
        match c {
            's' => Some(FormatConversion::Str),
            'r' => Some(FormatConversion::Repr),
            'a' => Some(FormatConversion::Ascii),
            'b' => Some(FormatConversion::Bytes),
            _ => None,
        }
    }

    fn from_string(text: &str) -> Option<FormatConversion> {
        let mut chars = text.chars();
        if chars.next() != Some('!') {
            return None;
        }

        FormatConversion::from_char(chars.next()?)
    }
}

#[derive(Debug, Copy, Clone, PartialEq)]
pub enum FormatAlign {
    Left,
    Right,
    AfterSign,
    Center,
}

impl FormatAlign {
    fn from_char(c: char) -> Option<FormatAlign> {
        match c {
            '<' => Some(FormatAlign::Left),
            '>' => Some(FormatAlign::Right),
            '=' => Some(FormatAlign::AfterSign),
            '^' => Some(FormatAlign::Center),
            _ => None,
        }
    }
}

impl FormatParse for FormatAlign {
    fn parse(text: &str) -> (Option<Self>, &str) {
        let mut chars = text.chars();
        if let Some(maybe_align) = chars.next().and_then(Self::from_char) {
            (Some(maybe_align), chars.as_str())
        } else {
            (None, text)
        }
    }
}

#[derive(Debug, Copy, Clone, PartialEq)]
pub enum FormatSign {
    Plus,
    Minus,
    MinusOrSpace,
}

impl FormatParse for FormatSign {
    fn parse(text: &str) -> (Option<Self>, &str) {
        let mut chars = text.chars();
        match chars.next() {
            Some('-') => (Some(Self::Minus), chars.as_str()),
            Some('+') => (Some(Self::Plus), chars.as_str()),
            Some(' ') => (Some(Self::MinusOrSpace), chars.as_str()),
            _ => (None, text),
        }
    }
}

#[derive(Debug, PartialEq)]
pub enum FormatGrouping {
    Comma,
    Underscore,
}

impl FormatParse for FormatGrouping {
    fn parse(text: &str) -> (Option<Self>, &str) {
        let mut chars = text.chars();
        match chars.next() {
            Some('_') => (Some(Self::Underscore), chars.as_str()),
            Some(',') => (Some(Self::Comma), chars.as_str()),
            _ => (None, text),
        }
    }
}

#[derive(Copy, Clone, Debug, PartialEq)]
pub enum FormatType {
    String,
    Binary,
    Character,
    Decimal,
    Octal,
    Number(Case),
    Hex(Case),
    Exponent(Case),
    GeneralFormat(Case),
    FixedPoint(Case),
    Percentage,
}

impl From<&FormatType> for char {
    fn from(from: &FormatType) -> char {
        match from {
            FormatType::String => 's',
            FormatType::Binary => 'b',
            FormatType::Character => 'c',
            FormatType::Decimal => 'd',
            FormatType::Octal => 'o',
            FormatType::Number(Case::Lower) => 'n',
            FormatType::Number(Case::Upper) => 'N',
            FormatType::Hex(Case::Lower) => 'x',
            FormatType::Hex(Case::Upper) => 'X',
            FormatType::Exponent(Case::Lower) => 'e',
            FormatType::Exponent(Case::Upper) => 'E',
            FormatType::GeneralFormat(Case::Lower) => 'g',
            FormatType::GeneralFormat(Case::Upper) => 'G',
            FormatType::FixedPoint(Case::Lower) => 'f',
            FormatType::FixedPoint(Case::Upper) => 'F',
            FormatType::Percentage => '%',
        }
    }
}

impl FormatParse for FormatType {
    fn parse(text: &str) -> (Option<Self>, &str) {
        let mut chars = text.chars();
        match chars.next() {
            Some('s') => (Some(Self::String), chars.as_str()),
            Some('b') => (Some(Self::Binary), chars.as_str()),
            Some('c') => (Some(Self::Character), chars.as_str()),
            Some('d') => (Some(Self::Decimal), chars.as_str()),
            Some('o') => (Some(Self::Octal), chars.as_str()),
            Some('n') => (Some(Self::Number(Case::Lower)), chars.as_str()),
            Some('N') => (Some(Self::Number(Case::Upper)), chars.as_str()),
            Some('x') => (Some(Self::Hex(Case::Lower)), chars.as_str()),
            Some('X') => (Some(Self::Hex(Case::Upper)), chars.as_str()),
            Some('e') => (Some(Self::Exponent(Case::Lower)), chars.as_str()),
            Some('E') => (Some(Self::Exponent(Case::Upper)), chars.as_str()),
            Some('f') => (Some(Self::FixedPoint(Case::Lower)), chars.as_str()),
            Some('F') => (Some(Self::FixedPoint(Case::Upper)), chars.as_str()),
            Some('g') => (Some(Self::GeneralFormat(Case::Lower)), chars.as_str()),
            Some('G') => (Some(Self::GeneralFormat(Case::Upper)), chars.as_str()),
            Some('%') => (Some(Self::Percentage), chars.as_str()),
            _ => (None, text),
        }
    }
}

#[derive(Debug, PartialEq)]
pub struct FormatSpec {
    conversion: Option<FormatConversion>,
    fill: Option<char>,
    align: Option<FormatAlign>,
    sign: Option<FormatSign>,
    alternate_form: bool,
    width: Option<usize>,
    grouping_option: Option<FormatGrouping>,
    precision: Option<usize>,
    format_type: Option<FormatType>,
}

fn get_num_digits(text: &str) -> usize {
    for (index, character) in text.char_indices() {
        if !character.is_ascii_digit() {
            return index;
        }
    }
    text.len()
}

fn parse_fill_and_align(text: &str) -> (Option<char>, Option<FormatAlign>, &str) {
    let char_indices: Vec<(usize, char)> = text.char_indices().take(3).collect();
    if char_indices.is_empty() {
        (None, None, text)
    } else if char_indices.len() == 1 {
        let (maybe_align, remaining) = FormatAlign::parse(text);
        (None, maybe_align, remaining)
    } else {
        let (maybe_align, remaining) = FormatAlign::parse(&text[char_indices[1].0..]);
        if maybe_align.is_some() {
            (Some(char_indices[0].1), maybe_align, remaining)
        } else {
            let (only_align, only_align_remaining) = FormatAlign::parse(text);
            (None, only_align, only_align_remaining)
        }
    }
}

fn parse_number(text: &str) -> Result<(Option<usize>, &str), FormatSpecError> {
    let num_digits: usize = get_num_digits(text);
    if num_digits == 0 {
        return Ok((None, text));
    }
    if let Ok(num) = text[..num_digits].parse::<usize>() {
        Ok((Some(num), &text[num_digits..]))
    } else {
        // NOTE: this condition is different from CPython
        Err(FormatSpecError::DecimalDigitsTooMany)
    }
}

fn parse_alternate_form(text: &str) -> (bool, &str) {
    let mut chars = text.chars();
    match chars.next() {
        Some('#') => (true, chars.as_str()),
        _ => (false, text),
    }
}

fn parse_zero(text: &str) -> (bool, &str) {
    let mut chars = text.chars();
    match chars.next() {
        Some('0') => (true, chars.as_str()),
        _ => (false, text),
    }
}

fn parse_precision(text: &str) -> Result<(Option<usize>, &str), FormatSpecError> {
    let mut chars = text.chars();
    Ok(match chars.next() {
        Some('.') => {
            let (size, remaining) = parse_number(chars.as_str())?;
            if let Some(size) = size {
                if size > i32::MAX as usize {
                    return Err(FormatSpecError::PrecisionTooBig);
                }
                (Some(size), remaining)
            } else {
                (None, text)
            }
        }
        _ => (None, text),
    })
}

impl FormatSpec {
    pub fn parse(text: &str) -> Result<Self, FormatSpecError> {
        // get_integer in CPython
        let (conversion, text) = FormatConversion::parse(text);
        let (mut fill, mut align, text) = parse_fill_and_align(text);
        let (sign, text) = FormatSign::parse(text);
        let (alternate_form, text) = parse_alternate_form(text);
        let (zero, text) = parse_zero(text);
        let (width, text) = parse_number(text)?;
        let (grouping_option, text) = FormatGrouping::parse(text);
        let (precision, text) = parse_precision(text)?;
        let (format_type, text) = FormatType::parse(text);
        if !text.is_empty() {
            return Err(FormatSpecError::InvalidFormatSpecifier);
        }

        if zero && fill.is_none() {
            fill.replace('0');
            align = align.or(Some(FormatAlign::AfterSign));
        }

        Ok(FormatSpec {
            conversion,
            fill,
            align,
            sign,
            alternate_form,
            width,
            grouping_option,
            precision,
            format_type,
        })
    }

    fn compute_fill_string(fill_char: char, fill_chars_needed: i32) -> String {
        (0..fill_chars_needed)
            .map(|_| fill_char)
            .collect::<String>()
    }

    #[allow(
        clippy::cast_possible_wrap,
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss
    )]
    fn add_magnitude_separators_for_char(
        magnitude_str: &str,
        inter: i32,
        sep: char,
        disp_digit_cnt: i32,
    ) -> String {
        // Don't add separators to the floating decimal point of numbers
        let mut parts = magnitude_str.splitn(2, '.');
        let magnitude_int_str = parts.next().unwrap().to_string();
        let dec_digit_cnt = magnitude_str.len() as i32 - magnitude_int_str.len() as i32;
        let int_digit_cnt = disp_digit_cnt - dec_digit_cnt;
        let mut result = FormatSpec::separate_integer(magnitude_int_str, inter, sep, int_digit_cnt);
        if let Some(part) = parts.next() {
            result.push_str(&format!(".{part}"));
        }
        result
    }

    #[allow(
        clippy::cast_sign_loss,
        clippy::cast_possible_wrap,
        clippy::cast_possible_truncation
    )]
    fn separate_integer(
        magnitude_str: String,
        inter: i32,
        sep: char,
        disp_digit_cnt: i32,
    ) -> String {
        let magnitude_len = magnitude_str.len() as i32;
        let offset = i32::from(disp_digit_cnt % (inter + 1) == 0);
        let disp_digit_cnt = disp_digit_cnt + offset;
        let pad_cnt = disp_digit_cnt - magnitude_len;
        let sep_cnt = disp_digit_cnt / (inter + 1);
        let diff = pad_cnt - sep_cnt;
        if pad_cnt > 0 && diff > 0 {
            // separate with 0 padding
            let padding = "0".repeat(diff as usize);
            let padded_num = format!("{padding}{magnitude_str}");
            FormatSpec::insert_separator(padded_num, inter, sep, sep_cnt)
        } else {
            // separate without padding
            let sep_cnt = (magnitude_len - 1) / inter;
            FormatSpec::insert_separator(magnitude_str, inter, sep, sep_cnt)
        }
    }

    #[allow(
        clippy::cast_sign_loss,
        clippy::cast_possible_truncation,
        clippy::cast_possible_wrap
    )]
    fn insert_separator(mut magnitude_str: String, inter: i32, sep: char, sep_cnt: i32) -> String {
        let magnitude_len = magnitude_str.len() as i32;
        for i in 1..=sep_cnt {
            magnitude_str.insert((magnitude_len - inter * i) as usize, sep);
        }
        magnitude_str
    }

    fn validate_format(&self, default_format_type: FormatType) -> Result<(), FormatSpecError> {
        let format_type = self.format_type.as_ref().unwrap_or(&default_format_type);
        match (&self.grouping_option, format_type) {
            (
                Some(FormatGrouping::Comma),
                FormatType::String
                | FormatType::Character
                | FormatType::Binary
                | FormatType::Octal
                | FormatType::Hex(_)
                | FormatType::Number(_),
            ) => {
                let ch = char::from(format_type);
                Err(FormatSpecError::UnspecifiedFormat(',', ch))
            }
            (
                Some(FormatGrouping::Underscore),
                FormatType::String | FormatType::Character | FormatType::Number(_),
            ) => {
                let ch = char::from(format_type);
                Err(FormatSpecError::UnspecifiedFormat('_', ch))
            }
            _ => Ok(()),
        }
    }

    fn get_separator_interval(&self) -> usize {
        match self.format_type {
            Some(FormatType::Binary | FormatType::Octal | FormatType::Hex(_)) => 4,
            Some(FormatType::Decimal | FormatType::Number(_) | FormatType::FixedPoint(_)) => 3,
            None => 3,
            _ => panic!("Separators only valid for numbers!"),
        }
    }

    #[allow(clippy::cast_possible_wrap, clippy::cast_possible_truncation)]
    fn add_magnitude_separators(&self, magnitude_str: String, prefix: &str) -> String {
        match &self.grouping_option {
            Some(fg) => {
                let sep = match fg {
                    FormatGrouping::Comma => ',',
                    FormatGrouping::Underscore => '_',
                };
                let inter = self.get_separator_interval().try_into().unwrap();
                let magnitude_len = magnitude_str.len();
                let width = self.width.unwrap_or(magnitude_len) as i32 - prefix.len() as i32;
                let disp_digit_cnt = cmp::max(width, magnitude_len as i32);
                FormatSpec::add_magnitude_separators_for_char(
                    &magnitude_str,
                    inter,
                    sep,
                    disp_digit_cnt,
                )
            }
            None => magnitude_str,
        }
    }

    pub fn format_bool(&self, input: bool) -> Result<String, FormatSpecError> {
        let x = u8::from(input);
        match &self.format_type {
            Some(
                FormatType::Binary
                | FormatType::Decimal
                | FormatType::Octal
                | FormatType::Number(Case::Lower)
                | FormatType::Hex(_)
                | FormatType::GeneralFormat(_)
                | FormatType::Character,
            ) => self.format_int(&BigInt::from_u8(x).unwrap()),
            Some(FormatType::Exponent(_) | FormatType::FixedPoint(_) | FormatType::Percentage) => {
                self.format_float(f64::from(x))
            }
            None => {
                let first_letter = (input.to_string().as_bytes()[0] as char).to_uppercase();
                Ok(first_letter.collect::<String>() + &input.to_string()[1..])
            }
            _ => Err(FormatSpecError::InvalidFormatSpecifier),
        }
    }

    pub fn format_float(&self, num: f64) -> Result<String, FormatSpecError> {
        self.validate_format(FormatType::FixedPoint(Case::Lower))?;
        let precision = self.precision.unwrap_or(6);
        let magnitude = num.abs();
        let raw_magnitude_str: Result<String, FormatSpecError> = match &self.format_type {
            Some(FormatType::FixedPoint(case)) => Ok(float::format_fixed(
                precision,
                magnitude,
                *case,
                self.alternate_form,
            )),
            Some(
                FormatType::Decimal
                | FormatType::Binary
                | FormatType::Octal
                | FormatType::Hex(_)
                | FormatType::String
                | FormatType::Character
                | FormatType::Number(Case::Upper),
            ) => {
                let ch = char::from(self.format_type.as_ref().unwrap());
                Err(FormatSpecError::UnknownFormatCode(ch, "float"))
            }
            Some(FormatType::GeneralFormat(case) | FormatType::Number(case)) => {
                let precision = if precision == 0 { 1 } else { precision };
                Ok(float::format_general(
                    precision,
                    magnitude,
                    *case,
                    self.alternate_form,
                    false,
                ))
            }
            Some(FormatType::Exponent(case)) => Ok(float::format_exponent(
                precision,
                magnitude,
                *case,
                self.alternate_form,
            )),
            Some(FormatType::Percentage) => match magnitude {
                magnitude if magnitude.is_nan() => Ok("nan%".to_owned()),
                magnitude if magnitude.is_infinite() => Ok("inf%".to_owned()),
                _ => {
                    let result = format!("{:.*}", precision, magnitude * 100.0);
                    let point = float::decimal_point_or_empty(precision, self.alternate_form);
                    Ok(format!("{result}{point}%"))
                }
            },
            None => match magnitude {
                magnitude if magnitude.is_nan() => Ok("nan".to_owned()),
                magnitude if magnitude.is_infinite() => Ok("inf".to_owned()),
                _ => match self.precision {
                    Some(precision) => Ok(float::format_general(
                        precision,
                        magnitude,
                        Case::Lower,
                        self.alternate_form,
                        true,
                    )),
                    None => Ok(float::to_string(magnitude)),
                },
            },
        };
        let format_sign = self.sign.unwrap_or(FormatSign::Minus);
        let sign_str = if num.is_sign_negative() && !num.is_nan() {
            "-"
        } else {
            match format_sign {
                FormatSign::Plus => "+",
                FormatSign::Minus => "",
                FormatSign::MinusOrSpace => " ",
            }
        };
        let magnitude_str = self.add_magnitude_separators(raw_magnitude_str?, sign_str);
        Ok(
            self.format_sign_and_align(
                &AsciiStr::new(&magnitude_str),
                sign_str,
                FormatAlign::Right,
            ),
        )
    }

    #[inline]
    fn format_int_radix(&self, magnitude: &BigInt, radix: u32) -> Result<String, FormatSpecError> {
        match self.precision {
            Some(_) => Err(FormatSpecError::PrecisionNotAllowed),
            None => Ok(magnitude.to_str_radix(radix)),
        }
    }

    pub fn format_int(&self, num: &BigInt) -> Result<String, FormatSpecError> {
        self.validate_format(FormatType::Decimal)?;
        let magnitude = num.abs();
        let prefix = if self.alternate_form {
            match self.format_type {
                Some(FormatType::Binary) => "0b",
                Some(FormatType::Octal) => "0o",
                Some(FormatType::Hex(Case::Lower)) => "0x",
                Some(FormatType::Hex(Case::Upper)) => "0X",
                _ => "",
            }
        } else {
            ""
        };
        let raw_magnitude_str = match self.format_type {
            Some(FormatType::Binary) => self.format_int_radix(&magnitude, 2),
            Some(FormatType::Decimal) => self.format_int_radix(&magnitude, 10),
            Some(FormatType::Octal) => self.format_int_radix(&magnitude, 8),
            Some(FormatType::Hex(Case::Lower)) => self.format_int_radix(&magnitude, 16),
            Some(FormatType::Hex(Case::Upper)) => {
                if self.precision.is_some() {
                    Err(FormatSpecError::PrecisionNotAllowed)
                } else {
                    let mut result = magnitude.to_str_radix(16);
                    result.make_ascii_uppercase();
                    Ok(result)
                }
            }

            Some(FormatType::Number(Case::Lower)) => self.format_int_radix(&magnitude, 10),
            Some(FormatType::Number(Case::Upper)) => {
                Err(FormatSpecError::UnknownFormatCode('N', "int"))
            }
            Some(FormatType::String) => Err(FormatSpecError::UnknownFormatCode('s', "int")),
            Some(FormatType::Character) => match (self.sign, self.alternate_form) {
                (Some(_), _) => Err(FormatSpecError::NotAllowed("Sign")),
                (_, true) => Err(FormatSpecError::NotAllowed("Alternate form (#)")),
                (_, _) => match num.to_u32() {
                    Some(n) if n <= 0x0010_ffff => Ok(std::char::from_u32(n).unwrap().to_string()),
                    Some(_) | None => Err(FormatSpecError::CodeNotInRange),
                },
            },
            Some(
                FormatType::GeneralFormat(_)
                | FormatType::FixedPoint(_)
                | FormatType::Exponent(_)
                | FormatType::Percentage,
            ) => match num.to_f64() {
                Some(float) => return self.format_float(float),
                _ => Err(FormatSpecError::UnableToConvert),
            },
            None => self.format_int_radix(&magnitude, 10),
        }?;
        let format_sign = self.sign.unwrap_or(FormatSign::Minus);
        let sign_str = match num.sign() {
            Sign::Minus => "-",
            _ => match format_sign {
                FormatSign::Plus => "+",
                FormatSign::Minus => "",
                FormatSign::MinusOrSpace => " ",
            },
        };
        let sign_prefix = format!("{sign_str}{prefix}");
        let magnitude_str = self.add_magnitude_separators(raw_magnitude_str, &sign_prefix);
        Ok(self.format_sign_and_align(
            &AsciiStr::new(&magnitude_str),
            &sign_prefix,
            FormatAlign::Right,
        ))
    }

    pub fn format_string<T>(&self, s: &T) -> Result<String, FormatSpecError>
    where
        T: CharLen + Deref<Target = str>,
    {
        self.validate_format(FormatType::String)?;
        match self.format_type {
            Some(FormatType::String) | None => {
                let mut value = self.format_sign_and_align(s, "", FormatAlign::Left);
                if let Some(precision) = self.precision {
                    value.truncate(precision);
                }
                Ok(value)
            }
            _ => {
                let ch = char::from(self.format_type.as_ref().unwrap());
                Err(FormatSpecError::UnknownFormatCode(ch, "str"))
            }
        }
    }

    #[allow(clippy::cast_possible_wrap, clippy::cast_possible_truncation)]
    fn format_sign_and_align<T>(
        &self,
        magnitude_str: &T,
        sign_str: &str,
        default_align: FormatAlign,
    ) -> String
    where
        T: CharLen + Deref<Target = str>,
    {
        let align = self.align.unwrap_or(default_align);

        let num_chars = magnitude_str.char_len();
        let fill_char = self.fill.unwrap_or(' ');
        let fill_chars_needed: i32 = self.width.map_or(0, |w| {
            cmp::max(0, (w as i32) - (num_chars as i32) - (sign_str.len() as i32))
        });

        let magnitude_str = &**magnitude_str;
        match align {
            FormatAlign::Left => format!(
                "{}{}{}",
                sign_str,
                magnitude_str,
                FormatSpec::compute_fill_string(fill_char, fill_chars_needed)
            ),
            FormatAlign::Right => format!(
                "{}{}{}",
                FormatSpec::compute_fill_string(fill_char, fill_chars_needed),
                sign_str,
                magnitude_str
            ),
            FormatAlign::AfterSign => format!(
                "{}{}{}",
                sign_str,
                FormatSpec::compute_fill_string(fill_char, fill_chars_needed),
                magnitude_str
            ),
            FormatAlign::Center => {
                let left_fill_chars_needed = fill_chars_needed / 2;
                let right_fill_chars_needed = fill_chars_needed - left_fill_chars_needed;
                let left_fill_string =
                    FormatSpec::compute_fill_string(fill_char, left_fill_chars_needed);
                let right_fill_string =
                    FormatSpec::compute_fill_string(fill_char, right_fill_chars_needed);
                format!("{left_fill_string}{sign_str}{magnitude_str}{right_fill_string}")
            }
        }
    }
}

pub trait CharLen {
    /// Returns the number of characters in the text
    fn char_len(&self) -> usize;
}

struct AsciiStr<'a> {
    inner: &'a str,
}

impl<'a> AsciiStr<'a> {
    fn new(inner: &'a str) -> Self {
        Self { inner }
    }
}

impl CharLen for AsciiStr<'_> {
    fn char_len(&self) -> usize {
        self.inner.len()
    }
}

impl Deref for AsciiStr<'_> {
    type Target = str;
    fn deref(&self) -> &Self::Target {
        self.inner
    }
}

#[derive(Debug, PartialEq)]
pub enum FormatSpecError {
    DecimalDigitsTooMany,
    PrecisionTooBig,
    InvalidFormatSpecifier,
    UnspecifiedFormat(char, char),
    UnknownFormatCode(char, &'static str),
    PrecisionNotAllowed,
    NotAllowed(&'static str),
    UnableToConvert,
    CodeNotInRange,
    NotImplemented(char, &'static str),
}

#[derive(Debug, PartialEq)]
pub enum FormatParseError {
    UnmatchedBracket,
    MissingStartBracket,
    UnescapedStartBracketInLiteral,
    InvalidFormatSpecifier,
    UnknownConversion,
    EmptyAttribute,
    MissingRightBracket,
    InvalidCharacterAfterRightBracket,
}

impl FromStr for FormatSpec {
    type Err = FormatSpecError;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        FormatSpec::parse(s)
    }
}

#[derive(Debug, PartialEq)]
pub enum FieldNamePart {
    Attribute(String),
    Index(usize),
    StringIndex(String),
}

impl FieldNamePart {
    fn parse_part(
        chars: &mut impl PeekingNext<Item = char>,
    ) -> Result<Option<FieldNamePart>, FormatParseError> {
        chars
            .next()
            .map(|ch| match ch {
                '.' => {
                    let mut attribute = String::new();
                    for ch in chars.peeking_take_while(|ch| *ch != '.' && *ch != '[') {
                        attribute.push(ch);
                    }
                    if attribute.is_empty() {
                        Err(FormatParseError::EmptyAttribute)
                    } else {
                        Ok(FieldNamePart::Attribute(attribute))
                    }
                }
                '[' => {
                    let mut index = String::new();
                    for ch in chars {
                        if ch == ']' {
                            return if index.is_empty() {
                                Err(FormatParseError::EmptyAttribute)
                            } else if let Ok(index) = index.parse::<usize>() {
                                Ok(FieldNamePart::Index(index))
                            } else {
                                Ok(FieldNamePart::StringIndex(index))
                            };
                        }
                        index.push(ch);
                    }
                    Err(FormatParseError::MissingRightBracket)
                }
                _ => Err(FormatParseError::InvalidCharacterAfterRightBracket),
            })
            .transpose()
    }
}

#[derive(Debug, PartialEq)]
pub enum FieldType {
    Auto,
    Index(usize),
    Keyword(String),
}

#[derive(Debug, PartialEq)]
pub struct FieldName {
    pub field_type: FieldType,
    pub parts: Vec<FieldNamePart>,
}

impl FieldName {
    pub fn parse(text: &str) -> Result<FieldName, FormatParseError> {
        let mut chars = text.chars().peekable();
        let mut first = String::new();
        for ch in chars.peeking_take_while(|ch| *ch != '.' && *ch != '[') {
            first.push(ch);
        }

        let field_type = if first.is_empty() {
            FieldType::Auto
        } else if let Ok(index) = first.parse::<usize>() {
            FieldType::Index(index)
        } else {
            FieldType::Keyword(first)
        };

        let mut parts = Vec::new();
        while let Some(part) = FieldNamePart::parse_part(&mut chars)? {
            parts.push(part);
        }

        Ok(FieldName { field_type, parts })
    }
}

#[derive(Debug, PartialEq)]
pub enum FormatPart {
    Field {
        field_name: String,
        conversion_spec: Option<char>,
        format_spec: String,
    },
    Literal(String),
}

#[derive(Debug, PartialEq)]
pub struct FormatString {
    pub format_parts: Vec<FormatPart>,
}

impl FormatString {
    fn parse_literal_single(text: &str) -> Result<(char, &str), FormatParseError> {
        let mut chars = text.chars();
        // This should never be called with an empty str
        let first_char = chars.next().unwrap();
        // isn't this detectable only with bytes operation?
        if first_char == '{' || first_char == '}' {
            let maybe_next_char = chars.next();
            // if we see a bracket, it has to be escaped by doubling up to be in a literal
            return if maybe_next_char.is_none() || maybe_next_char.unwrap() != first_char {
                Err(FormatParseError::UnescapedStartBracketInLiteral)
            } else {
                Ok((first_char, chars.as_str()))
            };
        }
        Ok((first_char, chars.as_str()))
    }

    fn parse_literal(text: &str) -> Result<(FormatPart, &str), FormatParseError> {
        let mut cur_text = text;
        let mut result_string = String::new();
        while !cur_text.is_empty() {
            match FormatString::parse_literal_single(cur_text) {
                Ok((next_char, remaining)) => {
                    result_string.push(next_char);
                    cur_text = remaining;
                }
                Err(err) => {
                    return if result_string.is_empty() {
                        Err(err)
                    } else {
                        Ok((FormatPart::Literal(result_string), cur_text))
                    };
                }
            }
        }
        Ok((FormatPart::Literal(result_string), ""))
    }

    fn parse_part_in_brackets(text: &str) -> Result<FormatPart, FormatParseError> {
        let parts: Vec<&str> = text.splitn(2, ':').collect();
        // before the comma is a keyword or arg index, after the comma is maybe a spec.
        let arg_part = parts[0];

        let format_spec = if parts.len() > 1 {
            parts[1].to_owned()
        } else {
            String::new()
        };

        // On parts[0] can still be the conversion (!r, !s, !a)
        let parts: Vec<&str> = arg_part.splitn(2, '!').collect();
        // before the bang is a keyword or arg index, after the comma is maybe a conversion spec.
        let arg_part = parts[0];

        let conversion_spec = parts
            .get(1)
            .map(|conversion| {
                // conversions are only every one character
                conversion
                    .chars()
                    .exactly_one()
                    .map_err(|_| FormatParseError::UnknownConversion)
            })
            .transpose()?;

        Ok(FormatPart::Field {
            field_name: arg_part.to_owned(),
            conversion_spec,
            format_spec,
        })
    }

    fn parse_spec(text: &str) -> Result<(FormatPart, &str), FormatParseError> {
        let mut nested = false;
        let mut end_bracket_pos = None;
        let mut left = String::new();

        // There may be one layer nesting brackets in spec
        for (idx, c) in text.char_indices() {
            if idx == 0 {
                if c != '{' {
                    return Err(FormatParseError::MissingStartBracket);
                }
            } else if c == '{' {
                if nested {
                    return Err(FormatParseError::InvalidFormatSpecifier);
                }
                nested = true;
                left.push(c);
                continue;
            } else if c == '}' {
                if nested {
                    nested = false;
                    left.push(c);
                    continue;
                }
                end_bracket_pos = Some(idx);
                break;
            } else {
                left.push(c);
            }
        }
        if let Some(pos) = end_bracket_pos {
            let (_, right) = text.split_at(pos);
            let format_part = FormatString::parse_part_in_brackets(&left)?;
            Ok((format_part, &right[1..]))
        } else {
            Err(FormatParseError::UnmatchedBracket)
        }
    }
}

pub trait FromTemplate<'a>: Sized {
    type Err;
    fn from_str(s: &'a str) -> Result<Self, Self::Err>;
}

impl<'a> FromTemplate<'a> for FormatString {
    type Err = FormatParseError;

    fn from_str(text: &'a str) -> Result<Self, Self::Err> {
        let mut cur_text: &str = text;
        let mut parts: Vec<FormatPart> = Vec::new();
        while !cur_text.is_empty() {
            // Try to parse both literals and bracketed format parts until we
            // run out of text
            cur_text = FormatString::parse_literal(cur_text)
                .or_else(|_| FormatString::parse_spec(cur_text))
                .map(|(part, new_text)| {
                    parts.push(part);
                    new_text
                })?;
        }
        Ok(FormatString {
            format_parts: parts,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_fill_and_align() {
        assert_eq!(
            parse_fill_and_align(" <"),
            (Some(' '), Some(FormatAlign::Left), "")
        );
        assert_eq!(
            parse_fill_and_align(" <22"),
            (Some(' '), Some(FormatAlign::Left), "22")
        );
        assert_eq!(
            parse_fill_and_align("<22"),
            (None, Some(FormatAlign::Left), "22")
        );
        assert_eq!(
            parse_fill_and_align(" ^^"),
            (Some(' '), Some(FormatAlign::Center), "^")
        );
        assert_eq!(
            parse_fill_and_align("==="),
            (Some('='), Some(FormatAlign::AfterSign), "=")
        );
    }

    #[test]
    fn test_width_only() {
        let expected = Ok(FormatSpec {
            conversion: None,
            fill: None,
            align: None,
            sign: None,
            alternate_form: false,
            width: Some(33),
            grouping_option: None,
            precision: None,
            format_type: None,
        });
        assert_eq!(FormatSpec::parse("33"), expected);
    }

    #[test]
    fn test_fill_and_width() {
        let expected = Ok(FormatSpec {
            conversion: None,
            fill: Some('<'),
            align: Some(FormatAlign::Right),
            sign: None,
            alternate_form: false,
            width: Some(33),
            grouping_option: None,
            precision: None,
            format_type: None,
        });
        assert_eq!(FormatSpec::parse("<>33"), expected);
    }

    #[test]
    fn test_all() {
        let expected = Ok(FormatSpec {
            conversion: None,
            fill: Some('<'),
            align: Some(FormatAlign::Right),
            sign: Some(FormatSign::Minus),
            alternate_form: true,
            width: Some(23),
            grouping_option: Some(FormatGrouping::Comma),
            precision: Some(11),
            format_type: Some(FormatType::Binary),
        });
        assert_eq!(FormatSpec::parse("<>-#23,.11b"), expected);
    }

    fn format_bool(text: &str, value: bool) -> Result<String, FormatSpecError> {
        FormatSpec::parse(text).and_then(|spec| spec.format_bool(value))
    }

    #[test]
    fn test_format_bool() {
        assert_eq!(format_bool("b", true), Ok("1".to_owned()));
        assert_eq!(format_bool("b", false), Ok("0".to_owned()));
        assert_eq!(format_bool("d", true), Ok("1".to_owned()));
        assert_eq!(format_bool("d", false), Ok("0".to_owned()));
        assert_eq!(format_bool("o", true), Ok("1".to_owned()));
        assert_eq!(format_bool("o", false), Ok("0".to_owned()));
        assert_eq!(format_bool("n", true), Ok("1".to_owned()));
        assert_eq!(format_bool("n", false), Ok("0".to_owned()));
        assert_eq!(format_bool("x", true), Ok("1".to_owned()));
        assert_eq!(format_bool("x", false), Ok("0".to_owned()));
        assert_eq!(format_bool("X", true), Ok("1".to_owned()));
        assert_eq!(format_bool("X", false), Ok("0".to_owned()));
        assert_eq!(format_bool("g", true), Ok("1".to_owned()));
        assert_eq!(format_bool("g", false), Ok("0".to_owned()));
        assert_eq!(format_bool("G", true), Ok("1".to_owned()));
        assert_eq!(format_bool("G", false), Ok("0".to_owned()));
        assert_eq!(format_bool("c", true), Ok("\x01".to_owned()));
        assert_eq!(format_bool("c", false), Ok("\x00".to_owned()));
        assert_eq!(format_bool("e", true), Ok("1.000000e+00".to_owned()));
        assert_eq!(format_bool("e", false), Ok("0.000000e+00".to_owned()));
        assert_eq!(format_bool("E", true), Ok("1.000000E+00".to_owned()));
        assert_eq!(format_bool("E", false), Ok("0.000000E+00".to_owned()));
        assert_eq!(format_bool("f", true), Ok("1.000000".to_owned()));
        assert_eq!(format_bool("f", false), Ok("0.000000".to_owned()));
        assert_eq!(format_bool("F", true), Ok("1.000000".to_owned()));
        assert_eq!(format_bool("F", false), Ok("0.000000".to_owned()));
        assert_eq!(format_bool("%", true), Ok("100.000000%".to_owned()));
        assert_eq!(format_bool("%", false), Ok("0.000000%".to_owned()));
    }

    #[test]
    fn test_format_int() {
        assert_eq!(
            FormatSpec::parse("d")
                .unwrap()
                .format_int(&BigInt::from_bytes_be(Sign::Plus, b"\x10")),
            Ok("16".to_owned())
        );
        assert_eq!(
            FormatSpec::parse("x")
                .unwrap()
                .format_int(&BigInt::from_bytes_be(Sign::Plus, b"\x10")),
            Ok("10".to_owned())
        );
        assert_eq!(
            FormatSpec::parse("b")
                .unwrap()
                .format_int(&BigInt::from_bytes_be(Sign::Plus, b"\x10")),
            Ok("10000".to_owned())
        );
        assert_eq!(
            FormatSpec::parse("o")
                .unwrap()
                .format_int(&BigInt::from_bytes_be(Sign::Plus, b"\x10")),
            Ok("20".to_owned())
        );
        assert_eq!(
            FormatSpec::parse("+d")
                .unwrap()
                .format_int(&BigInt::from_bytes_be(Sign::Plus, b"\x10")),
            Ok("+16".to_owned())
        );
        assert_eq!(
            FormatSpec::parse("^ 5d")
                .unwrap()
                .format_int(&BigInt::from_bytes_be(Sign::Minus, b"\x10")),
            Ok(" -16 ".to_owned())
        );
        assert_eq!(
            FormatSpec::parse("0>+#10x")
                .unwrap()
                .format_int(&BigInt::from_bytes_be(Sign::Plus, b"\x10")),
            Ok("00000+0x10".to_owned())
        );
    }

    #[test]
    fn test_format_int_sep() {
        let spec = FormatSpec::parse(",").expect("");
        assert_eq!(spec.grouping_option, Some(FormatGrouping::Comma));
        assert_eq!(
            spec.format_int(&BigInt::from_str("1234567890123456789012345678").unwrap()),
            Ok("1,234,567,890,123,456,789,012,345,678".to_owned())
        );
    }

    #[test]
    fn test_format_parse() {
        let expected = Ok(FormatString {
            format_parts: vec![
                FormatPart::Literal("abcd".to_owned()),
                FormatPart::Field {
                    field_name: "1".to_owned(),
                    conversion_spec: None,
                    format_spec: String::new(),
                },
                FormatPart::Literal(":".to_owned()),
                FormatPart::Field {
                    field_name: "key".to_owned(),
                    conversion_spec: None,
                    format_spec: String::new(),
                },
            ],
        });

        assert_eq!(FormatString::from_str("abcd{1}:{key}"), expected);
    }

    #[test]
    fn test_format_parse_multi_byte_char() {
        assert!(FormatString::from_str("{a:%ЫйЯЧ}").is_ok());
    }

    #[test]
    fn test_format_parse_fail() {
        assert_eq!(
            FormatString::from_str("{s"),
            Err(FormatParseError::UnmatchedBracket)
        );
    }

    #[test]
    fn test_format_parse_escape() {
        let expected = Ok(FormatString {
            format_parts: vec![
                FormatPart::Literal("{".to_owned()),
                FormatPart::Field {
                    field_name: "key".to_owned(),
                    conversion_spec: None,
                    format_spec: String::new(),
                },
                FormatPart::Literal("}ddfe".to_owned()),
            ],
        });

        assert_eq!(FormatString::from_str("{{{key}}}ddfe"), expected);
    }

    #[test]
    fn test_format_invalid_specification() {
        assert_eq!(
            FormatSpec::parse("%3"),
            Err(FormatSpecError::InvalidFormatSpecifier)
        );
        assert_eq!(
            FormatSpec::parse(".2fa"),
            Err(FormatSpecError::InvalidFormatSpecifier)
        );
        assert_eq!(
            FormatSpec::parse("ds"),
            Err(FormatSpecError::InvalidFormatSpecifier)
        );
        assert_eq!(
            FormatSpec::parse("x+"),
            Err(FormatSpecError::InvalidFormatSpecifier)
        );
        assert_eq!(
            FormatSpec::parse("b4"),
            Err(FormatSpecError::InvalidFormatSpecifier)
        );
        assert_eq!(
            FormatSpec::parse("o!"),
            Err(FormatSpecError::InvalidFormatSpecifier)
        );
        assert_eq!(
            FormatSpec::parse("d "),
            Err(FormatSpecError::InvalidFormatSpecifier)
        );
    }

    #[test]
    fn test_parse_field_name() {
        assert_eq!(
            FieldName::parse(""),
            Ok(FieldName {
                field_type: FieldType::Auto,
                parts: Vec::new(),
            })
        );
        assert_eq!(
            FieldName::parse("0"),
            Ok(FieldName {
                field_type: FieldType::Index(0),
                parts: Vec::new(),
            })
        );
        assert_eq!(
            FieldName::parse("key"),
            Ok(FieldName {
                field_type: FieldType::Keyword("key".to_owned()),
                parts: Vec::new(),
            })
        );
        assert_eq!(
            FieldName::parse("key.attr[0][string]"),
            Ok(FieldName {
                field_type: FieldType::Keyword("key".to_owned()),
                parts: vec![
                    FieldNamePart::Attribute("attr".to_owned()),
                    FieldNamePart::Index(0),
                    FieldNamePart::StringIndex("string".to_owned())
                ],
            })
        );
        assert_eq!(
            FieldName::parse("key.."),
            Err(FormatParseError::EmptyAttribute)
        );
        assert_eq!(
            FieldName::parse("key[]"),
            Err(FormatParseError::EmptyAttribute)
        );
        assert_eq!(
            FieldName::parse("key["),
            Err(FormatParseError::MissingRightBracket)
        );
        assert_eq!(
            FieldName::parse("key[0]after"),
            Err(FormatParseError::InvalidCharacterAfterRightBracket)
        );
    }
}
