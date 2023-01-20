use std::str::FromStr;

use anyhow::{anyhow, Result};
use once_cell::sync::Lazy;
use regex::Regex;
use rustpython_common::cformat::{
    CConversionFlags, CFormatPart, CFormatPrecision, CFormatQuantity, CFormatSpec, CFormatString,
};
use rustpython_parser::ast::{Constant, Expr, ExprKind};

use crate::ast::types::Range;
use crate::ast::whitespace::indentation;
use crate::checkers::ast::Checker;
use crate::fix::Fix;
use crate::registry::{Diagnostic, Rule};
use crate::rules::pyupgrade::helpers::{curly_escape, is_keyword};
use crate::violations;

static MODULO_CALL: Lazy<Regex> = Lazy::new(|| Regex::new(r" % ([({])").unwrap());
static PYTHON_NAME: Lazy<Regex> = Lazy::new(|| Regex::new(r"[^\W0-9]\w*").unwrap());

#[derive(Debug, PartialEq, Clone)]
struct PercentFormatPart {
    key: Option<String>,
    conversion_flag: Option<String>,
    width: Option<String>,
    precision: Option<String>,
    conversion: String,
}

// Can we just remove this and use
impl PercentFormatPart {
    fn new(
        key: Option<String>,
        conversion_flag: Option<String>,
        width: Option<String>,
        precision: Option<String>,
        conversion: String,
    ) -> Self {
        Self {
            key,
            conversion_flag,
            width,
            precision,
            conversion,
        }
    }
}

impl From<&CFormatSpec> for PercentFormatPart {
    fn from(spec: &CFormatSpec) -> Self {
        let clean_width = match &spec.min_field_width {
            Some(width_item) => match width_item {
                CFormatQuantity::Amount(amount) => Some(amount.to_string()),
                CFormatQuantity::FromValuesTuple => Some("*".to_string()),
            },
            None => None,
        };
        let clean_precision = match &spec.precision {
            Some(CFormatPrecision::Quantity(quantity)) => match quantity {
                CFormatQuantity::Amount(amount) => Some(format!(".{amount}")),
                CFormatQuantity::FromValuesTuple => Some(".*".to_string()),
            },
            Some(CFormatPrecision::Dot) => Some(".".to_string()),
            None => None,
        };
        let flags = if spec.flags.is_empty() {
            None
        } else {
            Some(parse_conversion_flags(spec.flags))
        };
        Self::new(
            spec.mapping_key.clone(),
            flags,
            clean_width,
            clean_precision,
            spec.format_char.to_string(),
        )
    }
}

#[derive(Debug, PartialEq, Clone)]
struct PercentFormat {
    item: String,
    format_spec: Option<PercentFormatPart>,
}

impl PercentFormat {
    fn new(item: String, parts: Option<PercentFormatPart>) -> Self {
        Self {
            item,
            format_spec: parts,
        }
    }
}

/// Converts the `RustPython` Conversion Flags into their Python string
/// representation.
fn parse_conversion_flags(flags: CConversionFlags) -> String {
    let mut flag_string = String::new();
    if flags.contains(CConversionFlags::ALTERNATE_FORM) {
        flag_string.push('#');
    }
    if flags.contains(CConversionFlags::ZERO_PAD) {
        flag_string.push('0');
    }
    if flags.contains(CConversionFlags::LEFT_ADJUST) {
        flag_string.push('-');
    }
    if flags.contains(CConversionFlags::BLANK_SIGN) {
        flag_string.push(' ');
    }
    if flags.contains(CConversionFlags::SIGN_CHAR) {
        flag_string.push('+');
    }
    flag_string
}

/// Converts a string to a vector of [`PercentFormat`] structs.
fn parse_percent_format(string: &str) -> Result<Vec<PercentFormat>> {
    let format_string =
        CFormatString::from_str(string).map_err(|_| anyhow!("Failed to parse CFormatString"))?;
    let format_parts: Vec<&CFormatPart<String>> =
        format_string.iter().map(|(_, part)| part).collect();
    let mut formats: Vec<PercentFormat> = vec![];
    for (i, part) in format_parts.iter().enumerate() {
        if let CFormatPart::Literal(item) = &part {
            let mut current_format = PercentFormat::new(item.to_string(), None);
            let Some(format_part) = format_parts.get(i + 1) else {
                formats.push(current_format);
                continue;
            };
            if let CFormatPart::Spec(c_spec) = &format_part {
                current_format.format_spec = Some(c_spec.into());
            }
            formats.push(current_format);
        }
    }
    Ok(formats)
}

