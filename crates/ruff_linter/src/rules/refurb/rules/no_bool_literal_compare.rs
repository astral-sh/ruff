use ruff_diagnostics::{AlwaysFixableViolation, Diagnostic, Edit, Fix};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::{self as ast, Expr};
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;

/// ## What it does
/// Checks for uses of long-form truthy and falsey checks.
///
/// ## Why is this bad?
/// Python has a shorthand for checking if a value is truthy or falsey, which
/// is more concise and idiomatic.
///
/// ## Example
/// ```python
/// failed = True
/// if failed == True:
///     print("failed")
/// # or
/// if failed is True:
///     print("failed")
/// # or
/// if failed == False:
///     print("passed")
/// # or
/// if failed == False:
///     print("passed")
/// ```
///
/// Use instead:
/// ```python
/// failed = True
/// if failed:  # for truthy checks
///     print("failed")
/// # or
/// if not failed:  # for falsey checks
///     print("passed")
/// ```
///
#[violation]
pub struct BoolLiteralCompare {
    check_type: CheckType,
}

impl AlwaysFixableViolation for BoolLiteralCompare {
    #[derive_message_formats]
    fn message(&self) -> String {
        let BoolLiteralCompare { check_type } = self;

        match check_type {
            CheckType::Truthy => format!("Comparison to `True` should be `cond`"),
            CheckType::Falsey => format!("Comparison to `False` should be `not cond`"),
        }
    }

    fn fix_title(&self) -> String {
        let BoolLiteralCompare { check_type } = self;

        match check_type {
            CheckType::Truthy => format!("Use shorthand truthy check"),
            CheckType::Falsey => format!("Use shorthand falsey check"),
        }
    }
}

/// FURB149
pub(crate) fn bool_literal_compare(checker: &mut Checker, compare: &ast::ExprCompare) {
    let comparator = compare.left.as_ref();
    let [op, ..] = compare.ops.as_slice() else {
        return;
    };
    let [next, ..] = compare.comparators.as_slice() else {
        return;
    };

    // we'll try to determine what they're doing, whether or not they're using a yoda comparison
    let (expr, boolvalue) = match (comparator, next) {
        (
            Expr::BooleanLiteral(ast::ExprBooleanLiteral { .. }),
            Expr::BooleanLiteral(ast::ExprBooleanLiteral { .. }),
        ) => {
            // they're comparing two bools, so we can't do anything here
            return;
        }
        (
            Expr::BooleanLiteral(ast::ExprBooleanLiteral {
                value: boolvalue, ..
            }),
            expr,
        )
        | (
            expr,
            Expr::BooleanLiteral(ast::ExprBooleanLiteral {
                value: boolvalue, ..
            }),
        ) => (expr, boolvalue),
        _ => {
            return;
        }
    };

    let (content, check_type) = match (op, boolvalue) {
        (ast::CmpOp::Is | ast::CmpOp::Eq, true)
        | (ast::CmpOp::IsNot | ast::CmpOp::NotEq, false) => {
            (checker.generator().expr(expr), CheckType::Truthy)
        }

        (ast::CmpOp::Is | ast::CmpOp::Eq, false)
        | (ast::CmpOp::IsNot | ast::CmpOp::NotEq, true) => (
            format!("not {}", checker.generator().expr(expr)),
            CheckType::Falsey,
        ),
        _ => return,
    };

    let mut diagnostic = Diagnostic::new(BoolLiteralCompare { check_type }, compare.range());

    diagnostic.set_fix(Fix::unsafe_edit(Edit::range_replacement(
        content,
        compare.range(),
    )));

    checker.diagnostics.push(diagnostic);
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CheckType {
    Truthy,
    Falsey,
}
