use ruff_macros::{ViolationMetadata, derive_message_formats};

use itertools::Itertools;
use ruff_python_ast::{self as ast, Expr};
use ruff_python_trivia::PythonWhitespace;
use ruff_text_size::Ranged;
use std::borrow::Cow;

use crate::checkers::ast::Checker;
use crate::linter::float::as_non_finite_float_string_literal;
use crate::{Edit, Fix, FixAvailability, Violation};

/// ## What it does
/// Checks for unnecessary string literal or float casts in `Decimal`
/// constructors.
///
/// ## Why is this bad?
/// The `Decimal` constructor accepts a variety of arguments, including
/// integers, floats, and strings. However, it's not necessary to cast
/// integer literals to strings when passing them to the `Decimal`.
///
/// Similarly, `Decimal` accepts `inf`, `-inf`, and `nan` as string literals,
/// so there's no need to wrap those values in a `float` call when passing
/// them to the `Decimal` constructor.
///
/// Prefer the more concise form of argument passing for `Decimal`
/// constructors, as it's more readable and idiomatic.
///
/// Note that this rule does not flag quoted float literals such as `Decimal("0.1")`, which will
/// produce a more precise `Decimal` value than the unquoted `Decimal(0.1)`.
///
/// ## Example
/// ```python
/// from decimal import Decimal
///
/// Decimal("0")
/// Decimal(float("Infinity"))
/// ```
///
/// Use instead:
/// ```python
/// from decimal import Decimal
///
/// Decimal(0)
/// Decimal("Infinity")
/// ```
///
/// ## References
/// - [Python documentation: `decimal`](https://docs.python.org/3/library/decimal.html)
#[derive(ViolationMetadata)]
#[violation_metadata(stable_since = "0.12.0")]
pub(crate) struct VerboseDecimalConstructor {
    replacement: String,
}

impl Violation for VerboseDecimalConstructor {
    const FIX_AVAILABILITY: FixAvailability = FixAvailability::Always;

    #[derive_message_formats]
    fn message(&self) -> String {
        "Verbose expression in `Decimal` constructor".to_string()
    }

    fn fix_title(&self) -> Option<String> {
        let VerboseDecimalConstructor { replacement } = self;
        Some(format!("Replace with `{replacement}`"))
    }
}

/// FURB157
pub(crate) fn verbose_decimal_constructor(checker: &Checker, call: &ast::ExprCall) {
    if !checker
        .semantic()
        .resolve_qualified_name(&call.func)
        .is_some_and(|qualified_name| matches!(qualified_name.segments(), ["decimal", "Decimal"]))
    {
        return;
    }

    // Decimal accepts arguments of the form: `Decimal(value='0', context=None)`
    let Some(value) = call.arguments.find_argument_value("value", 0) else {
        return;
    };

    match value {
        Expr::StringLiteral(ast::ExprStringLiteral {
            value: str_literal, ..
        }) => {
            // Parse the inner string as an integer.
            //
            // For reference, a string argument to `Decimal` is parsed in CPython
            // using this regex:
            // https://github.com/python/cpython/blob/ac556a2ad1213b8bb81372fe6fb762f5fcb076de/Lib/_pydecimal.py#L6060-L6077
            // _after_ trimming whitespace from the string and removing all occurrences of "_".
            let original_str = str_literal.to_str().trim_whitespace();
            // Extract the unary sign, if any.
            let (unary, original_str) = if let Some(trimmed) = original_str.strip_prefix('+') {
                ("+", trimmed)
            } else if let Some(trimmed) = original_str.strip_prefix('-') {
                ("-", trimmed)
            } else {
                ("", original_str)
            };
            let mut rest = Cow::from(original_str);
            let has_digit_separators = memchr::memchr(b'_', rest.as_bytes()).is_some();
            if has_digit_separators {
                rest = Cow::from(rest.replace('_', ""));
            }

            // Early return if we now have an empty string
            // or a very long string:
            if (rest.len() > PYTHONINTMAXSTRDIGITS) || (rest.is_empty()) {
                return;
            }

            // Skip leading zeros.
            let rest = rest.trim_start_matches('0');

            // Verify that the rest of the string is a valid integer.
            if !rest.bytes().all(|c| c.is_ascii_digit()) {
                return;
            }

            // If the original string had digit separators, normalize them
            let rest = if has_digit_separators {
                Cow::from(normalize_digit_separators(original_str))
            } else {
                Cow::from(rest)
            };

            // If all the characters are zeros, then the value is zero.
            let rest = match (unary, rest.is_empty()) {
                // `Decimal("-0")` is not the same as `Decimal("0")`
                // so we return early.
                ("-", true) => {
                    return;
                }
                (_, true) => "0",
                _ => &rest,
            };

            let replacement = format!("{unary}{rest}");

            let mut diagnostic = checker.report_diagnostic(
                VerboseDecimalConstructor {
                    replacement: replacement.clone(),
                },
                value.range(),
            );

            diagnostic.set_fix(Fix::safe_edit(Edit::range_replacement(
                replacement,
                value.range(),
            )));
        }
        Expr::Call(ast::ExprCall {
            func, arguments, ..
        }) => {
            // Must be a call to the `float` builtin.
            if !checker.semantic().match_builtin_expr(func, "float") {
                return;
            }

            // Must have exactly one argument, which is a string literal.
            if !arguments.keywords.is_empty() {
                return;
            }
            let [float] = arguments.args.as_ref() else {
                return;
            };
            let Some(float_str) = as_non_finite_float_string_literal(float) else {
                return;
            };

            let mut replacement = checker.locator().slice(float).to_string();
            // `Decimal(float("-nan")) == Decimal("nan")`
            if float_str == "-nan" {
                // Here we do not attempt to remove just the '-' character.
                // It may have been encoded (e.g. as '\N{hyphen-minus}')
                // in the original source slice, and the added complexity
                // does not make sense for this edge case.
                replacement = "\"nan\"".to_string();
            }
            let mut diagnostic = checker.report_diagnostic(
                VerboseDecimalConstructor {
                    replacement: replacement.clone(),
                },
                value.range(),
            );

            diagnostic.set_fix(Fix::safe_edit(Edit::range_replacement(
                replacement,
                value.range(),
            )));
        }
        _ => {}
    }
}

/// Normalizes digit separators in a numeric string by:
/// - Stripping leading and trailing underscores
/// - Collapsing medial underscore sequences to single underscores
fn normalize_digit_separators(original_str: &str) -> String {
    // Strip leading and trailing underscores
    let trimmed = original_str
        .trim_start_matches(['_', '0'])
        .trim_end_matches('_');

    // Collapse medial underscore sequences to single underscores
    trimmed
        .chars()
        .dedup_by(|a, b| *a == '_' && a == b)
        .collect()
}

// ```console
// $ python
// >>> import sys
// >>> sys.int_info.str_digits_check_threshold
// 640
// ```
const PYTHONINTMAXSTRDIGITS: usize = 640;
