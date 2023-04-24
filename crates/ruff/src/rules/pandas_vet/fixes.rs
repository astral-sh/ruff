use rustpython_parser::ast::{Expr, ExprKind, Keyword, Location};

use ruff_diagnostics::{Edit, Fix};
use ruff_python_ast::source_code::Locator;

use crate::autofix::actions::remove_argument;

fn match_name(expr: &Expr) -> Option<&str> {
    if let ExprKind::Call { func, .. } = &expr.node {
        if let ExprKind::Attribute { value, .. } = &func.node {
            if let ExprKind::Name { id, .. } = &value.node {
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
    violation_at: Location,
    violation_end: Location,
    args: &[Expr],
    keywords: &[Keyword],
) -> Option<Fix> {
    // Add the assignment.
    let name = match_name(expr)?;
    let insert_assignment = Edit::insertion(format!("{name} = "), expr.location);

    // Remove the `inplace` argument.
    let remove_argument = remove_argument(
        locator,
        expr.location,
        violation_at,
        violation_end,
        args,
        keywords,
        false,
    )
    .ok()?;

    Some(Fix::from_iter([insert_assignment, remove_argument]))
}
