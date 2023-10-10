use ruff_python_ast::comparable::ComparableExpr;
use ruff_python_ast::{self as ast, BoolOp, CmpOp, Expr};

use ruff_diagnostics::{AlwaysFixableViolation, Diagnostic, Edit, Fix};
use ruff_macros::{derive_message_formats, violation};
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;
use crate::registry::AsRule;

/// ## What it does
/// TODO
///
/// ## Why is this bad?
/// TODO
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
        format!("Replace `{key} in {dict}` with `{dict}.get({key})`")
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
        left,
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

    let [comparator] = comparators.as_slice() else {
        return;
    };

    let Expr::Subscript(ast::ExprSubscript { value, slice, .. }) = right else {
        return;
    };

    if ComparableExpr::from(comparator) == ComparableExpr::from(value)
        && ComparableExpr::from(left) == ComparableExpr::from(slice)
    {
        let mut diagnostic = Diagnostic::new(
            UnnecessaryKeyCheck {
                key: checker.generator().expr(left),
                dict: checker.generator().expr(value),
            },
            expr.range(),
        );
        if checker.patch(diagnostic.kind.rule()) {
            diagnostic.set_fix(Fix::safe_edit(Edit::range_replacement(
                format!(
                    "{}.get({})",
                    checker.generator().expr(value),
                    checker.generator().expr(left)
                ),
                expr.range(),
            )));
        }
        checker.diagnostics.push(diagnostic);
    }
}
