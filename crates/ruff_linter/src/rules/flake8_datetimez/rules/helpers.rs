use ruff_python_ast::{Expr, ExprAttribute};

use crate::checkers::ast::Checker;

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub(super) enum DatetimeModuleAntipattern {
    NoTzArgumentPassed,
    NonePassedToTzArgument,
}

/// Check if the parent expression is a call to `astimezone`. This assumes that
/// the current expression is a `datetime.datetime` object.
pub(super) fn parent_expr_is_astimezone(checker: &Checker) -> bool {
    checker.semantic().current_expression_parent().is_some_and( |parent| {
        matches!(parent, Expr::Attribute(ExprAttribute { attr, .. }) if attr.as_str() == "astimezone")
    })
}
