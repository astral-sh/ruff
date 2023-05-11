use ruff_text_size::TextRange;
use std::str::FromStr;

use rustpython_common::cformat::{
    CConversionFlags, CFormatPart, CFormatPrecision, CFormatQuantity, CFormatString,
};
use rustpython_parser::ast::{self, Constant, Expr, ExprKind};
use rustpython_parser::{lexer, Mode, Tok};

use ruff_diagnostics::{AlwaysAutofixableViolation, Diagnostic, Edit, Fix};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::source_code::Locator;
use ruff_python_ast::str::{leading_quote, trailing_quote};
use ruff_python_ast::whitespace::indentation;
use ruff_python_stdlib::identifiers::is_identifier;

use crate::checkers::ast::Checker;
use crate::registry::AsRule;
use crate::rules::pyupgrade::helpers::curly_escape;

#[violation]
pub struct PrintfStringFormatting;

impl AlwaysAutofixableViolation for PrintfStringFormatting {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Use format specifiers instead of percent format")
    }

    fn autofix_title(&self) -> String {
        "Replace with format specifiers".to_string()
    }
}

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
fn clean_params_tuple(checker: &mut Checker, right: &Expr, locator: &Locator) -> String {
    let mut contents = checker.locator.slice(right.range()).to_string();
    if let ExprKind::Tuple(ast::ExprTuple { elts, .. }) = &right.node {
        if elts.len() == 1 {
            if !locator.contains_line_break(right.range()) {
                for (i, character) in contents.chars().rev().enumerate() {
                    if character == ',' {
                        let correct_index = contents.len() - i - 1;
                        contents.remove(correct_index);
                        break;
                    }
                }
            }
        }
    }
    contents
}

