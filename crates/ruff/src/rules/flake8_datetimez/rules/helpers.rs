use ruff_python_ast::{Expr, ExprAttribute};

use crate::checkers::ast::Checker;

/// Check if the parent expression is a call to `astimezone`. This assumes that
/// the current expression is a `datetime.datetime` object.
pub(crate) fn parent_expr_is_astimezone(checker: &Checker) -> bool {
    checker.semantic().expr_parent().map_or(false, |parent| {
        matches!(parent, Expr::Attribute(ExprAttribute { attr, .. }) if attr.as_str() == "astimezone")
    })
}
