use ruff_python_ast::{AnyNodeRef, Expr, ExprAttribute, ExprCall};

use crate::checkers::ast::Checker;

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub(super) enum DatetimeModuleAntipattern {
    NoTzArgumentPassed,
    NonePassedToTzArgument,
}

/// Check if the current expression is followed by
/// a chain of `.replace()` calls followed by `.astimezone`.
///
/// This assumes that the current expression is a `datetime.datetime` object.
pub(super) fn parent_expr_is_astimezone(checker: &Checker) -> bool {
    let semantic = checker.semantic();
    let mut last = None;

    for (index, expr) in semantic.current_expressions().enumerate() {
        if index == 0 {
            // datetime.now(...).replace(...).astimezone
            // ^^^^^^^^^^^^^^^^^
            continue;
        }

        if index % 2 == 1 {
            // datetime.now(...).replace(...).astimezone
            //                   ^^^^^^^      ^^^^^^^^^^
            let Expr::Attribute(ExprAttribute { attr, .. }) = expr else {
                return false;
            };

            match attr.as_str() {
                "replace" => last = Some(AnyNodeRef::from(expr)),
                "astimezone" => return true,
                _ => return false,
            }
        } else {
            // datetime.now(...).replace(...).astimezone
            //                          ^^^^^
            let Expr::Call(ExprCall { func, .. }) = expr else {
                return false;
            };

            if !last.is_some_and(|it| it.ptr_eq(AnyNodeRef::from(&**func))) {
                return false;
            }
        }
    }

    false
}
