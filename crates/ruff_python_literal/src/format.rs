use itertools::{Itertools, PeekingNext};

use std::error::Error;
use std::str::FromStr;

use crate::Case;

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
            Some(_) => (None, chars.as_str()),
            _ => (None, text),
        }
    }
}

/// The format specification component of a format field
///
/// For example the content would be parsed from `<20` in:
/// ```python
/// "hello {name:<20}".format(name="test")
/// ```
///
/// Format specifications allow nested placeholders for dynamic formatting.
/// For example, the following statements are equivalent:
/// ```python
/// "hello {name:{fmt}}".format(name="test", fmt="<20")
/// "hello {name:{align}{width}}".format(name="test", align="<", width="20")
/// "hello {name:<20{empty}>}".format(name="test", empty="")
/// ```
///
/// Nested placeholders can include additional format specifiers.
/// ```python
/// "hello {name:{fmt:*>}}".format(name="test", fmt="<20")
/// ```
///
/// However, placeholders can only be singly nested (preserving our sanity).
/// A [`FormatSpecError::PlaceholderRecursionExceeded`] will be raised while parsing in this case.
/// ```python
/// "hello {name:{fmt:{not_allowed}}}".format(name="test", fmt="<20")  # Syntax error
/// ```
///
/// When placeholders are present in a format specification, parsing will return a [`DynamicFormatSpec`]
/// and avoid attempting to parse any of the clauses. Otherwise, a [`StaticFormatSpec`] will be used.
#[derive(Debug, PartialEq)]
pub enum FormatSpec {
    Static(StaticFormatSpec),
    Dynamic(DynamicFormatSpec),
}

#[derive(Debug, PartialEq)]
pub struct StaticFormatSpec {
    // Ex) `!s` in `'{!s}'`
    conversion: Option<FormatConversion>,
    // Ex) `*` in `'{:*^30}'`
    fill: Option<char>,
    // Ex) `<` in `'{:<30}'`
    align: Option<FormatAlign>,
    // Ex) `+` in `'{:+f}'`
    sign: Option<FormatSign>,
    // Ex) `#` in `'{:#x}'`
    alternate_form: bool,
    // Ex) `30` in `'{:<30}'`
    width: Option<usize>,
    // Ex) `,` in `'{:,}'`
    grouping_option: Option<FormatGrouping>,
    // Ex) `2` in `'{:.2}'`
    precision: Option<usize>,
    // Ex) `f` in `'{:+f}'`
    format_type: Option<FormatType>,
}

#[derive(Debug, PartialEq)]
pub struct DynamicFormatSpec {
    // Ex) `x` and `y` in `'{:*{x},{y}b}'`
    pub placeholders: Vec<FormatPart>,
}

