use ruff_python_ast::helpers::is_const_none;
use ruff_python_ast::{Arguments, Expr, ExprAttribute};

use crate::checkers::ast::Checker;

/// Check if the parent expression is a call to `astimezone`. This assumes that
/// the current expression is a `datetime.datetime` object.
pub(super) fn parent_expr_is_astimezone(checker: &Checker) -> bool {
    checker.semantic().current_expression_parent().is_some_and( |parent| {
        matches!(parent, Expr::Attribute(ExprAttribute { attr, .. }) if attr.as_str() == "astimezone")
    })
}

/// Return `true` if a keyword argument is present with a non-`None` value.
pub(super) fn has_non_none_keyword(arguments: &Arguments, keyword: &str) -> bool {
    arguments
        .find_keyword(keyword)
        .is_some_and(|keyword| !is_const_none(&keyword.value))
}
