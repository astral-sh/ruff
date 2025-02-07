use anyhow::{bail, Result};

use ruff_diagnostics::{AlwaysFixableViolation, Diagnostic, Edit, Fix};
use ruff_macros::{derive_message_formats, ViolationMetadata};
use ruff_python_ast::{self as ast, Arguments, Expr};
use ruff_python_codegen::Stylist;
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;
use crate::cst::matchers::{
    match_call_mut, match_formatted_string, match_formatted_string_expression, match_name,
    transform_expression,
};
use crate::Locator;

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
    for (index, element) in f_string.elements.iter().enumerate() {
        let Some(ast::FStringExpressionElement {
            expression,
            conversion,
            ..
        }) = element.as_expression()
        else {
            continue;
        };

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
        }) = expression.as_ref()
        else {
            continue;
        };

        // Can't be a conversion otherwise.
        if !keywords.is_empty() {
            continue;
        }

        // Can't be a conversion otherwise.
        let [arg] = &**args else {
            continue;
        };

        // Avoid attempting to rewrite, e.g., `f"{str({})}"`; the curly braces are problematic.
        if matches!(
            arg,
            Expr::Dict(_) | Expr::Set(_) | Expr::DictComp(_) | Expr::SetComp(_)
        ) {
            continue;
        }

        if !checker
            .semantic()
            .resolve_builtin_symbol(func)
            .is_some_and(|builtin| matches!(builtin, "str" | "repr" | "ascii"))
        {
            continue;
        }

        let mut diagnostic = Diagnostic::new(ExplicitFStringTypeConversion, expression.range());
        diagnostic.try_set_fix(|| {
            convert_call_to_conversion_flag(f_string, index, checker.locator(), checker.stylist())
        });
        checker.report_diagnostic(diagnostic);
    }
}

/// Generate a [`Fix`] to replace an explicit type conversion with a conversion flag.
fn convert_call_to_conversion_flag(
    f_string: &ast::FString,
    index: usize,
    locator: &Locator,
    stylist: &Stylist,
) -> Result<Fix> {
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
            _ => bail!("Unexpected function call: `{:?}`", name.value),
        }
        formatted_string_expression.expression = call.args[0].value.clone();
        Ok(expression)
    })
    .map(|output| Fix::safe_edit(Edit::range_replacement(output, f_string.range())))
}
