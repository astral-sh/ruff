use anyhow::{bail, Result};
use libcst_native::{
    ConcatenatedString, Expression, FormattedStringContent, FormattedStringExpression,
};
use rustpython_parser::ast::{self, Expr, Ranged};

use ruff_diagnostics::{AlwaysAutofixableViolation, Diagnostic, Edit, Fix};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::prelude::ConversionFlag;
use ruff_python_ast::source_code::{Locator, Stylist};

use crate::autofix::codemods::CodegenStylist;
use crate::checkers::ast::Checker;
use crate::cst::matchers::{match_call_mut, match_expression, match_name};
use crate::registry::AsRule;

/// ## What it does
/// Checks for uses of `str()`, `repr()`, and `ascii()` as explicit type
/// conversions within f-strings.
///
/// ## Why is this bad?
/// f-strings support dedicated conversion flags for these types, which are
/// more succinct and idiomatic.
///
/// In the case of `str()`, it's also redundant, since `str()` is the default
/// conversion.
///
/// ## Example
/// ```python
/// a = "some string"
/// f"{str(a)}"
/// f"{repr(a)}"
/// ```
///
/// Use instead:
/// ```python
/// a = "some string"
/// f"{a}"
/// f"{a!r}"
/// ```
#[violation]
pub struct ExplicitFStringTypeConversion {
    operation: Operation,
}

impl AlwaysAutofixableViolation for ExplicitFStringTypeConversion {
    #[derive_message_formats]
    fn message(&self) -> String {
        let ExplicitFStringTypeConversion { operation } = self;
        match operation {
            Operation::ConvertCallToConversionFlag => {
                format!("Use explicit conversion flag")
            }
            Operation::RemoveCall => format!("Remove unnecessary `str` conversion"),
            Operation::RemoveConversionFlag => format!("Remove unnecessary conversion flag"),
        }
    }

    fn autofix_title(&self) -> String {
        let ExplicitFStringTypeConversion { operation } = self;
        match operation {
            Operation::ConvertCallToConversionFlag => {
                format!("Replace with conversion flag")
            }
            Operation::RemoveCall => format!("Remove `str` call"),
            Operation::RemoveConversionFlag => format!("Remove conversion flag"),
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
enum Operation {
    /// Ex) Convert `f"{repr(bla)}"` to `f"{bla!r}"`
    ConvertCallToConversionFlag,
    /// Ex) Convert `f"{bla!s}"` to `f"{bla}"`
    RemoveConversionFlag,
    /// Ex) Convert `f"{str(bla)}"` to `f"{bla}"`
    RemoveCall,
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
            value,
            conversion,
            format_spec,
            range: _,
        } = formatted_value;

        match conversion {
            ConversionFlag::Ascii | ConversionFlag::Repr => {
                // Nothing to do.
                continue;
            }
            ConversionFlag::Str => {
                // Skip if there's a format spec.
                if format_spec.is_some() {
                    continue;
                }

                // Remove the conversion flag entirely.
                // Ex) `f"{bla!s}"`
                let mut diagnostic = Diagnostic::new(
                    ExplicitFStringTypeConversion {
                        operation: Operation::RemoveConversionFlag,
                    },
                    value.range(),
                );
                if checker.patch(diagnostic.kind.rule()) {
                    diagnostic.try_set_fix(|| {
                        remove_conversion_flag(expr, index, checker.locator, checker.stylist)
                    });
                }
                checker.diagnostics.push(diagnostic);
            }
            ConversionFlag::None => {
                // Replace with the appropriate conversion flag.
                let Expr::Call(ast::ExprCall {
                    func,
                    args,
                    keywords,
                    ..
                }) = value.as_ref() else {
                    continue;
                };

                // Can't be a conversion otherwise.
                if args.len() != 1 || !keywords.is_empty() {
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

                if id == "str" && format_spec.is_none() {
                    // Ex) `f"{str(bla)}"`
                    let mut diagnostic = Diagnostic::new(
                        ExplicitFStringTypeConversion {
                            operation: Operation::RemoveCall,
                        },
                        value.range(),
                    );
                    if checker.patch(diagnostic.kind.rule()) {
                        diagnostic.try_set_fix(|| {
                            remove_conversion_call(expr, index, checker.locator, checker.stylist)
                        });
                    }
                    checker.diagnostics.push(diagnostic);
                } else {
                    // Ex) `f"{repr(bla)}"`
                    let mut diagnostic = Diagnostic::new(
                        ExplicitFStringTypeConversion {
                            operation: Operation::ConvertCallToConversionFlag,
                        },
                        value.range(),
                    );
                    if checker.patch(diagnostic.kind.rule()) {
                        diagnostic.try_set_fix(|| {
                            convert_call_to_conversion_flag(
                                expr,
                                index,
                                checker.locator,
                                checker.stylist,
                            )
                        });
                    }
                    checker.diagnostics.push(diagnostic);
                }
            }
        }
    }
}

/// Generate a [`Fix`] to remove a conversion flag from a formatted expression.
fn remove_conversion_flag(
    expr: &Expr,
    index: usize,
    locator: &Locator,
    stylist: &Stylist,
) -> Result<Fix> {
    // Parenthesize the expression, to support implicit concatenation.
    let range = expr.range();
    let content = locator.slice(range);
    let parenthesized_content = format!("({content})");
    let mut expression = match_expression(&parenthesized_content)?;

    // Replace the formatted call expression at `index` with a conversion flag.
    let formatted_string_expression = match_part(index, &mut expression)?;
    formatted_string_expression.conversion = None;

    // Remove the parentheses (first and last characters).
    let mut content = expression.codegen_stylist(stylist);
    content.remove(0);
    content.pop();

    Ok(Fix::automatic(Edit::range_replacement(content, range)))
}

/// Generate a [`Fix`] to remove a call from a formatted expression.
fn remove_conversion_call(
    expr: &Expr,
    index: usize,
    locator: &Locator,
    stylist: &Stylist,
) -> Result<Fix> {
    // Parenthesize the expression, to support implicit concatenation.
    let range = expr.range();
    let content = locator.slice(range);
    let parenthesized_content = format!("({content})");
    let mut expression = match_expression(&parenthesized_content)?;

    // Replace the formatted call expression at `index` with a conversion flag.
    let formatted_string_expression = match_part(index, &mut expression)?;
    let call = match_call_mut(&mut formatted_string_expression.expression)?;
    formatted_string_expression.expression = call.args[0].value.clone();

    // Remove the parentheses (first and last characters).
    let mut content = expression.codegen_stylist(stylist);
    content.remove(0);
    content.pop();

    Ok(Fix::automatic(Edit::range_replacement(content, range)))
}

/// Generate a [`Fix`] to replace an explicit type conversion with a conversion flag.
fn convert_call_to_conversion_flag(
    expr: &Expr,
    index: usize,
    locator: &Locator,
    stylist: &Stylist,
) -> Result<Fix> {
    // Parenthesize the expression, to support implicit concatenation.
    let range = expr.range();
    let content = locator.slice(range);
    let parenthesized_content = format!("({content})");
    let mut expression = match_expression(&parenthesized_content)?;

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

    // Remove the parentheses (first and last characters).
    let mut content = expression.codegen_stylist(stylist);
    content.remove(0);
    content.pop();

    Ok(Fix::automatic(Edit::range_replacement(content, range)))
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
