use ruff_diagnostics::{Diagnostic, Edit, Fix, FixAvailability, Violation};
use ruff_macros::{derive_message_formats, ViolationMetadata};
use ruff_python_ast::{self as ast, Expr};
use ruff_python_trivia::PythonWhitespace;
use ruff_text_size::Ranged;
use std::borrow::Cow;

use crate::checkers::ast::Checker;

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
/// ## Example
/// ```python
/// Decimal("0")
/// Decimal(float("Infinity"))
/// ```
///
/// Use instead:
/// ```python
/// Decimal(0)
/// Decimal("Infinity")
/// ```
///
/// ## References
/// - [Python documentation: `decimal`](https://docs.python.org/3/library/decimal.html)
#[derive(ViolationMetadata)]
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

    let diagnostic = match value {
        Expr::StringLiteral(ast::ExprStringLiteral {
            value: str_literal, ..
        }) => {
            // Parse the inner string as an integer.
            //
            // For reference, a string argument to `Decimal` is parsed in CPython
            // using this regex:
            // https://github.com/python/cpython/blob/ac556a2ad1213b8bb81372fe6fb762f5fcb076de/Lib/_pydecimal.py#L6060-L6077
            // _after_ trimming whitespace from the string and removing all occurrences of "_".
            let mut trimmed = Cow::from(str_literal.to_str().trim_whitespace());
            if memchr::memchr(b'_', trimmed.as_bytes()).is_some() {
                trimmed = Cow::from(trimmed.replace('_', ""));
            }
            // Extract the unary sign, if any.
            let (unary, rest) = if let Some(trimmed) = trimmed.strip_prefix('+') {
                ("+", Cow::from(trimmed))
            } else if let Some(trimmed) = trimmed.strip_prefix('-') {
                ("-", Cow::from(trimmed))
            } else {
                ("", trimmed)
            };

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

            // If all the characters are zeros, then the value is zero.
            let rest = match (unary, rest.is_empty()) {
                // `Decimal("-0")` is not the same as `Decimal("0")`
                // so we return early.
                ("-", true) => {
                    return;
                }
                (_, true) => "0",
                _ => rest,
            };

            let replacement = format!("{unary}{rest}");
            let mut diagnostic = Diagnostic::new(
                VerboseDecimalConstructor {
                    replacement: replacement.clone(),
                },
                value.range(),
            );

            diagnostic.set_fix(Fix::safe_edit(Edit::range_replacement(
                replacement,
                value.range(),
            )));

            diagnostic
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
            let Some(float) = float.as_string_literal_expr() else {
                return;
            };

            let trimmed = float.value.to_str().trim();
            let mut matches_non_finite_keyword = false;
            for non_finite_keyword in [
                "inf",
                "+inf",
                "-inf",
                "infinity",
                "+infinity",
                "-infinity",
                "nan",
                "+nan",
                "-nan",
            ] {
                if trimmed.eq_ignore_ascii_case(non_finite_keyword) {
                    matches_non_finite_keyword = true;
                    break;
                }
            }
            if !matches_non_finite_keyword {
                return;
            }

            let mut replacement = checker.locator().slice(float).to_string();
            // `Decimal(float("-nan")) == Decimal("nan")`
            if trimmed.eq_ignore_ascii_case("-nan") {
                // Here we do not attempt to remove just the '-' character.
                // It may have been encoded (e.g. as '\N{hyphen-minus}')
                // in the original source slice, and the added complexity
                // does not make sense for this edge case.
                replacement = "\"nan\"".to_string();
            }
            let mut diagnostic = Diagnostic::new(
                VerboseDecimalConstructor {
                    replacement: replacement.clone(),
                },
                value.range(),
            );

            diagnostic.set_fix(Fix::safe_edit(Edit::range_replacement(
                replacement,
                value.range(),
            )));

            diagnostic
        }
        _ => {
            return;
        }
    };

    checker.report_diagnostic(diagnostic);
}

// ```console
// $ python
// >>> import sys
// >>> sys.int_info.str_digits_check_threshold
// 640
// ```
const PYTHONINTMAXSTRDIGITS: usize = 640;
