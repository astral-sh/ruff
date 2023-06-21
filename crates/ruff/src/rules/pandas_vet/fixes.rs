use ruff_text_size::TextRange;
use rustpython_parser::ast::{self, Expr, Keyword, Ranged};

use ruff_diagnostics::{Edit, Fix};
use ruff_python_ast::source_code::Locator;

use crate::autofix::edits::remove_argument;

fn match_name(expr: &Expr) -> Option<&str> {
    if let Expr::Call(ast::ExprCall { func, .. }) = expr {
        if let Expr::Attribute(ast::ExprAttribute { value, .. }) = func.as_ref() {
            if let Expr::Name(ast::ExprName { id, .. }) = value.as_ref() {
                return Some(id);
            }
        }
    }
    None
}

/// Remove the `inplace` argument from a function call and replace it with an
/// assignment.
pub(super) fn convert_inplace_argument_to_assignment(
    locator: &Locator,
    expr: &Expr,
    violation_range: TextRange,
    args: &[Expr],
    keywords: &[Keyword],
) -> Option<Fix> {
    // Add the assignment.
    let name = match_name(expr)?;
    let insert_assignment = Edit::insertion(format!("{name} = "), expr.start());

    // Remove the `inplace` argument.
    let remove_argument = remove_argument(
        locator,
        expr.start(),
        violation_range,
        args,
        keywords,
        false,
    )
    .ok()?;
    Some(Fix::suggested_edits(insert_assignment, [remove_argument]))
}
