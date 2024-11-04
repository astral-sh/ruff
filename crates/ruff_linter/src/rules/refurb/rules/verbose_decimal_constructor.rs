use ruff_diagnostics::{Diagnostic, Edit, Fix, FixAvailability, Violation};
use ruff_macros::{derive_message_formats, violation};
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
#[violation]
pub struct VerboseDecimalConstructor {
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
pub(crate) fn verbose_decimal_constructor(checker: &mut Checker, call: &ast::ExprCall) {
    if !checker
        .semantic()
        .resolve_qualified_name(&call.func)
        .is_some_and(|qualified_name| matches!(qualified_name.segments(), ["decimal", "Decimal"]))
    {
        return;
    }

    // Decimal accepts arguments of the form: `Decimal(value='0', context=None)`
    let Some(value) = call.arguments.find_argument("value", 0) else {
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
            let (unary, mut rest) = if let Some(trimmed) = trimmed.strip_prefix('+') {
                ("+", Cow::from(trimmed))
            } else if let Some(trimmed) = trimmed.strip_prefix('-') {
                ("-", Cow::from(trimmed))
            } else {
                ("", trimmed)
            };
            // Extract the exponent, if any.
            let mut e_indices = memchr::memrchr2_iter(b'e', b'E', rest.as_bytes());
            if let Some(index) = e_indices.next() {
                // More than one `e` is an error of type `decimal.InvalidOperation`
                // when calling Decimal on a string. The suggested
                // fix would turn this into a SyntaxError. To maintain
                // the behavior of the code, we abort the check in this case.
                if e_indices.next().is_some() {
                    return;
                }
                // This range will not cause a panic: in the worst case,
                // 'e' was the last character in `rest`, and then the right
                // hand side will be `""`.
                let exponent = rest[index + 1..]
                    .strip_prefix('+')
                    .unwrap_or(&rest[index + 1..]);
                // Verify that the exponent is a nonnegative integer
                if !exponent.bytes().all(|c| c.is_ascii_digit()) {
                    return;
                };
                // NB: We need not convert, e.g., `2e3` to `2000`.
                // However, `2e+3` is a syntax error, so we remove
                // the sign on the exponent.
                rest = Cow::from(rest.replace('+', ""));
            }

            // Skip leading zeros.
            let rest = rest.trim_start_matches('0');

            // Verify that the rest of the string is a valid integer.
            // NB: We have already checked that there is at most one 'e'.
            if !rest
                .bytes()
                .all(|c| c.is_ascii_digit() || c == b'e' || c == b'E')
            {
                return;
            };

            // If all the characters are zeros, then the value is zero.
            let rest = if rest.is_empty() { "0" } else { rest };

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
            };

            // Must have exactly one argument, which is a string literal.
            if arguments.keywords.len() != 0 {
                return;
            };
            let [float] = arguments.args.as_ref() else {
                return;
            };
            let Some(float) = float.as_string_literal_expr() else {
                return;
            };
            if !matches!(
                float.value.to_str().to_lowercase().as_str(),
                "inf" | "-inf" | "infinity" | "-infinity" | "nan"
            ) {
                return;
            }

            let replacement = checker.locator().slice(float).to_string();
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

    checker.diagnostics.push(diagnostic);
}

// // Slightly modified from [CPython regex] to ignore  https://github.com/python/cpython/blob/ac556a2ad1213b8bb81372fe6fb762f5fcb076de/Lib/_pydecimal.py#L6060-L6077
// static DECIMAL_PARSER_REGEX: LazyLock<Regex> = LazyLock::new(|| {
//     Regex::new(
//         r"(?x)                   # Verbose mode for comments
// ^                                             # Start of string
// (?P<sign>[-+])?                               # Optional sign
// (?:
//     (?P<int>\d*)                              # Integer part (can be empty)
//     (\.(?P<frac>\d+))?                        # Optional fractional part
//     (E(?P<exp>[-+]?\d+))?                     # Optional exponent
// |
//     Inf(inity)?                               # Infinity
// |
//     (?P<signal>s)?                            # Optional signal
//     NaN                                       # NaN
//     (?P<diag>\d*)                             # Optional diagnostic info
// )
// $                                             # End of string
// ",
//     )
//     .unwrap()
// });