/// Removes the first instance of a given element from a vector, if the item is
/// not in the vector, nothing happens
fn remove<T: PartialEq + Copy>(vec: &mut Vec<T>, item: T) {
    if let Some(index) = vec.iter().position(|&x| x == item) {
        vec.remove(index);
    }
}

fn simplify_conversion_flag(flag: &str) -> String {
    let mut parts: Vec<char> = vec![];
    for mut character in flag.chars() {
        if parts.contains(&character) {
            continue;
        }
        if character == '-' {
            character = '<';
        }
        parts.push(character);
        if character == '<' && parts.contains(&'0') {
            remove(&mut parts, '0');
        } else if character == '+' && parts.contains(&' ') {
            remove(&mut parts, ' ');
        }
    }
    String::from_iter(parts)
}

/// Returns true if any of `conversion_flag`, `width`, `precision`, and
/// conversion are a non-empty string
fn any_percent_format(pf: &PercentFormatPart) -> bool {
    let mut cf_bool = false;
    let mut w_bool = false;
    let mut precision_bool = false;
    let conversion_bool = !pf.conversion.is_empty();
    if let Some(conversion_flag) = &pf.conversion_flag {
        cf_bool = !conversion_flag.is_empty();
    }
    if let Some(width) = &pf.width {
        w_bool = !width.is_empty();
    }
    if let Some(precision) = &pf.precision {
        precision_bool = !precision.is_empty();
    }
    cf_bool || w_bool || precision_bool || conversion_bool
}

/// Convert a [`PercentFormat`] struct into a `String`.
fn handle_part(part: &PercentFormat) -> String {
    let mut string = part.item.clone();
    string = curly_escape(&string);
    let Some(mut fmt) = part.format_spec.clone() else {
        return string;
    };
    if fmt.conversion == *"%" {
        string.push('%');
        return string;
    }
    let mut parts = vec![string, "{".to_string()];
    if fmt.conversion == *"s" {
        fmt.conversion = String::new();
    }
    if let Some(key_item) = &fmt.key {
        parts.push(key_item.to_string());
    }
    let converter: String;
    if fmt.conversion == *"r" || fmt.conversion == *"a" {
        converter = format!("!{}", fmt.conversion);
        fmt.conversion = String::new();
    } else {
        converter = String::new();
    }
    if any_percent_format(&fmt) {
        parts.push(":".to_string());
        if let Some(conversion_flag) = &fmt.conversion_flag {
            if !conversion_flag.is_empty() {
                let simplified = simplify_conversion_flag(conversion_flag);
                parts.push(simplified);
            }
        }
        if let Some(width) = &fmt.width {
            if !width.is_empty() {
                parts.push(width.to_string());
            }
        }
        if let Some(precision) = &fmt.precision {
            if !precision.is_empty() {
                parts.push(precision.to_string());
            }
        }
    }
    if !fmt.conversion.is_empty() {
        parts.push(fmt.conversion);
    }
    parts.push(converter);
    parts.push("}".to_string());
    String::from_iter(parts)
}

/// Convert a sequence of [`PercentFormat`] structs into a `String`.
fn percent_to_format(parsed: &[PercentFormat]) -> String {
    let mut contents = String::new();
    for part in parsed {
        contents.push_str(&handle_part(part));
    }
    contents
}

/// If a tuple has one argument, remove the comma; otherwise, return it as-is.
fn clean_params_tuple(checker: &mut Checker, right: &Expr) -> String {
    let mut base_string = checker
        .locator
        .slice_source_code_range(&Range::from_located(right))
        .to_string();
    if let ExprKind::Tuple { elts, .. } = &right.node {
        if elts.len() == 1 {
            if right.location.row() == right.end_location.unwrap().row() {
                for (i, character) in base_string.chars().rev().enumerate() {
                    if character == ',' {
                        let correct_index = base_string.len() - i - 1;
                        base_string.remove(correct_index);
                        break;
                    }
                }
            }
        }
    }
    base_string
}

