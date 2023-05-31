use anyhow::{bail, Result};
use libcst_native::Codegen;
use rustpython_parser::ast::{self, Expr, Ranged};

use ruff_diagnostics::{AlwaysAutofixableViolation, Diagnostic, Edit, Fix};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::source_code::{Locator, Stylist};

use crate::checkers::ast::Checker;
use crate::cst::matchers::{
    match_call_mut, match_expression, match_formatted_string, match_formatted_string_expression,
    match_name,
};
use crate::registry::AsRule;

/// ## What it does
/// Checks for usages of `str()`, `repr()`, and `ascii()` as explicit type
/// conversions within f-strings.
///
/// ## Why is this bad?
/// f-strings support dedicated conversion flags for these types, which are
/// more succinct and idiomatic.
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
        format!("Use conversion in f-string")
    }

    fn autofix_title(&self) -> String {
        "Replace f-string function call with conversion".to_string()
    }
}

fn fix_explicit_f_string_type_conversion(
    expr: &Expr,
    index: usize,
    locator: &Locator,
    stylist: &Stylist,
) -> Result<Fix> {
    // Replace the call node with its argument and a conversion flag.
    let range = expr.range();
    let content = locator.slice(range);
    let mut expression = match_expression(content)?;
    let formatted_string = match_formatted_string(&mut expression)?;

    // Replace the formatted call expression at `index` with a conversion flag.
    let mut formatted_string_expression =
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

    let mut state = stylist.codegen_state();
    expression.codegen(&mut state);

    Ok(Fix::automatic(Edit::range_replacement(
        state.to_string(),
        range,
    )))
}

/// RUF010
pub(crate) fn explicit_f_string_type_conversion(
    checker: &mut Checker,
    expr: &Expr,
    values: &[Expr],
) {
    for (index, formatted_value) in values.iter().enumerate() {
        let Expr::FormattedValue(ast::ExprFormattedValue {
            conversion,
            value,
            ..
        }) = &formatted_value else {
            continue;
        };
        // Skip if there's already a conversion flag.
        if !conversion.is_none() {
            return;
        }

        let Expr::Call(ast::ExprCall {
            func,
            args,
            keywords,
            ..
        }) = value.as_ref() else {
            return;
        };

        // Can't be a conversion otherwise.
        if args.len() != 1 || !keywords.is_empty() {
            return;
        }

        let Expr::Name(ast::ExprName { id, .. }) = func.as_ref() else {
            return;
        };

        if !matches!(id.as_str(), "str" | "repr" | "ascii") {
            return;
        };

        if !checker.semantic_model().is_builtin(id) {
            return;
        }

        let mut diagnostic = Diagnostic::new(ExplicitFStringTypeConversion, value.range());
        if checker.patch(diagnostic.kind.rule()) {
            diagnostic.try_set_fix(|| {
                fix_explicit_f_string_type_conversion(expr, index, checker.locator, checker.stylist)
            });
        }
        checker.diagnostics.push(diagnostic);
    }
}
