use std::borrow::Cow;
use std::fmt::Write;
use std::str::FromStr;

use ruff_diagnostics::{Diagnostic, Edit, Fix, FixAvailability, Violation};
use ruff_macros::{derive_message_formats, ViolationMetadata};
use ruff_python_ast::{self as ast, whitespace::indentation, AnyStringFlags, Expr, StringFlags};
use ruff_python_codegen::Stylist;
use ruff_python_literal::cformat::{
    CConversionFlags, CFormatPart, CFormatPrecision, CFormatQuantity, CFormatString,
};
use ruff_python_parser::TokenKind;
use ruff_python_stdlib::identifiers::is_identifier;
use ruff_source_file::LineRanges;
use ruff_text_size::{Ranged, TextRange};

use crate::checkers::ast::Checker;
use crate::rules::pyupgrade::helpers::curly_escape;
use crate::Locator;

/// ## What it does
/// Checks for `printf`-style string formatting, and offers to replace it with
/// `str.format` calls.
///
/// ## Why is this bad?
/// `printf`-style string formatting has a number of quirks, and leads to less
/// readable code than using `str.format` calls or f-strings. In general, prefer
/// the newer `str.format` and f-strings constructs over `printf`-style string
/// formatting.
///
/// ## Example
///
/// ```python
/// "%s, %s" % ("Hello", "World")  # "Hello, World"
/// ```
///
/// Use instead:
///
/// ```python
/// "{}, {}".format("Hello", "World")  # "Hello, World"
/// ```
///
/// ```python
/// f"{'Hello'}, {'World'}"  # "Hello, World"
/// ```
///
/// ## Fix safety
///
/// In cases where the format string contains a single generic format specifier
/// (e.g. `%s`), and the right-hand side is an ambiguous expression,
/// we cannot offer a safe fix.
///
/// For example, given:
///
/// ```python
/// "%s" % val
/// ```
///
/// `val` could be a single-element tuple, _or_ a single value (not
/// contained in a tuple). Both of these would resolve to the same
/// formatted string when using `printf`-style formatting, but
/// resolve differently when using f-strings:
///
/// ```python
/// val = 1
/// print("%s" % val)  # "1"
/// print("{}".format(val))  # "1"
///
/// val = (1,)
/// print("%s" % val)  # "1"
/// print("{}".format(val))  # "(1,)"
/// ```
///
/// ## References
/// - [Python documentation: `printf`-style String Formatting](https://docs.python.org/3/library/stdtypes.html#old-string-formatting)
/// - [Python documentation: `str.format`](https://docs.python.org/3/library/stdtypes.html#str.format)
#[derive(ViolationMetadata)]
pub(crate) struct PrintfStringFormatting;

impl Violation for PrintfStringFormatting {
    const FIX_AVAILABILITY: FixAvailability = FixAvailability::Sometimes;

    #[derive_message_formats]
    fn message(&self) -> String {
        "Use format specifiers instead of percent format".to_string()
    }

    fn fix_title(&self) -> Option<String> {
        Some("Replace with format specifiers".to_string())
    }
}

fn simplify_conversion_flag(flags: CConversionFlags) -> String {
    let mut flag_string = String::new();
    if flags.intersects(CConversionFlags::LEFT_ADJUST) {
        flag_string.push('<');
    }
    if flags.intersects(CConversionFlags::SIGN_CHAR) {
        flag_string.push('+');
    }
    if flags.intersects(CConversionFlags::ALTERNATE_FORM) {
        flag_string.push('#');
    }
    if flags.intersects(CConversionFlags::BLANK_SIGN) {
        if !flags.intersects(CConversionFlags::SIGN_CHAR) {
            flag_string.push(' ');
        }
    }
    if flags.intersects(CConversionFlags::ZERO_PAD) {
        if !flags.intersects(CConversionFlags::LEFT_ADJUST) {
            flag_string.push('0');
        }
    }
    flag_string
}