/// Converts a dictionary to a function call while preserving as much styling as
/// possible.
fn clean_params_dictionary(checker: &mut Checker, right: &Expr) -> Option<String> {
    let is_multi_line = right.location.row() < right.end_location.unwrap().row();
    let mut new_string = String::new();
    if let ExprKind::Dict { keys, values } = &right.node {
        let mut new_vals: Vec<String> = vec![];
        let mut indent = None;
        let mut already_seen: Vec<String> = vec![];
        for (key, value) in keys.iter().zip(values.iter()) {
            // The original unit tests of pyupgrade reveal that we should not rewrite
            // non-string keys
            if let ExprKind::Constant {
                value: Constant::Str(key_string),
                ..
            } = &key.node
            {
                // If the dictionary key is not a valid variable name, abort.
                if !PYTHON_NAME.is_match(key_string) {
                    return None;
                }
                // If the key is a Python keyword, abort.
                if is_keyword(key_string) {
                    return None;
                }
                // If there are multiple entries of the same key, abort.
                if already_seen.contains(key_string) {
                    return None;
                }
                already_seen.push(key_string.clone());
                let mut new_string = String::new();
                if is_multi_line {
                    if indent.is_none() {
                        indent = indentation(checker.locator, key);
                    }
                }
                let value_range = Range::new(value.location, value.end_location.unwrap());
                let value_string = checker.locator.slice_source_code_range(&value_range);
                new_string.push_str(key_string);
                new_string.push('=');
                new_string.push_str(&value_string);
                new_vals.push(new_string);
            } else {
                // If there are any non-string keys, abort.
                return None;
            }
        }
        // If we couldn't parse out key values, abort.
        if new_vals.is_empty() {
            return None;
        }
        new_string.push('(');
        if is_multi_line {
            // If this is a multi-line dictionary, abort.
            let Some(indent) = indent else {
                println!(
                    "{:?}",
                    checker.locator.slice_source_code_range(&Range::new(
                        right.location,
                        right.end_location.unwrap()
                    ))
                );
                return None;
            };

            for item in &new_vals {
                new_string.push('\n');
                new_string.push_str(&indent);
                new_string.push_str(item);
                new_string.push(',');
            }
            // For the ending parentheses we want to go back one indent.
            new_string.push('\n');
            if indent.len() > 3 {
                new_string.push_str(&indent[3..]);
            }
        } else {
            new_string.push_str(&new_vals.join(", "));
        }
        new_string.push(')');
    }
    Some(new_string)
}

fn fix_percent_format_tuple(
    checker: &mut Checker,
    params: &Expr,
    parsed: &[PercentFormat],
) -> String {
    let mut contents = percent_to_format(parsed);
    contents.push_str(".format");
    let params_string = clean_params_tuple(checker, params);
    contents.push_str(&params_string);
    contents
}

fn fix_percent_format_dict(
    checker: &mut Checker,
    params: &Expr,
    parsed: &[PercentFormat],
) -> Option<String> {
    let mut contents = percent_to_format(parsed);
    contents.push_str(".format");
    let Some(params_string) = clean_params_dictionary(checker, params) else {
        return None;
    };
    if params_string.is_empty() {
        return None;
    };
    contents.push_str(&params_string);
    Some(contents)
}

/// Returns true if any of the [`PercentFormatPart`] components
/// (`conversion_flag`, `width`, and `precision`) are non-empty.
fn is_nontrivial(pf: &PercentFormatPart) -> bool {
    let mut cf_bool = false;
    let mut w_bool = false;
    let mut precision_bool = false;
    if let Some(conversion_flag) = &pf.conversion_flag {
        cf_bool = !conversion_flag.is_empty();
    }
    if let Some(width) = &pf.width {
        w_bool = !width.is_empty();
    }
    if let Some(precision) = &pf.precision {
        precision_bool = !precision.is_empty();
    }
    cf_bool || w_bool || precision_bool
}

/// Returns `true` if the sequence of [`PercentFormatPart`] indicate that an
/// [`Expr`] can be converted.
fn convertable(parsed: &[PercentFormat], right: &Expr) -> bool {
    for item in parsed {
        let Some(ref fmt) = item.format_spec else {
            continue;
        };
        // These require out-of-order parameter consumption.
        if fmt.width == Some("*".to_string()) || fmt.precision == Some(".*".to_string()) {
            return false;
        }
        // These conversions require modification of parameters.
        if vec!["d", "i", "u", "c"].contains(&&fmt.conversion[..]) {
            return false;
        }
        // py2: %#o formats different from {:#o}.
        if fmt
            .conversion_flag
            .clone()
            .unwrap_or_default()
            .contains('#')
            && fmt.conversion == "o"
        {
            return false;
        }
        // No equivalent in format.
        if let Some(key) = &fmt.key {
            if key.is_empty() {
                return false;
            }
        }
        // py2: conversion is subject to modifiers.
        let nontrivial = is_nontrivial(&fmt);
        if fmt.conversion == *"%" && nontrivial {
            return false;
        }
        // No equivalent in format.
        if vec!["a", "r"].contains(&&fmt.conversion[..]) && nontrivial {
            return false;
        }
        // %s with None and width is not supported.
        if let Some(width) = &fmt.width {
            if !width.is_empty() && fmt.conversion == *"s" {
                return false;
            }
        }
        // All dict substitutions must be named.
        if let ExprKind::Dict { .. } = &right.node {
            if fmt.key.is_none() {
                return false;
            }
        }
    }
    true
}

