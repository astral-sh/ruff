use std::fmt::Display;

use anyhow::Result;

use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_python_ast::parenthesize::parenthesized_range;
use ruff_python_ast::{self as ast, Expr};
use ruff_python_parser::TokenKind;
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;
use crate::{AlwaysFixableViolation, Edit, Fix};

/// ## What it does
/// Checks for uses of `str()`, `repr()`, and `ascii()` as explicit type
/// conversions within f-strings.
///
/// ## Why is this bad?
/// f-strings support dedicated conversion flags for these types, which are
/// more succinct and idiomatic.
///
/// Note that, in many cases, calling `str()` within an f-string is
/// unnecessary and can be removed entirely, as the value will be converted
/// to a string automatically, the notable exception being for classes that
/// implement a custom `__format__` method.
///
/// ## Example
/// ```python
/// a = "some string"
/// f"{repr(a)}"
/// ```
///
/// Use instead:
/// ```python
/// a = "some string"
/// f"{a!r}"
/// ```
#[derive(ViolationMetadata)]
pub(crate) struct ExplicitFStringTypeConversion;

impl AlwaysFixableViolation for ExplicitFStringTypeConversion {
    #[derive_message_formats]
    fn message(&self) -> String {
        "Use explicit conversion flag".to_string()
    }

    fn fix_title(&self) -> String {
        "Replace with conversion flag".to_string()
    }
}

/// RUF010
pub(crate) fn explicit_f_string_type_conversion(checker: &Checker, f_string: &ast::FString) {
    for element in &f_string.elements {
        let Some(ast::InterpolatedElement {
            expression,
            conversion,
            ..
        }) = element.as_interpolation()
        else {
            continue;
        };

        // Skip if there's already a conversion flag.
        if !conversion.is_none() {
            continue;
        }

        let Expr::Call(call) = expression.as_ref() else {
            continue;
        };

        let Some(conversion) = checker
            .semantic()
            .resolve_builtin_symbol(&call.func)
            .and_then(Conversion::from_str)
        else {
            continue;
        };
        let arg = match conversion {
            // Handles the cases: `f"{str(object=arg)}"` and `f"{str(arg)}"`
            Conversion::Str if call.arguments.len() == 1 => {
                let Some(arg) = call.arguments.find_argument_value("object", 0) else {
                    continue;
                };
                arg
            }
            Conversion::Str | Conversion::Repr | Conversion::Ascii => {
                // Can't be a conversion otherwise.
                if !call.arguments.keywords.is_empty() {
                    continue;
                }

                // Can't be a conversion otherwise.
                let [arg] = call.arguments.args.as_ref() else {
                    continue;
                };
                arg
            }
        };

        // Suppress lint for starred expressions.
        if matches!(arg, Expr::Starred(_)) {
            return;
        }

        let mut diagnostic =
            checker.report_diagnostic(ExplicitFStringTypeConversion, expression.range());
        diagnostic.try_set_fix(|| {
            convert_call_to_conversion_flag(checker, conversion, element, call, arg)
        });
    }
}

/// Generate a [`Fix`] to replace an explicit type conversion with a conversion flag.
fn convert_call_to_conversion_flag(
    checker: &Checker,
    conversion: Conversion,
    element: &ast::InterpolatedStringElement,
    call: &ast::ExprCall,
    arg: &Expr,
) -> Result<Fix> {
    if element
        .as_interpolation()
        .is_some_and(|interpolation| interpolation.debug_text.is_some())
    {
        anyhow::bail!("Don't support fixing f-string with debug text!");
    }

    let arg_str = checker.locator().slice(arg);
    let contains_curly_brace = checker
        .tokens()
        .in_range(arg.range())
        .iter()
        .any(|token| token.kind() == TokenKind::Lbrace);

    let output = if contains_curly_brace {
        format!(" {arg_str}!{conversion}")
    } else if matches!(arg, Expr::Lambda(_) | Expr::Named(_)) {
        format!("({arg_str})!{conversion}")
    } else {
        format!("{arg_str}!{conversion}")
    };

    let replace_range = if let Some(range) = parenthesized_range(
        call.into(),
        element.into(),
        checker.comment_ranges(),
        checker.source(),
    ) {
        range
    } else {
        call.range()
    };

    Ok(Fix::safe_edit(Edit::range_replacement(
        output,
        replace_range,
    )))
}

/// Represents the three built-in Python conversion functions that can be replaced
/// with f-string conversion flags.
#[derive(Copy, Clone)]
enum Conversion {
    Ascii,
    Str,
    Repr,
}

impl Conversion {
    fn from_str(value: &str) -> Option<Self> {
        Some(match value {
            "ascii" => Self::Ascii,
            "str" => Self::Str,
            "repr" => Self::Repr,
            _ => return None,
        })
    }
}

impl Display for Conversion {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let value = match self {
            Conversion::Ascii => "a",
            Conversion::Str => "s",
            Conversion::Repr => "r",
        };
        write!(f, "{value}")
    }
}
