use std::str::FromStr;

use once_cell::sync::Lazy;
use regex::Regex;
use rustpython_common::cformat::{
    CConversionFlags, CFormatPart, CFormatPrecision, CFormatQuantity, CFormatString,
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

fn simplify_conversion_flag(flags: CConversionFlags) -> String {
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
                format_string.push_str(key_item);
            }

            if !spec.flags.is_empty()
                || spec.min_field_width.is_some()
                || spec.precision.is_some()
                || (spec.format_char != 's' && spec.format_char != 'r' && spec.format_char != 'a')
            {
                format_string.push(':');

                if !spec.flags.is_empty() {
                    format_string.push_str(&simplify_conversion_flag(spec.flags));
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
    for (.., format_part) in format_string.iter() {
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
    let mut contents = String::new();
    if let ExprKind::Dict { keys, values } = &right.node {
        let mut arguments: Vec<String> = vec![];
        let mut seen: Vec<&str> = vec![];
        let mut indent = None;
        for (key, value) in keys.iter().zip(values.iter()) {
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
                if seen.contains(&key_string.as_str()) {
                    return None;
                }
                seen.push(key_string);
                let mut contents = String::new();
                if is_multi_line {
                    if indent.is_none() {
                        indent = indentation(checker.locator, key);
                    }
                }

                let value_string = checker
                    .locator
                    .slice_source_code_range(&Range::from_located(value));
                contents.push_str(key_string);
                contents.push('=');
                contents.push_str(&value_string);
                arguments.push(contents);
            } else {
                // If there are any non-string keys, abort.
                return None;
            }
        }
        // If we couldn't parse out key values, abort.
        if arguments.is_empty() {
            return None;
        }
        contents.push('(');
        if is_multi_line {
            // If this is a multi-line dictionary, abort.
            let Some(indent) = indent else {
                return None;
            };

            for item in &arguments {
                contents.push('\n');
                contents.push_str(&indent);
                contents.push_str(item);
                contents.push(',');
            }

            // For the ending parentheses, go back one indent.
            contents.push('\n');
            if indent.len() > 3 {
                contents.push_str(&indent[3..]);
            }
        } else {
            contents.push_str(&arguments.join(", "));
        }
        contents.push(')');
    }
    Some(contents)
}

/// Returns `true` if the sequence of [`PercentFormatPart`] indicate that an
/// [`Expr`] can be converted.
fn convertible(format_string: &CFormatString, right: &Expr) -> bool {
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
        if fmt.mapping_key.as_ref().map_or(false, String::is_empty) {
            return false;
        }

        let is_nontrivial =
            !fmt.flags.is_empty() || fmt.min_field_width.is_some() || fmt.precision.is_some();

        // Conversion is subject to modifiers.
        if is_nontrivial && fmt.format_char == '%' {
            return false;
        }

        // No equivalent in `format`.
        if is_nontrivial && (fmt.format_char == 'a' || fmt.format_char == 'r') {
            return false;
        }

        // "%s" with None and width is not supported.
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

    if !convertible(&format_string, right) {
        return;
    }

    let format_string = percent_to_format(&format_string);
    let params_string = match right.node {
        ExprKind::Tuple { .. } => clean_params_tuple(checker, right),
        ExprKind::Dict { .. } => {
            if let Some(params_string) = clean_params_dictionary(checker, right) {
                params_string
            } else {
                return;
            }
        }
        _ => return,
    };

    // TODO(charlie): Avoid any fixes that result in overly long lines.
    // TODO(charlie): Avoid fixing cases in which the modulo is on its own line.
    let contents = format!("{format_string}.format{params_string}");

    let mut diagnostic = Diagnostic::new(
        violations::PrintfStringFormatting,
        Range::from_located(expr),
    );
    if checker.patch(&Rule::PrintfStringFormatting) {
        if !EMOJI_SYNTAX.is_match(&left_string) {
            diagnostic.amend(Fix::replacement(
                contents,
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

    #[test_case("\"%s\"", "\"{}\""; "simple string")]
    #[test_case("\"%%%s\"", "\"%{}\""; "three percents")]
    #[test_case("\"%(foo)s\"", "\"{foo}\""; "word in string")]
    #[test_case("\"%2f\"", "\"{:2f}\""; "formatting in string")]
    #[test_case("\"%r\"", "\"{!r}\""; "format an r")]
    #[test_case("\"%a\"", "\"{!a}\""; "format an a")]
    fn test_percent_to_format(sample: &str, expected: &str) {
        let format_string = CFormatString::from_str(sample).unwrap();
        let actual = percent_to_format(&format_string);
        assert_eq!(actual, expected);
    }

    #[test]
    fn preserve_blanks() {
        assert_eq!(
            simplify_conversion_flag(CConversionFlags::empty()),
            String::new()
        );
    }

    #[test]
    fn preserve_space() {
        assert_eq!(
            simplify_conversion_flag(CConversionFlags::BLANK_SIGN),
            " ".to_string()
        );
    }

    #[test]
    fn complex_format() {
        assert_eq!(
            simplify_conversion_flag(CConversionFlags::all()),
            "<+#".to_string()
        );
    }
}