/// UP031
pub(crate) fn printf_string_formatting(checker: &mut Checker, expr: &Expr, right: &Expr) {
    let existing = checker
        .locator
        .slice_source_code_range(&Range::from_located(expr));

    // Split `"%s" % "Hello, world"` into `"%s"` and `"Hello, world"`.
    let mut split = MODULO_CALL.split(&existing);
    let Some(left_string) = split.next() else {
        return
    };
    if split.count() < 1 {
        return;
    }

    // Parse the format string (e.g. `"%s"`) into a list of `PercentFormat`.
    let Ok(parsed) = parse_percent_format(left_string) else {
        return;
    };
    if !convertable(&parsed, right) {
        return;
    }

    let mut contents = String::with_capacity(existing.len());
    match &right.node {
        ExprKind::Tuple { .. } => {
            contents = fix_percent_format_tuple(checker, right, &parsed);
        }
        ExprKind::Dict { .. } => {
            contents = match fix_percent_format_dict(checker, right, &parsed) {
                Some(string) => string,
                None => return,
            };
        }
        _ => {}
    }

    // If there is no change, then bail.
    if existing == contents {
        return;
    }

    let mut diagnostic = Diagnostic::new(
        violations::PrintfStringFormatting,
        Range::from_located(expr),
    );
    if checker.patch(&Rule::PrintfStringFormatting) {
        diagnostic.amend(Fix::replacement(
            contents,
            expr.location,
            expr.end_location.unwrap(),
        ));
    }
    checker.diagnostics.push(diagnostic);
}

#[cfg(test)]
mod test {
    use test_case::test_case;

    use super::*;

    #[test]
    fn test_parse_percent_format_none() {
        let sample = "\"\"";
        let e1 = PercentFormat::new("\"\"".to_string(), None);
        let expected = vec![e1];

        let received = parse_percent_format(sample).unwrap();
        assert_eq!(received, expected);
    }

    #[test]
    fn test_parse_percent_format_twice() {
        let sample = "\"%s two! %s\"";
        let sube1 = PercentFormatPart::new(None, None, None, None, "s".to_string());
        let e1 = PercentFormat::new("\"".to_string(), Some(sube1.clone()));
        let e2 = PercentFormat::new(" two! ".to_string(), Some(sube1.clone()));
        let e3 = PercentFormat::new("\"".to_string(), None);
        let expected = vec![e1, e2, e3];

        let received = parse_percent_format(sample).unwrap();
        assert_eq!(received, expected);
    }

    #[test]
    fn test_parse_percent_format_consecutive() {
        let sample = "\"%s%s\"";
        let sube1 = PercentFormatPart::new(None, None, None, None, "s".to_string());
        let e1 = PercentFormat::new(" two! ".to_string(), Some(sube1.clone()));
        let e2 = PercentFormat::new("\"".to_string(), Some(sube1));
        let e3 = PercentFormat::new("\"".to_string(), None);
        let expected = vec![e2, e1, e3];

        let received = parse_percent_format(sample).unwrap();
        assert_eq!(received, expected);
    }