/// Convert a [`PercentFormat`] struct into a `String`.
fn handle_part(part: &CFormatPart<String>) -> Cow<'_, str> {
    match part {
        CFormatPart::Literal(item) => curly_escape(item),
        CFormatPart::Spec(spec) => {
            let mut format_string = String::new();

            // TODO(charlie): What case is this?
            if spec.format_char == '%' {
                format_string.push('%');
                return Cow::Owned(format_string);
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
                                // Integer-only presentation types.
                                //
                                // See: https://docs.python.org/3/library/string.html#format-specification-mini-language
                                if matches!(
                                    spec.format_char,
                                    'b' | 'c' | 'd' | 'o' | 'x' | 'X' | 'n'
                                ) {
                                    format_string.push('0');
                                    format_string.push_str(&amount.to_string());
                                } else {
                                    format_string.push('.');
                                    format_string.push_str(&amount.to_string());
                                }
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
            Cow::Owned(format_string)
        }
    }
}

/// Convert a [`CFormatString`] into a `String`.
fn percent_to_format(format_string: &CFormatString) -> String {
    format_string
        .iter()
        .map(|(_, part)| handle_part(part))
        .collect()
}

/// If a tuple has one argument, remove the comma; otherwise, return it as-is.
fn clean_params_tuple<'a>(right: &Expr, locator: &Locator<'a>) -> Cow<'a, str> {
    if let Expr::Tuple(tuple) = &right {
        if tuple.len() == 1 {
            if !locator.contains_line_break(right.range()) {
                let mut contents = locator.slice(right).to_string();
                for (i, character) in contents.chars().rev().enumerate() {
                    if character == ',' {
                        let correct_index = contents.len() - i - 1;
                        contents.remove(correct_index);
                        break;
                    }
                }
                return Cow::Owned(contents);
            }
        }
    }

    Cow::Borrowed(locator.slice(right))
}