/// Converts a dictionary to a function call while preserving as much styling as
/// possible.
fn clean_params_dictionary(
    checker: &mut Checker,
    right: &Expr,
    locator: &Locator,
) -> Option<String> {
    let is_multi_line = locator.contains_line_break(right.range());
    let mut contents = String::new();
    if let ExprKind::Dict(ast::ExprDict { keys, values }) = &right.node {
        let mut arguments: Vec<String> = vec![];
        let mut seen: Vec<&str> = vec![];
        let mut indent = None;
        for (key, value) in keys.iter().zip(values.iter()) {
            match key {
                Some(key) => {
                    if let ExprKind::Constant(ast::ExprConstant {
                        value: Constant::Str(key_string),
                        ..
                    }) = &key.node
                    {
                        // If the dictionary key is not a valid variable name, abort.
                        if !is_identifier(key_string) {
                            return None;
                        }
                        // If there are multiple entries of the same key, abort.
                        if seen.contains(&key_string.as_str()) {
                            return None;
                        }
                        seen.push(key_string);
                        if is_multi_line {
                            if indent.is_none() {
                                indent = indentation(checker.locator, key);
                            }
                        }

                        let value_string = checker.locator.slice(value.range());
                        arguments.push(format!("{key_string}={value_string}"));
                    } else {
                        // If there are any non-string keys, abort.
                        return None;
                    }
                }
                None => {
                    let value_string = checker.locator.slice(value.range());
                    arguments.push(format!("**{value_string}"));
                }
            }
        }
        // If we couldn't parse out key values, abort.
        if arguments.is_empty() {
            return None;
        }
        contents.push('(');
        if is_multi_line {
            let Some(indent) = indent else {
                return None;
            };

            for item in &arguments {
                contents.push_str(checker.stylist.line_ending().as_str());
                contents.push_str(indent);
                contents.push_str(item);
                contents.push(',');
            }

            contents.push_str(checker.stylist.line_ending().as_str());

            // For the ending parentheses, go back one indent.
            let default_indent: &str = checker.stylist.indentation();
            if let Some(ident) = indent.strip_prefix(default_indent) {
                contents.push_str(ident);
            } else {
                contents.push_str(indent);
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
fn convertible(format_string: &CFormatString, params: &Expr) -> bool {
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
        if let ExprKind::Dict(_) = &params.node {
            if fmt.mapping_key.is_none() {
                return false;
            }
        }
    }
    true
}

/// UP031
pub(crate) fn printf_string_formatting(
    checker: &mut Checker,
    expr: &Expr,
    right: &Expr,
    locator: &Locator,
) {
    // Grab each string segment (in case there's an implicit concatenation).
    let mut strings: Vec<TextRange> = vec![];
    let mut extension = None;
    for (tok, range) in lexer::lex_starts_at(
        checker.locator.slice(expr.range()),
        Mode::Module,
        expr.start(),
    )
    .flatten()
    {
        if matches!(tok, Tok::String { .. }) {
            strings.push(range);
        } else if matches!(tok, Tok::Rpar) {
            // If we hit a right paren, we have to preserve it.
            extension = Some(range);
        } else if matches!(tok, Tok::Percent) {
            // Break as soon as we find the modulo symbol.
            break;
        }
    }

    // If there are no string segments, abort.
    if strings.is_empty() {
        return;
    }

    // Parse each string segment.
    let mut num_positional_arguments = 0;
    let mut num_keyword_arguments = 0;
    let mut format_strings = Vec::with_capacity(strings.len());
    for range in &strings {
        let string = checker.locator.slice(*range);
        let (Some(leader), Some(trailer)) = (leading_quote(string), trailing_quote(string)) else {
            return;
        };
        let string = &string[leader.len()..string.len() - trailer.len()];

        // Parse the format string (e.g. `"%s"`) into a list of `PercentFormat`.
        let Ok(format_string) = CFormatString::from_str(string) else {
            return;
        };
        if !convertible(&format_string, right) {
            return;
        }

        // Count the number of positional and keyword arguments.
        for (.., format_part) in format_string.iter() {
            let CFormatPart::Spec(ref fmt) = format_part else {
                continue;
            };
            if fmt.mapping_key.is_none() {
                num_positional_arguments += 1;
            } else {
                num_keyword_arguments += 1;
            }
        }

        // Convert the `%`-format string to a `.format` string.
        let format_string = percent_to_format(&format_string);
        format_strings.push(format!("{leader}{format_string}{trailer}"));
    }

    // Parse the parameters.
    let params_string = match right.node {
        ExprKind::Constant(_) | ExprKind::JoinedStr(_) => {
            format!("({})", checker.locator.slice(right.range()))
        }
        ExprKind::Name(_) | ExprKind::Attribute(_) | ExprKind::Subscript(_) | ExprKind::Call(_) => {
            if num_keyword_arguments > 0 {
                // If we have _any_ named fields, assume the right-hand side is a mapping.
                format!("(**{})", checker.locator.slice(right.range()))
            } else if num_positional_arguments > 1 {
                // If we have multiple fields, but no named fields, assume the right-hand side is a
                // tuple.
                format!("(*{})", checker.locator.slice(right.range()))
            } else {
                // Otherwise, if we have a single field, we can't make any assumptions about the
                // right-hand side. It _could_ be a tuple, but it could also be a single value,
                // and we can't differentiate between them.
                // For example:
                // ```python
                // x = (1,)
                // print("%s" % x)
                // print("{}".format(x))
                // ```
                return;
            }
        }
        ExprKind::Tuple(_) => clean_params_tuple(checker, right, locator),
        ExprKind::Dict(_) => {
            if let Some(params_string) = clean_params_dictionary(checker, right, locator) {
                params_string
            } else {
                return;
            }
        }
        _ => return,
    };

    // Reconstruct the string.
    let mut contents = String::new();
    let mut prev = None;
    for (range, format_string) in strings.iter().zip(format_strings) {
        // Add the content before the string segment.
        match prev {
            None => {
                contents.push_str(
                    checker
                        .locator
                        .slice(TextRange::new(expr.start(), range.start())),
                );
            }
            Some(prev) => {
                contents.push_str(checker.locator.slice(TextRange::new(prev, range.start())));
            }
        }
        // Add the string itself.
        contents.push_str(&format_string);
        prev = Some(range.end());
    }

    if let Some(range) = extension {
        contents.push_str(
            checker
                .locator
                .slice(TextRange::new(prev.unwrap(), range.end())),
        );
    }

    // Add the `.format` call.
    contents.push_str(&format!(".format{params_string}"));

    let mut diagnostic = Diagnostic::new(PrintfStringFormatting, expr.range());
    if checker.patch(diagnostic.kind.rule()) {
        #[allow(deprecated)]
        diagnostic.set_fix(Fix::unspecified(Edit::range_replacement(
            contents,
            expr.range(),
        )));
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
