use std::str::FromStr;

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

fn simplify_conversion_flag(flags: &CConversionFlags) -> String {
    let mut flag_string = String::new();
    if flags.contains(CConversionFlags::LEFT_ADJUST) {
        flag_string.push('<');
    }
    if flags.contains(CConversionFlags::SIGN_CHAR) {
        flag_string.push('+');
    }
    if flags.contains(CConversionFlags::ALTERNATE_FORM) {
        flag_string.push('#');
    }
    if flags.contains(CConversionFlags::BLANK_SIGN) {
        if !flags.contains(CConversionFlags::SIGN_CHAR) {
            flag_string.push(' ');
        }
    }
    if flags.contains(CConversionFlags::ZERO_PAD) {
        if !flags.contains(CConversionFlags::LEFT_ADJUST) {
            flag_string.push('0');
        }
    }
    flag_string
}

/// Convert a [`PercentFormat`] struct into a `String`.
fn handle_part(part: &CFormatPart<String>) -> String {
    match part {
        CFormatPart::Literal(item) => curly_escape(item),
        CFormatPart::Spec(spec) => {
            let mut format_string = String::new();

            // TODO(charlie): What case is this?
            if spec.format_char == '%' {
                format_string.push('%');
                return format_string;
            }

            format_string.push('{');

            // Ex) `{foo}`
            if let Some(key_item) = &spec.mapping_key {
                format_string.push_str(&key_item);
            }

            let converter: String;
            if spec.format_char == 'r' || spec.format_char == 'a' {
                converter = format!("!{}", spec.format_char);
            } else {
                converter = String::new();
            }

            if !spec.flags.is_empty()
                || spec.min_field_width.is_some()
                || spec.precision.is_some()
                || (spec.format_char != 's' && spec.format_char != 'r' && spec.format_char != 'a')
            {
                format_string.push(':');

                if !spec.flags.is_empty() {
                    format_string.push_str(&simplify_conversion_flag(&spec.flags));
                }

                if let Some(width) = &spec.min_field_width {
                    let amount = match width {
                        CFormatQuantity::Amount(amount) => amount,
                        CFormatQuantity::FromValuesTuple => {
                            unreachable!("FromValuesTuple is unsupported")
                        }
                    };
                    format_string.push_str(&amount.to_string());
                }

                if let Some(precision) = &spec.precision {
                    match precision {
                        CFormatPrecision::Quantity(quantity) => match quantity {
                            CFormatQuantity::Amount(amount) => {
                                format_string.push('.');
                                format_string.push_str(&amount.to_string());
                            }
                            CFormatQuantity::FromValuesTuple => {
                                unreachable!("Width should be a usize")
                            }
                        },
                        CFormatPrecision::Dot => {
                            format_string.push('.');
                            format_string.push('0');
                        }
                    }
                }
            }
            if spec.format_char != 's' && spec.format_char != 'r' && spec.format_char != 'a' {
                format_string.push(spec.format_char);
            }
            if spec.format_char == 'r' || spec.format_char == 'a' {
                format_string.push('!');
                format_string.push(spec.format_char);
            }
            format_string.push('}');
            format_string
        }
    }
}