/// Converts a dictionary to a function call while preserving as much styling as
/// possible.
fn clean_params_dictionary(right: &Expr, locator: &Locator, stylist: &Stylist) -> Option<String> {
    let is_multi_line = locator.contains_line_break(right.range());
    let mut contents = String::new();
    if let Expr::Dict(ast::ExprDict { items, range: _ }) = &right {
        let mut arguments: Vec<String> = vec![];
        let mut seen: Vec<&str> = vec![];
        let mut indent = None;
        for ast::DictItem { key, value } in items {
            if let Some(key) = key {
                if let Expr::StringLiteral(ast::ExprStringLiteral {
                    value: key_string, ..
                }) = key
                {
                    // If the dictionary key is not a valid variable name, abort.
                    if !is_identifier(key_string.to_str()) {
                        return None;
                    }
                    // If there are multiple entries of the same key, abort.
                    if seen.contains(&key_string.to_str()) {
                        return None;
                    }
                    seen.push(key_string.to_str());
                    if is_multi_line {
                        if indent.is_none() {
                            indent = indentation(locator.contents(), key);
                        }
                    }

                    let value_string = locator.slice(value);
                    arguments.push(format!("{key_string}={value_string}"));
                } else {
                    // If there are any non-string keys, abort.
                    return None;
                }
            } else {
                let value_string = locator.slice(value);
                arguments.push(format!("**{value_string}"));
            }
        }
        // If we couldn't parse out key values, abort.
        if arguments.is_empty() {
            return None;
        }
        contents.push('(');
        if is_multi_line {
            let indent = indent?;

            for item in &arguments {
                contents.push_str(stylist.line_ending().as_str());
                contents.push_str(indent);
                contents.push_str(item);
                contents.push(',');
            }

            contents.push_str(stylist.line_ending().as_str());

            // For the ending parentheses, go back one indent.
            let default_indent: &str = stylist.indentation();
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
        if fmt.mapping_key.as_ref().is_some_and(String::is_empty) {
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
        if let Expr::Dict(_) = &params {
            if fmt.mapping_key.is_none() {
                return false;
            }
        }
    }
    true
}

/// UP031
pub(crate) fn printf_string_formatting(
    checker: &Checker,
    bin_op: &ast::ExprBinOp,
    string_expr: &ast::ExprStringLiteral,
) {
    let right = &*bin_op.right;

    let mut num_positional_arguments = 0;
    let mut num_keyword_arguments = 0;
    let mut format_strings: Vec<(TextRange, String)> =
        Vec::with_capacity(string_expr.value.as_slice().len());

    // Parse each string segment.
    for string_literal in &string_expr.value {
        let string = checker.locator().slice(string_literal);
        let flags = AnyStringFlags::from(string_literal.flags);
        let string = &string
            [usize::from(flags.opener_len())..(string.len() - usize::from(flags.closer_len()))];

        // Parse the format string (e.g. `"%s"`) into a list of `PercentFormat`.
        let Ok(format_string) = CFormatString::from_str(string) else {
            return;
        };
        if !convertible(&format_string, right) {
            checker.report_diagnostic(Diagnostic::new(PrintfStringFormatting, string_expr.range()));
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
        format_strings.push((
            string_literal.range(),
            flags
                .display_contents(&percent_to_format(&format_string))
                .to_string(),
        ));
    }

    // Parse the parameters.
    let params_string = match right {
        Expr::StringLiteral(_)
        | Expr::BytesLiteral(_)
        | Expr::NumberLiteral(_)
        | Expr::BooleanLiteral(_)
        | Expr::NoneLiteral(_)
        | Expr::EllipsisLiteral(_)
        | Expr::FString(_) => Cow::Owned(format!("({})", checker.locator().slice(right))),
        Expr::Name(_) | Expr::Attribute(_) | Expr::Subscript(_) | Expr::Call(_) => {
            if num_keyword_arguments > 0 {
                // If we have _any_ named fields, assume the right-hand side is a mapping.
                Cow::Owned(format!("(**{})", checker.locator().slice(right)))
            } else if num_positional_arguments > 1 {
                // If we have multiple fields, but no named fields, assume the right-hand side is a
                // tuple.
                Cow::Owned(format!("(*{})", checker.locator().slice(right)))
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
                // So we offer an unsafe fix:
                Cow::Owned(format!("({})", checker.locator().slice(right)))
            }
        }
        Expr::Tuple(_) => clean_params_tuple(right, checker.locator()),
        Expr::Dict(_) => {
            let Some(params_string) =
                clean_params_dictionary(right, checker.locator(), checker.stylist())
            else {
                checker.report_diagnostic(Diagnostic::new(
                    PrintfStringFormatting,
                    string_expr.range(),
                ));
                return;
            };
            Cow::Owned(params_string)
        }
        _ => return,
    };

    // Reconstruct the string.
    let mut contents = String::new();
    let mut prev_end = None;
    for (range, format_string) in format_strings {
        // Add the content before the string segment.
        match prev_end {
            None => {
                contents.push_str(
                    checker
                        .locator()
                        .slice(TextRange::new(bin_op.start(), range.start())),
                );
            }
            Some(prev_end) => {
                contents.push_str(
                    checker
                        .locator()
                        .slice(TextRange::new(prev_end, range.start())),
                );
            }
        }
        // Add the string itself.
        contents.push_str(&format_string);
        prev_end = Some(range.end());
    }

    if let Some(prev_end) = prev_end {
        for token in checker.tokens().after(prev_end) {
            match token.kind() {
                // If we hit a right paren, we have to preserve it.
                TokenKind::Rpar => {
                    contents.push_str(
                        checker
                            .locator()
                            .slice(TextRange::new(prev_end, token.end())),
                    );
                }
                // Break as soon as we find the modulo symbol.
                TokenKind::Percent => break,
                _ => {}
            }
        }
    }

    // Add the `.format` call.
    let _ = write!(&mut contents, ".format{params_string}");

    let mut diagnostic = Diagnostic::new(PrintfStringFormatting, bin_op.range());
    diagnostic.set_fix(Fix::unsafe_edit(Edit::range_replacement(
        contents,
        bin_op.range(),
    )));
    checker.report_diagnostic(diagnostic);
}

#[cfg(test)]
mod tests {
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
