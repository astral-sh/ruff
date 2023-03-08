use anyhow::{bail, Result};
use libcst_native::{Codegen, CodegenState, Expression, GeneratorExp};

use ruff_diagnostics::Fix;
use ruff_python_ast::source_code::{Locator, Stylist};

use crate::cst::matchers::{match_call, match_expression};

/// (PIE802) Convert `[i for i in a]` into `i for i in a`
pub fn fix_unnecessary_comprehension_any_all(
    locator: &Locator,
    stylist: &Stylist,
    expr: &rustpython_parser::ast::Expr,
) -> Result<Fix> {
    // Expr(ListComp) -> Expr(GeneratorExp)
    let expression_text = locator.slice(expr);
    let mut tree = match_expression(expression_text)?;
    let call = match_call(&mut tree)?;

    let Expression::ListComp(list_comp) = &call.args[0].value else {
        bail!(
            "Expected Expression::ListComp"
        );
    };

    call.args[0].value = Expression::GeneratorExp(Box::new(GeneratorExp {
        elt: list_comp.elt.clone(),
        for_in: list_comp.for_in.clone(),
        lpar: list_comp.lpar.clone(),
        rpar: list_comp.rpar.clone(),
    }));

    if let Some(comma) = &call.args[0].comma {
        call.args[0].whitespace_after_arg = comma.whitespace_after.clone();
        call.args[0].comma = None;
    }

    let mut state = CodegenState {
        default_newline: stylist.line_ending(),
        default_indent: stylist.indentation(),
        ..CodegenState::default()
    };
    tree.codegen(&mut state);

    Ok(Fix::replacement(
        state.to_string(),
        expr.location,
        expr.end_location.unwrap(),
    ))
}
