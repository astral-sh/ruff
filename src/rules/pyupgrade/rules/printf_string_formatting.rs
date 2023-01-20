use std::str::FromStr;

use once_cell::sync::Lazy;
use regex::Regex;
use rustpython_common::cformat::{
    CConversionFlags, CFormatPart, CFormatQuantity, CFormatSpec, CFormatString,
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
static EMOJI_SYNTAX: Lazy<Regex> = Lazy::new(|| Regex::new(r"\\N\{.*?}").unwrap());

#[derive(Debug, PartialEq, Clone)]
struct PercentFormatPart {
    key: Option<String>,
    conversion_flag: Option<String>,
    width: Option<String>,
    precision: Option<String>,
    conversion: String,
}

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

    fn from_rustpython(spec: &CFormatSpec) -> Self {
        let clean_width = match &spec.min_field_width {
            Some(width_item) => match width_item {
                CFormatQuantity::Amount(amount) => Some(amount.to_string()),
                // FOR REVIEWER: Not sure if below is the correct way to handle
                // FromValuesTuple
                CFormatQuantity::FromValuesTuple => Some("*".to_string()),
            },
            None => None,
        };
        let clean_precision = match &spec.precision {
            Some(width_item) => match width_item {
                CFormatQuantity::Amount(amount) => Some(format!(".{amount}")),
                // FOR REVIEWER: Not sure if below is the correct way to handle
                // FromValuesTuple
                CFormatQuantity::FromValuesTuple => Some(".*".to_string()),
            },
            None => None,
        };
        let flags = if spec.flags.is_empty() {
            None
        } else {
            Some(get_flags(spec.flags))
        };

        PercentFormatPart::new(
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
    parts: Option<PercentFormatPart>,
}

impl PercentFormat {
    fn new(item: String, parts: Option<PercentFormatPart>) -> Self {
        Self { item, parts }
    }
}

/// Converts `RustPython`'s C Conversion Flags into their python string
/// representation
fn get_flags(flags: CConversionFlags) -> String {
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

/// Converts a string to a vector of `PercentFormat` structs
fn parse_percent_format(string: &str) -> Vec<PercentFormat> {
    let mut formats: Vec<PercentFormat> = vec![];

    let Ok(format_string) = CFormatString::from_str(string) else {
        return formats;
    };
    let format_vec: Vec<&CFormatPart<String>> =
        format_string.iter().map(|(_, part)| part).collect();
    for (i, part) in format_vec.iter().enumerate() {
        if let CFormatPart::Literal(item) = &part {
            let mut current_format = PercentFormat::new(item.to_string(), None);
            let the_next = match format_vec.get(i + 1) {
                Some(next) => next,
                None => {
                    formats.push(current_format);
                    continue;
                }
            };
            if let CFormatPart::Spec(c_spec) = &the_next {
                current_format.parts = Some(PercentFormatPart::from_rustpython(c_spec));
            }
            formats.push(current_format);
        }
    }
    formats
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

fn handle_part(part: &PercentFormat) -> String {
    let mut string = part.item.clone();
    string = curly_escape(&string);
    let mut fmt = match part.parts.clone() {
        None => return string,
        Some(item) => item,
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
    }
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
    if !fmt.conversion.is_empty() {
        parts.push(fmt.conversion);
    }
    for character in converter.chars() {
        parts.push(character.to_string());
    }
    parts.push("}".to_string());
    String::from_iter(parts)
}

fn percent_to_format(string: &str) -> String {
    let mut final_string = String::new();
    for part in parse_percent_format(string) {
        let handled = handle_part(&part);
        final_string.push_str(&handled);
    }
    final_string
}

/// If the tuple has one argument it removes the comma, otherwise it returns the
/// tuple as is
fn clean_right_tuple(checker: &mut Checker, right: &Expr) -> String {
    // FOR REVIEWER: Let me know if you want this redone in libcst, the reason I
    // didnt is because it starts as a Tuple, but ends as a Call
    let mut base_string = checker
        .locator
        .slice_source_code_range(&Range::from_located(right))
        .to_string();
    let is_multi_line = base_string.contains('\n');
    if let ExprKind::Tuple { elts, .. } = &right.node {
        if elts.len() == 1 {
            if !is_multi_line {
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
/// possible. This function also looks for areas that might cause issues, and
/// returns an empty string if it finds one
fn clean_right_dict(checker: &mut Checker, right: &Expr) -> Option<String> {
    let whole_range = Range::new(right.location, right.end_location.unwrap());
    let whole_string = checker.locator.slice_source_code_range(&whole_range);
    let is_multi_line = whole_string.contains('\n');
    let mut new_string = String::new();
    if let ExprKind::Dict { keys, values } = &right.node {
        let mut new_vals: Vec<String> = vec![];
        let mut indent = String::new();
        let mut already_seen: Vec<String> = vec![];
        for (key, value) in keys.iter().zip(values.iter()) {
            // The original unit tests of pyupgrade reveal that we should not rewrite
            // non-string keys
            if let ExprKind::Constant {
                value: Constant::Str(key_string),
                ..
            } = &key.node
            {
                // If the dictionary key is not a valid python variable name, then do not fix
                if !PYTHON_NAME.is_match(key_string) {
                    return None;
                }
                // We should not rewrite if the key is a python keyword
                if is_keyword(key_string) {
                    return None;
                }
                // If there are multiple entries of the same key, we need to return because we
                // cannot handle this ambiguity
                if already_seen.contains(key_string) {
                    return None;
                }
                already_seen.push(key_string.clone());
                let mut new_string = String::new();
                if is_multi_line && indent.is_empty() {
                    indent = indentation(checker.locator, key).unwrap().to_string();
                }
                let value_range = Range::new(value.location, value.end_location.unwrap());
                let value_string = checker.locator.slice_source_code_range(&value_range);
                new_string.push_str(key_string);
                new_string.push('=');
                new_string.push_str(&value_string);
                new_vals.push(new_string);
            } else {
                // If there are any non-string keys, we should be timid and not modify the
                // string
                return None;
            }
        }
        // If we couldn't parse out key values return an empty string so that we don't
        // attempt a fix
        if new_vals.is_empty() {
            return None;
        }
        new_string.push('(');
        if is_multi_line {
            for item in &new_vals {
                new_string.push('\n');
                new_string.push_str(&indent);
                new_string.push_str(item);
                // This implementation adds a trailing comma always, let me know if you want a
                // more in-depth solution
                new_string.push(',');
            }
            // For the ending parentheses we want to go back one indent
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

fn fix_percent_format_tuple(checker: &mut Checker, right: &Expr, left_string: &str) -> String {
    let mut cleaned_string = percent_to_format(left_string);
    cleaned_string.push_str(".format");
    let right_string = clean_right_tuple(checker, right);
    cleaned_string.push_str(&right_string);
    cleaned_string
}

fn fix_percent_format_dict(
    checker: &mut Checker,
    right: &Expr,
    left_string: &str,
) -> Option<String> {
    let mut cleaned_string = percent_to_format(left_string);
    cleaned_string.push_str(".format");
    let right_string = match clean_right_dict(checker, right) {
        // If we could not properly parse the dictionary we should None so the program knows not to
        // fix this
        None => return None,
        Some(string) => {
            if string.is_empty() {
                return None;
            }
            string
        }
    };
    cleaned_string.push_str(&right_string);
    Some(cleaned_string)
}

/// Returns true if any of `conversion_flag`, `width`, and `precision` are a
/// non-empty string
fn get_nontrivial_fmt(pf: &PercentFormatPart) -> bool {
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

/// Checks the string for a number of issues that mean we should not convert
/// things
fn check_statement(parsed: Vec<PercentFormat>, right: &Expr) -> bool {
    for item in parsed {
        let fmt = match item.parts {
            None => continue,
            Some(item) => item,
        };
        // timid: these require out-of-order parameter consumption
        if fmt.width == Some("*".to_string()) || fmt.precision == Some(".*".to_string()) {
            return false;
        }
        // these conversions require modification of parameters
        if vec!["d", "i", "u", "c"].contains(&&fmt.conversion[..]) {
            return false;
        }
        // timid: py2: %#o formats different from {:#o} (--py3?)
        if fmt
            .conversion_flag
            .clone()
            .unwrap_or_default()
            .contains('#')
            && fmt.conversion == "o"
        {
            return false;
        }
        // no equivalent in format
        if let Some(key) = &fmt.key {
            if key.is_empty() {
                return false;
            }
        }
        // timid: py2: conversion is subject to modifiers (--py3?)
        let nontrivial_fmt = get_nontrivial_fmt(&fmt);
        if fmt.conversion == *"%" && nontrivial_fmt {
            return false;
        }
        // no equivalent in format
        if vec!["a", "r"].contains(&&fmt.conversion[..]) && nontrivial_fmt {
            return false;
        }
        // %s with None and width is not supported
        if let Some(width) = &fmt.width {
            if !width.is_empty() && fmt.conversion == *"s" {
                return false;
            }
        }
        // all dict substitutions must be named
        if let ExprKind::Dict { .. } = &right.node {
            // Technically a value of "" would also count as `not key`, (which is what the
            // python code uses) BUT we already have a check above for this
            if fmt.key.is_none() {
                return false;
            }
        }
    }
    true
}

/// UP031
pub(crate) fn printf_string_formatting(checker: &mut Checker, expr: &Expr, right: &Expr) {
    let expr_string = checker
        .locator
        .slice_source_code_range(&Range::from_located(expr));

    let mut split = MODULO_CALL.split(&expr_string);
    // Pyupgrade does this test in the functions that change, but I am relying on
    // this logic for something else, so I will use it here, pyupgrade notes
    // this is an overly timid check
    let Some(left_string) = split.next() else {
        return
    };
    if split.count() < 1 {
        return;
    }

    let parsed = parse_percent_format(left_string);
    let is_valid = check_statement(parsed, right);
    // If the statement is not valid, then bail
    if !is_valid {
        return;
    }
    let mut new_string = String::new();
    match &right.node {
        ExprKind::Tuple { .. } => {
            new_string = fix_percent_format_tuple(checker, right, left_string);
        }
        ExprKind::Dict { .. } => {
            new_string = match fix_percent_format_dict(checker, right, left_string) {
                None => return,
                Some(string) => string,
            };
        }
        _ => {}
    }
    // We should not replace if the string we get back is empty
    if new_string.is_empty() {
        return;
    }
    let old_string = checker
        .locator
        .slice_source_code_range(&Range::from_located(expr));
    // If there is no change, then bail
    if new_string == old_string {
        return;
    }
    let mut diagnostic = Diagnostic::new(
        violations::PrintfStringFormatting,
        Range::from_located(expr),
    );
    // Emoji sytnax is very rare and adds a lot of complexity to the code, so we are
    // only issuing a warning if it exists, and not fixing the code
    if checker.patch(&Rule::PrintfStringFormatting) {
        if !EMOJI_SYNTAX.is_match(&expr_string) {
            diagnostic.amend(Fix::replacement(
                new_string,
                expr.location,
                expr.end_location.unwrap(),
            ));
        }
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

        let received = parse_percent_format(sample);
        assert_eq!(received, expected);
    }

    #[test]
    fn test_parse_percent_format_double_two() {
        let sample = "\"%s two! %s\"";
        let sube1 = PercentFormatPart::new(None, None, None, None, "s".to_string());
        let e1 = PercentFormat::new(" two! ".to_string(), Some(sube1.clone()));
        let e2 = PercentFormat::new("\"".to_string(), Some(sube1));
        let e3 = PercentFormat::new("\"".to_string(), None);
        let expected = vec![e2, e1, e3];

        let received = parse_percent_format(sample);
        assert_eq!(received, expected);
    }

    #[test_case("\"%ld\"",PercentFormatPart::new(None, None, None, None, "d".to_string()); "two letter")]
    #[test_case( "\"%.*f\"",PercentFormatPart::new(None, None, None, Some(".*".to_string()), "f".to_string()); "dot star letter")]
    #[test_case( "\"%.5f\"",PercentFormatPart::new(None, None, None, Some(".5".to_string()), "f".to_string()); "dot number letter")]
    #[test_case( "\"%.f\"",PercentFormatPart::new(None, None, None, Some(".".to_string()), "f".to_string()); "dot letter")]
    #[test_case( "\"%*d\"",PercentFormatPart::new(None, None, Some("*".to_string()), None, "d".to_string()); "star d")]
    #[test_case( "\"%5d\"",PercentFormatPart::new(None, None, Some("5".to_string()), None, "d".to_string()); "number letter")]
    #[test_case( "\"% #0-+d\"",PercentFormatPart::new(None, Some("#0- +".to_string()), None, None, "d".to_string()); "hastag and symbols")]
    #[test_case( "\"%#o\"",PercentFormatPart::new(None, Some("#".to_string()), None, None, "o".to_string()); "format hashtag")]
    #[test_case( "\"%()s\"",PercentFormatPart::new(Some(String::new()), None, None, None, "s".to_string()); "empty paren")]
    #[test_case( "\"%(hi)s\"",PercentFormatPart::new(Some("hi".to_string()), None, None, None, "s".to_string()); "word in paren")]
    #[test_case( "\"%s\"",PercentFormatPart::new(None, None, None, None, "s".to_string()); "format s")]
    // #[test_case( "\"%%\"",PercentFormatPart::new(None, None, None, None, "%".to_string());
    // "format double percentage")]
    #[test_case( "\"%a\"",PercentFormatPart::new(None, None, None, None, "a".to_string()); "format an a")]
    #[test_case( "\"%r\"",PercentFormatPart::new(None, None, None, None, "r".to_string()); "format an r")]
    fn test_parse_percent_format(sample: &str, expected: PercentFormatPart) {
        let e1 = PercentFormat::new("\"".to_string(), Some(expected));
        let e2 = PercentFormat::new("\"".to_string(), None);
        let expected = vec![e1, e2];

        let received = parse_percent_format(sample);
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

        let received = parse_percent_format(sample);
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

        let received = parse_percent_format(sample);
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

        let received = parse_percent_format(sample);
        assert_eq!(received, expected);
    }

    #[test_case("\"%s\"", "\"{}\""; "simple string")]
    #[test_case("\"%%%s\"", "\"%{}\""; "three percents")]
    #[test_case("\"%(foo)s\"", "\"{foo}\""; "word in string")]
    #[test_case("\"%2f\"", "\"{:2f}\""; "formatting in string")]
    #[test_case("\"%r\"", "\"{!r}\""; "format an r")]
    #[test_case("\"%a\"", "\"{!a}\""; "format an a")]
    fn test_percent_to_format(sample: &str, expected: &str) {
        let received = percent_to_format(sample);
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