    #[test_case("\"%ld\"", PercentFormatPart::new(None, None, None, None, "d".to_string()); "two letter")]
    #[test_case( "\"%.*f\"", PercentFormatPart::new(None, None, None, Some(".*".to_string()), "f".to_string()); "dot star letter")]
    #[test_case( "\"%.5f\"", PercentFormatPart::new(None, None, None, Some(".5".to_string()), "f".to_string()); "dot number letter")]
    #[test_case( "\"%.f\"", PercentFormatPart::new(None, None, None, Some(".".to_string()), "f".to_string()); "dot letter")]
    #[test_case( "\"%*d\"", PercentFormatPart::new(None, None, Some("*".to_string()), None, "d".to_string()); "star d")]
    #[test_case( "\"%5d\"", PercentFormatPart::new(None, None, Some("5".to_string()), None, "d".to_string()); "number letter")]
    #[test_case( "\"% #0-+d\"", PercentFormatPart::new(None, Some("#0- +".to_string()), None, None, "d".to_string()); "hashtag and symbols")]
    #[test_case( "\"%#o\"", PercentFormatPart::new(None, Some("#".to_string()), None, None, "o".to_string()); "format hashtag")]
    #[test_case( "\"%()s\"", PercentFormatPart::new(Some(String::new()), None, None, None, "s".to_string()); "empty paren")]
    #[test_case( "\"%(hi)s\"", PercentFormatPart::new(Some("hi".to_string()), None, None, None, "s".to_string()); "word in paren")]
    #[test_case( "\"%s\"", PercentFormatPart::new(None, None, None, None, "s".to_string()); "format s")]
    #[test_case( "\"%a\"", PercentFormatPart::new(None, None, None, None, "a".to_string()); "format an a")]
    #[test_case( "\"%r\"", PercentFormatPart::new(None, None, None, None, "r".to_string()); "format an r")]
    fn test_parse_percent_format(sample: &str, expected: PercentFormatPart) {
        let e1 = PercentFormat::new("\"".to_string(), Some(expected));
        let e2 = PercentFormat::new("\"".to_string(), None);
        let expected = vec![e1, e2];

        let received = parse_percent_format(sample).unwrap();
        assert_eq!(received, expected);
    }

    #[test]
    fn test_one_parenthesis_non_formatting() {
        let sample = "Writing merged info for %s slides (%s unrecognized)";
        let sube1 = PercentFormatPart::new(None, None, None, None, "s".to_string());
        let e1 = PercentFormat::new("Writing merged info for ".to_string(), Some(sube1.clone()));
        let e2 = PercentFormat::new(" slides (".to_string(), Some(sube1));
        let e3 = PercentFormat::new(" unrecognized)".to_string(), None);
        let expected = vec![e1, e2, e3];

        let received = parse_percent_format(sample).unwrap();
        assert_eq!(received, expected);
    }

    #[test]
    fn test_two_parenthesis_non_formatting() {
        let sample = "Expected one image (got %d) per channel (got %d)";
        let sube1 = PercentFormatPart::new(None, None, None, None, "d".to_string());
        let e1 = PercentFormat::new("Expected one image (got ".to_string(), Some(sube1.clone()));
        let e2 = PercentFormat::new(") per channel (got ".to_string(), Some(sube1));
        let e3 = PercentFormat::new(")".to_string(), None);
        let expected = vec![e1, e2, e3];

        let received = parse_percent_format(sample).unwrap();
        assert_eq!(received, expected);
    }

    #[test]
    fn test_parse_percent_format_everything() {
        let sample = "\"%(complete)#4.4f\"";
        let sube1 = PercentFormatPart::new(
            Some("complete".to_string()),
            Some("#".to_string()),
            Some("4".to_string()),
            Some(".4".to_string()),
            "f".to_string(),
        );
        let e1 = PercentFormat::new("\"".to_string(), Some(sube1));
        let e2 = PercentFormat::new("\"".to_string(), None);
        let expected = vec![e1, e2];

        let received = parse_percent_format(sample).unwrap();
        assert_eq!(received, expected);
    }

    #[test_case("\"%s\"", "\"{}\""; "simple string")]
    #[test_case("\"%%%s\"", "\"%{}\""; "three percents")]
    #[test_case("\"%(foo)s\"", "\"{foo}\""; "word in string")]
    #[test_case("\"%2f\"", "\"{:2f}\""; "formatting in string")]
    #[test_case("\"%r\"", "\"{!r}\""; "format an r")]
    #[test_case("\"%a\"", "\"{!a}\""; "format an a")]
    fn test_percent_to_format(sample: &str, expected: &str) {
        let received = percent_to_format(&parse_percent_format(sample).unwrap());
        assert_eq!(received, expected);
    }

    #[test_case("", ""; "preserve blanks")]
    #[test_case(" ", " "; "preserve one space")]
    #[test_case("  ", " "; "two spaces to one")]
    #[test_case("#0- +", "#<+"; "complex format")]
    #[test_case("-", "<"; "simple format")]
    fn test_simplify_conversion_flag(sample: &str, expected: &str) {
        let received = simplify_conversion_flag(sample);
        assert_eq!(received, expected);
    }
}
