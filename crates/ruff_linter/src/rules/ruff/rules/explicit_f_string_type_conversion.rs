use std::fmt::Display;

use anyhow::Result;

use libcst_native::{Expression, LeftParen, ParenthesizedNode, RightParen};
use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_python_ast::{self as ast, Expr};
use ruff_python_codegen::Stylist;
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;
use crate::cst::helpers::space;
use crate::cst::matchers::{
    match_call_mut, match_formatted_string, match_formatted_string_expression, match_name,
    transform_expression,
};
use crate::{Edit, Fix, FixAvailability, Locator, Violation};

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

impl Violation for ExplicitFStringTypeConversion {
    const FIX_AVAILABILITY: FixAvailability = FixAvailability::Sometimes;

    #[derive_message_formats]
    fn message(&self) -> String {
        "Use explicit conversion flag".to_string()
    }

    fn fix_title(&self) -> Option<String> {
        Some("Replace with conversion flag".to_string())
    }
}

/// RUF010
pub(crate) fn explicit_f_string_type_conversion(checker: &Checker, f_string: &ast::FString) {
    for (index, element) in f_string.elements.iter().enumerate() {
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
            convert_call_to_conversion_flag(
                element,
                f_string,
                index,
                checker.locator(),
                checker.stylist(),
            )
        });
    }
}

/// Generate a [`Fix`] to replace an explicit type conversion with a conversion flag.
fn convert_call_to_conversion_flag(
    element: &ast::InterpolatedStringElement,
    f_string: &ast::FString,
    index: usize,
    locator: &Locator,
    stylist: &Stylist,
) -> Result<Fix> {
    if element
        .as_interpolation()
        .is_some_and(|interpolation| interpolation.debug_text.is_some())
    {
        anyhow::bail!("Don't support fixing f-string with debug text!");
    }
    let source_code = locator.slice(f_string);
    transform_expression(source_code, stylist, |mut expression| {
        let formatted_string = match_formatted_string(&mut expression)?;
        // Replace the formatted call expression at `index` with a conversion flag.
        let formatted_string_expression =
            match_formatted_string_expression(&mut formatted_string.parts[index])?;
        let call = match_call_mut(&mut formatted_string_expression.expression)?;
        let name = match_name(&call.func)?;
        match name.value {
            "str" => {
                formatted_string_expression.conversion = Some("s");
            }
            "repr" => {
                formatted_string_expression.conversion = Some("r");
            }
            "ascii" => {
                formatted_string_expression.conversion = Some("a");
            }
            _ => anyhow::bail!("Unexpected function call: `{:?}`", name.value),
        }

        if contains_brace(&call.args[0].value) {
            formatted_string_expression.whitespace_before_expression = space();
        }

        formatted_string_expression.expression = if needs_paren(&call.args[0].value) {
            call.args[0]
                .value
                .clone()
                .with_parens(LeftParen::default(), RightParen::default())
        } else {
            call.args[0].value.clone()
        };

        Ok(expression)
    })
    .map(|output| Fix::safe_edit(Edit::range_replacement(output, f_string.range())))
}

fn contains_brace(expr: &Expression) -> bool {
    matches!(
        expr,
        Expression::Dict(_) | Expression::DictComp(_) | Expression::Set(_) | Expression::SetComp(_)
    )
}

fn needs_paren(expr: &Expression) -> bool {
    matches!(expr, Expression::Lambda(_) | Expression::NamedExpr(_))
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