#[derive(Copy, Clone, Debug, PartialEq, Default)]
pub enum AllowPlaceholderNesting {
    #[default]
    Yes,
    No,
    AllowPlaceholderNesting,
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

/// Parses a placeholder format part within a format specification
fn parse_nested_placeholder(text: &str) -> Result<Option<(FormatPart, &str)>, FormatSpecError> {
    match FormatString::parse_spec(text, AllowPlaceholderNesting::No) {
        // Not a nested placeholder, OK
        Err(FormatParseError::MissingStartBracket) => Ok(None),
        Err(err) => Err(FormatSpecError::InvalidPlaceholder(err)),
        Ok((format_part, text)) => Ok(Some((format_part, text))),
    }
}

/// Parse all placeholders in a format specification
/// If no placeholders are present, an empty vector will be returned
fn parse_nested_placeholders(mut text: &str) -> Result<Vec<FormatPart>, FormatSpecError> {
    let mut placeholders = vec![];
    while let Some(bracket) = text.find('{') {
        if let Some((format_part, rest)) = parse_nested_placeholder(&text[bracket..])? {
            text = rest;
            placeholders.push(format_part);
        } else {
            text = &text[bracket + 1..];
        }
    }
    Ok(placeholders)
}

impl FormatSpec {
    pub fn parse(text: &str) -> Result<Self, FormatSpecError> {
        let placeholders = parse_nested_placeholders(text)?;
        if !placeholders.is_empty() {
            return Ok(FormatSpec::Dynamic(DynamicFormatSpec { placeholders }));
        }

        let (conversion, text) = FormatConversion::parse(text);
        let (mut fill, mut align, text) = parse_fill_and_align(text);
        let (sign, text) = FormatSign::parse(text);
        let (alternate_form, text) = parse_alternate_form(text);
        let (zero, text) = parse_zero(text);
        let (width, text) = parse_number(text)?;
        let (grouping_option, text) = FormatGrouping::parse(text);
        let (precision, text) = parse_precision(text)?;

        let (format_type, _text) = if text.is_empty() {
            (None, text)
        } else {
            // If there's any remaining text, we should yield a valid format type and consume it
            // all.
            let (format_type, text) = FormatType::parse(text);
            if format_type.is_none() {
                return Err(FormatSpecError::InvalidFormatType);
            }
            if !text.is_empty() {
                return Err(FormatSpecError::InvalidFormatSpecifier);
            }
            (format_type, text)
        };

        if zero && fill.is_none() {
            fill.replace('0');
            align = align.or(Some(FormatAlign::AfterSign));
        }

        Ok(FormatSpec::Static(StaticFormatSpec {
            conversion,
            fill,
            align,
            sign,
            alternate_form,
            width,
            grouping_option,
            precision,
            format_type,
        }))
    }
}

#[derive(Debug, PartialEq)]
pub enum FormatSpecError {
    DecimalDigitsTooMany,
    PrecisionTooBig,
    InvalidFormatSpecifier,
    InvalidFormatType,
    InvalidPlaceholder(FormatParseError),
    PlaceholderRecursionExceeded,
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
    PlaceholderRecursionExceeded,
    UnknownConversion,
    EmptyAttribute,
    MissingRightBracket,
    InvalidCharacterAfterRightBracket,
}

impl std::fmt::Display for FormatParseError {
    fn fmt(&self, fmt: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnmatchedBracket => {
                std::write!(fmt, "unmatched bracket in format string")
            }
            Self::MissingStartBracket => {
                std::write!(fmt, "missing start bracket in format string")
            }
            Self::UnescapedStartBracketInLiteral => {
                std::write!(fmt, "unescaped start bracket in literal")
            }
            Self::PlaceholderRecursionExceeded => {
                std::write!(fmt, "multiply nested placeholder not allowed")
            }
            Self::UnknownConversion => {
                std::write!(fmt, "unknown conversion")
            }
            Self::EmptyAttribute => {
                std::write!(fmt, "empty attribute")
            }
            Self::MissingRightBracket => {
                std::write!(fmt, "missing right bracket")
            }
            Self::InvalidCharacterAfterRightBracket => {
                std::write!(fmt, "invalid character after right bracket")
            }
        }
    }
}