/// Convert a [`CFormatString`] into a `String`.
fn percent_to_format(format_string: &CFormatString) -> String {
    let mut contents = String::new();
    for (i, format_part) in format_string.iter() {
        contents.push_str(&handle_part(format_part));
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
    format_string: &CFormatString,
) -> String {
    let mut contents = percent_to_format(format_string);
    contents.push_str(".format");
    let params_string = clean_params_tuple(checker, params);
    contents.push_str(&params_string);
    contents
}

fn fix_percent_format_dict(
    checker: &mut Checker,
    params: &Expr,
    format_string: &CFormatString,
) -> Option<String> {
    let mut contents = percent_to_format(format_string);
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
fn is_nontrivial(pf: &CFormatSpec) -> bool {
    !pf.flags.is_empty() || pf.min_field_width.is_some() || pf.precision.is_some()
}

/// Returns `true` if the sequence of [`PercentFormatPart`] indicate that an
/// [`Expr`] can be converted.
fn convertable(format_string: &CFormatString, right: &Expr) -> bool {
    for (.., format_part) in format_string.iter() {
        let CFormatPart::Spec(ref fmt) = format_part else {
            continue;
        };

        // These require out-of-order parameter consumption.
        if matches!(fmt.min_field_width, Some(CFormatQuantity::FromValuesTuple)) {
            return false;
        }
        if matches!(
            fmt.precision,
            Some(CFormatPrecision::Quantity(CFormatQuantity::FromValuesTuple))
        ) {
            return false;
        }

        // These conversions require modification of parameters.
        if fmt.format_char == 'd'
            || fmt.format_char == 'i'
            || fmt.format_char == 'u'
            || fmt.format_char == 'c'
        {
            return false;
        }

        // No equivalent in format.
        if fmt.mapping_key.as_ref().map_or(false, |key| key.is_empty()) {
            return false;
        }

        // py2: conversion is subject to modifiers.
        let nontrivial = is_nontrivial(fmt);
        if fmt.format_char == '%' && nontrivial {
            return false;
        }
        // No equivalent in format.
        if nontrivial && (fmt.format_char == 'a' || fmt.format_char == 'r') {
            return false;
        }
        // %s with None and width is not supported.
        if fmt.min_field_width.is_some() && fmt.format_char == 's' {
            return false;
        }
        // All dict substitutions must be named.
        if let ExprKind::Dict { .. } = &right.node {
            if fmt.mapping_key.is_none() {
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
    let Ok(format_string) = CFormatString::from_str(left_string) else {
        return;
    };
    println!("{:?}", format_string);
    if !convertable(&format_string, right) {
        return;
    }

    let mut contents = String::with_capacity(existing.len());
    match &right.node {
        ExprKind::Tuple { .. } => {
            contents = fix_percent_format_tuple(checker, right, &format_string);
        }
        ExprKind::Dict { .. } => {
            contents = match fix_percent_format_dict(checker, right, &format_string) {
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

    // #[test]
    // fn test_parse_percent_format_none() {
    //     let sample = "\"\"";
    //     let e1 = PercentFormat::new("\"\"".to_string(), None);
    //     let expected = vec![e1];
    //
    //     let received = parse_percent_format(sample).unwrap();
    //     assert_eq!(received, expected);
    // }
    //
    // #[test]
    // fn test_parse_percent_format_twice() {
    //     let sample = "\"%s two! %s\"";
    //     let sube1 = PercentFormatPart::new(None, None, None, None,
    // "s".to_string());     let e1 = PercentFormat::new("\"".to_string(),
    // Some(sube1.clone()));     let e2 = PercentFormat::new(" two!
    // ".to_string(), Some(sube1));     let e3 =
    // PercentFormat::new("\"".to_string(), None);     let expected =
    // vec![e1, e2, e3];
    //
    //     let received = parse_percent_format(sample).unwrap();
    //     assert_eq!(received, expected);
    // }
    //
    // #[test]
    // fn test_parse_percent_format_consecutive() {
    //     let sample = "\"%s%s\"";
    //     let sube1 = PercentFormatPart::new(None, None, None, None,
    // "s".to_string());     let e1 = PercentFormat::new(" two!
    // ".to_string(), Some(sube1.clone()));     let e2 =
    // PercentFormat::new("\"".to_string(), Some(sube1));     let e3 =
    // PercentFormat::new("\"".to_string(), None);     let expected =
    // vec![e2, e1, e3];
    //
    //     let received = parse_percent_format(sample).unwrap();
    //     assert_eq!(received, expected);
    // }
    //
    // #[test_case("\"%ld\"", PercentFormatPart::new(None, None, None, None,
    // "d".to_string()); "two letter")] #[test_case( "\"%.*f\"",
    // PercentFormatPart::new(None, None, None, Some(".*".to_string()),
    // "f".to_string()); "dot star letter")] #[test_case( "\"%.5f\"",
    // PercentFormatPart::new(None, None, None, Some(".5".to_string()),
    // "f".to_string()); "dot number letter")] #[test_case( "\"%.f\"",
    // PercentFormatPart::new(None, None, None, Some(".".to_string()),
    // "f".to_string()); "dot letter")] #[test_case( "\"%*d\"",
    // PercentFormatPart::new(None, None, Some("*".to_string()), None,
    // "d".to_string()); "star d")] #[test_case( "\"%5d\"",
    // PercentFormatPart::new(None, None, Some("5".to_string()), None,
    // "d".to_string()); "number letter")] #[test_case( "\"% #0-+d\"",
    // PercentFormatPart::new(None, Some("#0- +".to_string()), None, None,
    // "d".to_string()); "hashtag and symbols")] #[test_case( "\"%#o\"",
    // PercentFormatPart::new(None, Some("#".to_string()), None, None,
    // "o".to_string()); "format hashtag")] #[test_case( "\"%()s\"",
    // PercentFormatPart::new(Some(String::new()), None, None, None,
    // "s".to_string()); "empty paren")] #[test_case( "\"%(hi)s\"",
    // PercentFormatPart::new(Some("hi".to_string()), None, None, None,
    // "s".to_string()); "word in paren")] #[test_case( "\"%s\"",
    // PercentFormatPart::new(None, None, None, None, "s".to_string()); "format
    // s")] #[test_case( "\"%a\"", PercentFormatPart::new(None, None, None,
    // None, "a".to_string()); "format an a")] #[test_case( "\"%r\"",
    // PercentFormatPart::new(None, None, None, None, "r".to_string()); "format
    // an r")] fn test_parse_percent_format(sample: &str, expected:
    // PercentFormatPart) {     let e1 =
    // PercentFormat::new("\"".to_string(), Some(expected));     let e2 =
    // PercentFormat::new("\"".to_string(), None);     let expected =
    // vec![e1, e2];
    //
    //     let received = parse_percent_format(sample).unwrap();
    //     assert_eq!(received, expected);
    // }
    //
    // #[test]
    // fn test_one_parenthesis_non_formatting() {
    //     let sample = "Writing merged info for %s slides (%s unrecognized)";
    //     let sube1 = PercentFormatPart::new(None, None, None, None,
    // "s".to_string());     let e1 = PercentFormat::new("Writing merged
    // info for ".to_string(), Some(sube1.clone()));     let e2 =
    // PercentFormat::new(" slides (".to_string(), Some(sube1));     let e3
    // = PercentFormat::new(" unrecognized)".to_string(), None);
    //     let expected = vec![e1, e2, e3];
    //
    //     let received = parse_percent_format(sample).unwrap();
    //     assert_eq!(received, expected);
    // }
    //
    // #[test]
    // fn test_two_parenthesis_non_formatting() {
    //     let sample = "Expected one image (got %d) per channel (got %d)";
    //     let sube1 = PercentFormatPart::new(None, None, None, None,
    // "d".to_string());     let e1 = PercentFormat::new("Expected one image
    // (got ".to_string(), Some(sube1.clone()));     let e2 =
    // PercentFormat::new(") per channel (got ".to_string(), Some(sube1));
    //     let e3 = PercentFormat::new(")".to_string(), None);
    //     let expected = vec![e1, e2, e3];
    //
    //     let received = parse_percent_format(sample).unwrap();
    //     assert_eq!(received, expected);
    // }
    //
    // #[test]
    // fn test_parse_percent_format_everything() {
    //     let sample = "\"%(complete)#4.4f\"";
    //     let sube1 = PercentFormatPart::new(
    //         Some("complete".to_string()),
    //         Some("#".to_string()),
    //         Some("4".to_string()),
    //         Some(".4".to_string()),
    //         "f".to_string(),
    //     );
    //     let e1 = PercentFormat::new("\"".to_string(), Some(sube1));
    //     let e2 = PercentFormat::new("\"".to_string(), None);
    //     let expected = vec![e1, e2];
    //
    //     let received = parse_percent_format(sample).unwrap();
    //     assert_eq!(received, expected);
    // }
    //
    // #[test_case("\"%s\"", "\"{}\""; "simple string")]
    // #[test_case("\"%%%s\"", "\"%{}\""; "three percents")]
    // #[test_case("\"%(foo)s\"", "\"{foo}\""; "word in string")]
    // #[test_case("\"%2f\"", "\"{:2f}\""; "formatting in string")]
    // #[test_case("\"%r\"", "\"{!r}\""; "format an r")]
    // #[test_case("\"%a\"", "\"{!a}\""; "format an a")]
    // fn test_percent_to_format(sample: &str, expected: &str) {
    //     let received =
    // percent_to_format(&parse_percent_format(sample).unwrap());
    //     assert_eq!(received, expected);
    // }
    //
    // #[test_case("", ""; "preserve blanks")]
    // #[test_case(" ", " "; "preserve one space")]
    // #[test_case("  ", " "; "two spaces to one")]
    // #[test_case("#0- +", "#<+"; "complex format")]
    // #[test_case("-", "<"; "simple format")]
    // fn test_simplify_conversion_flag(sample: &str, expected: &str) {
    //     let received = simplify_conversion_flag(sample);
    //     assert_eq!(received, expected);
    // }
}
