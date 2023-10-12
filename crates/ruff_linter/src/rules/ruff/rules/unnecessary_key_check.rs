use ruff_python_ast::comparable::ComparableExpr;
use ruff_python_ast::{self as ast, BoolOp, CmpOp, Expr};

use ruff_diagnostics::{AlwaysFixableViolation, Diagnostic, Edit, Fix};
use ruff_macros::{derive_message_formats, violation};
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;
use crate::registry::AsRule;

/// ## What it does
/// Checks for unnecessary key check before subscripting a dictionary.
///
/// ## Why is this bad?
/// `get` can be used to get a value from a dictionary without having to check
/// if the key exists first.
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
pub struct UnnecessaryKeyCheck {
    key: String,
    dict: String,
}

impl AlwaysFixableViolation for UnnecessaryKeyCheck {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Unnecessary key check")
    }

    fn fix_title(&self) -> String {
        let UnnecessaryKeyCheck { key, dict } = self;
        format!("Replace with `{dict}.get({key})`")
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

    let Expr::Compare(ast::ExprCompare {
        left: key_left,
        ops,
        comparators,
        ..
    }) = left
    else {
        return;
    };

    if !matches!(ops.as_slice(), [CmpOp::In]) {
        return;
    }

    let [obj_left] = comparators.as_slice() else {
        return;
    };

    let Expr::Subscript(ast::ExprSubscript {
        value: obj_right,
        slice: key_right,
        ..
    }) = right
    else {
        return;
    };

    if ComparableExpr::from(obj_left) == ComparableExpr::from(obj_right)
        && ComparableExpr::from(key_left) == ComparableExpr::from(key_right)
    {
        let mut diagnostic = Diagnostic::new(
            UnnecessaryKeyCheck {
                key: checker.generator().expr(key_left),
                dict: checker.generator().expr(obj_left),
            },
            expr.range(),
        );
        if checker.patch(diagnostic.kind.rule()) {
            diagnostic.set_fix(Fix::safe_edit(Edit::range_replacement(
                format!(
                    "{}.get({})",
                    checker.generator().expr(obj_left),
                    checker.generator().expr(key_left)
                ),
                expr.range(),
            )));
        }
        checker.diagnostics.push(diagnostic);
    }
}
