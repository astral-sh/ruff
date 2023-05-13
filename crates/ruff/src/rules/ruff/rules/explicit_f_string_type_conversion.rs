use anyhow::Result;
use libcst_native::{Codegen, CodegenState};
use ruff_python_ast::source_code::{Locator, Stylist};
use rustpython_parser::ast::{self, Expr, ExprKind};

use ruff_diagnostics::{AlwaysAutofixableViolation, Diagnostic, Edit, Fix};
use ruff_macros::{derive_message_formats, violation};

use crate::checkers::ast::Checker;
use crate::cst::matchers::{
    match_call, match_expression, match_formatted_string, match_formatted_string_expression,
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
    formatted_values: &[usize],
    locator: &Locator,
    stylist: &Stylist,
) -> Result<Fix> {
    // Replace the call node with its argument and a conversion flag.
    let range = expr.range();
    let content = locator.slice(range);
    let mut expression = match_expression(content)?;
    let formatted_string = match_formatted_string(&mut expression)?;

    for index in formatted_values {
        let mut formatted_string_expression =
            match_formatted_string_expression(&mut formatted_string.parts[*index])?;
        let call = match_call(&mut formatted_string_expression.expression)?;
        let name = match_name(&mut call.func)?;
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
            _ => unreachable!(),
        }
        formatted_string_expression.expression = call.args[0].value.clone();
    }

    let mut state = CodegenState {
        default_newline: &stylist.line_ending(),
        default_indent: stylist.indentation(),
        ..CodegenState::default()
    };
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
    let mut formatted_values: Vec<usize> = vec![];
    for (index, value) in values.iter().enumerate() {
        let ExprKind::FormattedValue(ast::ExprFormattedValue {
            conversion,
            value,
            ..
        }) = &value.node else {
            continue;
        };
        // Skip if there's already a conversion flag.
        if *conversion != ast::ConversionFlag::None as u32 {
            return;
        }

        let ExprKind::Call(ast::ExprCall {
            func,
            args,
            keywords,
        }) = &value.node else {
            return;
        };

        // Can't be a conversion otherwise.
        if args.len() != 1 || !keywords.is_empty() {
            return;
        }

        let ExprKind::Name(ast::ExprName { id, .. }) = &func.node else {
            return;
        };

        if !matches!(id.as_str(), "str" | "repr" | "ascii") {
            return;
        };

        if !checker.ctx.is_builtin(id) {
            return;
        }
        formatted_values.push(index);
    }

    if formatted_values.is_empty() {
        return;
    }

    let mut diagnostic = Diagnostic::new(ExplicitFStringTypeConversion, expr.range());

    if checker.patch(diagnostic.kind.rule()) {
        diagnostic.try_set_fix(|| {
            fix_explicit_f_string_type_conversion(
                expr,
                &formatted_values,
                checker.locator,
                checker.stylist,
            )
        });
    }

    checker.diagnostics.push(diagnostic);
}