impl Error for FormatParseError {}

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

    fn parse_spec(
        text: &str,
        allow_nesting: AllowPlaceholderNesting,
    ) -> Result<(FormatPart, &str), FormatParseError> {
        let Some(text) = text.strip_prefix('{') else {
            return Err(FormatParseError::MissingStartBracket);
        };

        let mut nested = false;
        let mut left = String::new();

        for (idx, c) in text.char_indices() {
            if c == '{' {
                // There may be one layer nesting brackets in spec
                if nested || allow_nesting == AllowPlaceholderNesting::No {
                    return Err(FormatParseError::PlaceholderRecursionExceeded);
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
                let (_, right) = text.split_at(idx + 1);
                let format_part = FormatString::parse_part_in_brackets(&left)?;
                return Ok((format_part, right));
            }
            left.push(c);
        }
        Err(FormatParseError::UnmatchedBracket)
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
                .or_else(|_| FormatString::parse_spec(cur_text, AllowPlaceholderNesting::Yes))
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
        let expected = Ok(FormatSpec::Static(StaticFormatSpec {
            conversion: None,
            fill: None,
            align: None,
            sign: None,
            alternate_form: false,
            width: Some(33),
            grouping_option: None,
            precision: None,
            format_type: None,
        }));
        assert_eq!(FormatSpec::parse("33"), expected);
    }

    #[test]
    fn test_fill_and_width() {
        let expected = Ok(FormatSpec::Static(StaticFormatSpec {
            conversion: None,
            fill: Some('<'),
            align: Some(FormatAlign::Right),
            sign: None,
            alternate_form: false,
            width: Some(33),
            grouping_option: None,
            precision: None,
            format_type: None,
        }));
        assert_eq!(FormatSpec::parse("<>33"), expected);
    }

    #[test]
    fn test_format_part() {
        let expected = Ok(FormatSpec::Dynamic(DynamicFormatSpec {
            placeholders: vec![FormatPart::Field {
                field_name: "x".to_string(),
                conversion_spec: None,
                format_spec: String::new(),
            }],
        }));
        assert_eq!(FormatSpec::parse("{x}"), expected);
    }

    #[test]
    fn test_dynamic_format_spec() {
        let expected = Ok(FormatSpec::Dynamic(DynamicFormatSpec {
            placeholders: vec![
                FormatPart::Field {
                    field_name: "x".to_string(),
                    conversion_spec: None,
                    format_spec: String::new(),
                },
                FormatPart::Field {
                    field_name: "y".to_string(),
                    conversion_spec: None,
                    format_spec: "<2".to_string(),
                },
                FormatPart::Field {
                    field_name: "z".to_string(),
                    conversion_spec: None,
                    format_spec: String::new(),
                },
            ],
        }));
        assert_eq!(FormatSpec::parse("{x}{y:<2}{z}"), expected);
    }

    #[test]
    fn test_dynamic_format_spec_with_others() {
        let expected = Ok(FormatSpec::Dynamic(DynamicFormatSpec {
            placeholders: vec![FormatPart::Field {
                field_name: "x".to_string(),
                conversion_spec: None,
                format_spec: String::new(),
            }],
        }));
        assert_eq!(FormatSpec::parse("<{x}20b"), expected);
    }

    #[test]
    fn test_all() {
        let expected = Ok(FormatSpec::Static(StaticFormatSpec {
            conversion: None,
            fill: Some('<'),
            align: Some(FormatAlign::Right),
            sign: Some(FormatSign::Minus),
            alternate_form: true,
            width: Some(23),
            grouping_option: Some(FormatGrouping::Comma),
            precision: Some(11),
            format_type: Some(FormatType::Binary),
        }));
        assert_eq!(FormatSpec::parse("<>-#23,.11b"), expected);
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
    fn test_format_parse_nested_placeholder() {
        let expected = Ok(FormatString {
            format_parts: vec![
                FormatPart::Literal("abcd".to_owned()),
                FormatPart::Field {
                    field_name: "1".to_owned(),
                    conversion_spec: None,
                    format_spec: "{a}".to_owned(),
                },
            ],
        });

        assert_eq!(FormatString::from_str("abcd{1:{a}}"), expected);
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
            FormatSpec::parse("{"),
            Err(FormatSpecError::InvalidPlaceholder(
                FormatParseError::UnmatchedBracket
            ))
        );
        assert_eq!(
            FormatSpec::parse("{x"),
            Err(FormatSpecError::InvalidPlaceholder(
                FormatParseError::UnmatchedBracket
            ))
        );
        assert_eq!(
            FormatSpec::parse("}"),
            Err(FormatSpecError::InvalidFormatType)
        );
        assert_eq!(
            FormatSpec::parse("{}}"),
            // Note this should be an `InvalidFormatType` but we give up
            // on all other parsing validation when we see a placeholder
            Ok(FormatSpec::Dynamic(DynamicFormatSpec {
                placeholders: vec![FormatPart::Field {
                    field_name: String::new(),
                    conversion_spec: None,
                    format_spec: String::new()
                }]
            }))
        );
        assert_eq!(
            FormatSpec::parse("{{x}}"),
            Err(FormatSpecError::InvalidPlaceholder(
                FormatParseError::PlaceholderRecursionExceeded
            ))
        );
        assert_eq!(
            FormatSpec::parse("d "),
            Err(FormatSpecError::InvalidFormatSpecifier)
        );
        assert_eq!(
            FormatSpec::parse("z"),
            Err(FormatSpecError::InvalidFormatType)
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
