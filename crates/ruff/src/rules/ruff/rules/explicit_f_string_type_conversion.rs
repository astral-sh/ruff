use anyhow::{bail, Result};
use libcst_native::{
    ConcatenatedString, Expression, FormattedStringContent, FormattedStringExpression,
};

use ruff_diagnostics::{AlwaysAutofixableViolation, Diagnostic, Edit, Fix};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::{self as ast, Arguments, Expr};
use ruff_python_codegen::Stylist;
use ruff_source_file::Locator;
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;
use crate::cst::matchers::{match_call_mut, match_name, transform_expression};
use crate::registry::AsRule;

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
#[violation]
pub struct ExplicitFStringTypeConversion;

impl AlwaysAutofixableViolation for ExplicitFStringTypeConversion {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Use explicit conversion flag")
    }

    fn autofix_title(&self) -> String {
        "Replace with conversion flag".to_string()
    }
}

/// RUF010
pub(crate) fn explicit_f_string_type_conversion(
    checker: &mut Checker,
    expr: &Expr,
    values: &[Expr],
) {
    for (index, formatted_value) in values
        .iter()
        .filter_map(|expr| {
            if let Expr::FormattedValue(expr) = &expr {
                Some(expr)
            } else {
                None
            }
        })
        .enumerate()
    {
        let ast::ExprFormattedValue {
            value, conversion, ..
        } = formatted_value;

        // Skip if there's already a conversion flag.
        if !conversion.is_none() {
            continue;
        }

        let Expr::Call(ast::ExprCall {
            func,
            arguments:
                Arguments {
                    args,
                    keywords,
                    range: _,
                },
            ..
        }) = value.as_ref()
        else {
            continue;
        };

        // Can't be a conversion otherwise.
        if !keywords.is_empty() {
            continue;
        }

        // Can't be a conversion otherwise.
        let [arg] = args.as_slice() else {
            continue;
        };

        // Avoid attempting to rewrite, e.g., `f"{str({})}"`; the curly braces are problematic.
        if matches!(
            arg,
            Expr::Dict(_) | Expr::Set(_) | Expr::DictComp(_) | Expr::SetComp(_)
        ) {
            continue;
        }

        let Expr::Name(ast::ExprName { id, .. }) = func.as_ref() else {
            continue;
        };

        if !matches!(id.as_str(), "str" | "repr" | "ascii") {
            continue;
        };

        if !checker.semantic().is_builtin(id) {
            continue;
        }

        let mut diagnostic = Diagnostic::new(ExplicitFStringTypeConversion, value.range());
        if checker.patch(diagnostic.kind.rule()) {
            diagnostic.try_set_fix(|| {
                convert_call_to_conversion_flag(expr, index, checker.locator(), checker.stylist())
            });
        }
        checker.diagnostics.push(diagnostic);
    }
}

/// Generate a [`Fix`] to replace an explicit type conversion with a conversion flag.
fn convert_call_to_conversion_flag(
    expr: &Expr,
    index: usize,
    locator: &Locator,
    stylist: &Stylist,
) -> Result<Fix> {
    let source_code = locator.slice(expr);
    transform_expression(source_code, stylist, |mut expression| {
        // Replace the formatted call expression at `index` with a conversion flag.
        let formatted_string_expression = match_part(index, &mut expression)?;
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
            _ => bail!("Unexpected function call: `{:?}`", name.value),
        }
        formatted_string_expression.expression = call.args[0].value.clone();
        Ok(expression)
    })
    .map(|output| Fix::automatic(Edit::range_replacement(output, expr.range())))
}

/// Return the [`FormattedStringContent`] at the given index in an f-string or implicit
/// string concatenation.
fn match_part<'a, 'b>(
    index: usize,
    expr: &'a mut Expression<'b>,
) -> Result<&'a mut FormattedStringExpression<'b>> {
    match expr {
        Expression::ConcatenatedString(expr) => Ok(collect_parts(expr).remove(index)),
        Expression::FormattedString(expr) => {
            // Find the formatted expression at the given index. The `parts` field contains a mix
            // of string literals and expressions, but our `index` only counts expressions. All
            // the boxing and mutability makes this difficult to write in a functional style.
            let mut format_index = 0;
            for part in &mut expr.parts {
                if let FormattedStringContent::Expression(expr) = part {
                    if format_index == index {
                        return Ok(expr);
                    }
                    format_index += 1;
                }
            }

            bail!("Index out of bounds: `{index}`")
        }
        _ => bail!("Unexpected expression: `{:?}`", expr),
    }
}

/// Given an implicit string concatenation, return a list of all the formatted expressions.
fn collect_parts<'a, 'b>(
    expr: &'a mut ConcatenatedString<'b>,
) -> Vec<&'a mut FormattedStringExpression<'b>> {
    fn inner<'a, 'b>(
        string: &'a mut libcst_native::String<'b>,
        formatted_expressions: &mut Vec<&'a mut FormattedStringExpression<'b>>,
    ) {
        match string {
            libcst_native::String::Formatted(expr) => {
                for part in &mut expr.parts {
                    if let FormattedStringContent::Expression(expr) = part {
                        formatted_expressions.push(expr);
                    }
                }
            }
            libcst_native::String::Concatenated(expr) => {
                inner(&mut expr.left, formatted_expressions);
                inner(&mut expr.right, formatted_expressions);
            }
            libcst_native::String::Simple(_) => {}
        }
    }

    let mut formatted_expressions = vec![];
    inner(&mut expr.left, &mut formatted_expressions);
    inner(&mut expr.right, &mut formatted_expressions);
    formatted_expressions
}
