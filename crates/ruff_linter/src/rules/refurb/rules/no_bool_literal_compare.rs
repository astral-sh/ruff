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
    if compare.ops.len() != 1 {
        // don't do chained comparisons
        return;
    }
    let comparator = compare.left.as_ref();

    let [op, ..] = &*compare.ops else {
        return;
    };

    let [next, ..] = &*compare.comparators else {
        return;
    };

    // we'll try to determine what they're doing, whether or not they're using a yoda comparison
    let (expr, boolvalue, in_parentheses) = match (comparator, next) {
        (Expr::BooleanLiteral(_), Expr::BooleanLiteral(_)) => {
            // they're comparing two bools, so we can't do anything here
            return;
        }
        (
            Expr::BooleanLiteral(ast::ExprBooleanLiteral {
                value: boolvalue,
                range: boolrange,
            }),
            expr,
        ) => {
            let in_parentheses =
                compare.start() != boolrange.start() || compare.end() != expr.end();
            (expr, boolvalue, in_parentheses)
        }
        (
            expr,
            Expr::BooleanLiteral(ast::ExprBooleanLiteral {
                value: boolvalue,
                range: boolrange,
            }),
        ) => {
            let in_parentheses =
                compare.start() != expr.start() || compare.end() != boolrange.end();
            (expr, boolvalue, in_parentheses)
        }
        _ => {
            return;
        }
    };

    let (lparen, rparen) = if in_parentheses { ("(", ")") } else { ("", "") };

    let (content, check_type) = match (op, boolvalue) {
        (ast::CmpOp::Is | ast::CmpOp::Eq, true)
        | (ast::CmpOp::IsNot | ast::CmpOp::NotEq, false) => (
            format!("{lparen}{}{rparen}", checker.generator().expr(expr)),
            CheckType::Truthy,
        ),
        (ast::CmpOp::Is | ast::CmpOp::Eq, false)
        | (ast::CmpOp::IsNot | ast::CmpOp::NotEq, true) => (
            format!("{lparen}not {}{rparen}", checker.generator().expr(expr)),
            CheckType::Falsey,
        ),
        _ => return,
    };

    let mut diagnostic = Diagnostic::new(BoolLiteralCompare { check_type }, compare.range());

    let edit = Edit::range_replacement(content, compare.range());

    let fix = match check_type {
        CheckType::Truthy => Fix::unsafe_edit(edit),
        CheckType::Falsey => Fix::unsafe_edit(edit),
    };

    diagnostic.set_fix(fix);

    checker.diagnostics.push(diagnostic);
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CheckType {
    Truthy,
    Falsey,
}
