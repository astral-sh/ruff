use anyhow::{bail, Result};
use libcst_native::{Codegen, CodegenState, Expression, GeneratorExp};

use crate::ast::types::Range;
use crate::cst::matchers::{match_expr, match_module};
use crate::fix::Fix;
use crate::source_code::{Locator, Stylist};

/// (PIE802) Convert `[i for i in a]` into `i for i in a`
pub fn fix_unnecessary_comprehension_any_all(
    locator: &Locator,
    stylist: &Stylist,
    expr: &rustpython_parser::ast::Expr,
) -> Result<Fix> {
    // Expr(ListComp) -> Expr(GeneratorExp)
    let module_text = locator.slice(&Range::from_located(expr));
    let mut tree = match_module(module_text)?;
    let mut body = match_expr(&mut tree)?;

    let Expression::ListComp(list_comp) = &body.value else {
        bail!(
            "Expected Expression::ListComp"
        );
    };

    body.value = Expression::GeneratorExp(Box::new(GeneratorExp {
        elt: list_comp.elt.clone(),
        for_in: list_comp.for_in.clone(),
        lpar: list_comp.lpar.clone(),
        rpar: list_comp.rpar.clone(),
    }));

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
