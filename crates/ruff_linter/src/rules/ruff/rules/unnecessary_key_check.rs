use ruff_python_ast::comparable::ComparableExpr;
use ruff_python_ast::{self as ast, BoolOp, CmpOp, Expr};

use ruff_diagnostics::{AlwaysFixableViolation, Diagnostic, Edit, Fix};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::helpers::contains_effect;
use ruff_python_ast::parenthesize::parenthesized_range;
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;

/// ## What it does
/// Checks for unnecessary key checks prior to accessing a dictionary.
///
/// ## Why is this bad?
/// When working with dictionaries, the `get` can be used to access a value
/// without having to check if the dictionary contains the relevant key,
/// returning `None` if the key is not present.
///
/// ## Examples
/// ```python
/// if "key" in dct and dct["key"]:
///     ...
/// ```
///
/// Use instead:
/// ```python
/// if dct.get("key"):
///     ...
/// ```
#[violation]
pub struct UnnecessaryKeyCheck;

impl AlwaysFixableViolation for UnnecessaryKeyCheck {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Unnecessary key check before dictionary access")
    }

    fn fix_title(&self) -> String {
        format!("Replace with `dict.get`")
    }
}

/// RUF019
pub(crate) fn unnecessary_key_check(checker: &mut Checker, expr: &Expr) {
    if !checker.semantic().in_boolean_test() {
        return;
    }

    let Expr::BoolOp(ast::ExprBoolOp {
        op: BoolOp::And,
        values,
        ..
    }) = expr
    else {
        return;
    };

    let [left, right] = values.as_slice() else {
        return;
    };

    // Left should be, e.g., `key in dct`.
    let Expr::Compare(ast::ExprCompare {
        left: key_left,
        ops,
        comparators,
        ..
    }) = left
    else {
        return;
    };

    if !matches!(&**ops, [CmpOp::In]) {
        return;
    }

    let [obj_left] = &**comparators else {
        return;
    };

    // Right should be, e.g., `dct[key]`.
    let Expr::Subscript(ast::ExprSubscript {
        value: obj_right,
        slice: key_right,
        ..
    }) = right
    else {
        return;
    };

    if ComparableExpr::from(obj_left) != ComparableExpr::from(obj_right)
        || ComparableExpr::from(key_left) != ComparableExpr::from(key_right)
    {
        return;
    }

    if contains_effect(obj_left, |id| checker.semantic().has_builtin_binding(id))
        || contains_effect(key_left, |id| checker.semantic().has_builtin_binding(id))
    {
        return;
    }

    let mut diagnostic = Diagnostic::new(UnnecessaryKeyCheck, expr.range());
    diagnostic.set_fix(Fix::safe_edit(Edit::range_replacement(
        format!(
            "{}.get({})",
            checker.locator().slice(
                parenthesized_range(
                    obj_right.into(),
                    right.into(),
                    checker.comment_ranges(),
                    checker.locator().contents(),
                )
                .unwrap_or(obj_right.range())
            ),
            checker.locator().slice(
                parenthesized_range(
                    key_right.into(),
                    right.into(),
                    checker.comment_ranges(),
                    checker.locator().contents(),
                )
                .unwrap_or(key_right.range())
            ),
        ),
        expr.range(),
    )));
    checker.diagnostics.push(diagnostic);
}
