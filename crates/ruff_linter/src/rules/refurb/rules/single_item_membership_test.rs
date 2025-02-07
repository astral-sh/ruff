use ruff_diagnostics::{Applicability, Diagnostic, Edit, Fix, FixAvailability, Violation};
use ruff_macros::{derive_message_formats, ViolationMetadata};
use ruff_python_ast::helpers::generate_comparison;
use ruff_python_ast::{self as ast, CmpOp, Expr, ExprStringLiteral};
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;
use crate::fix::edits::pad;

/// ## What it does
/// Checks for membership tests against single-item containers.
///
/// ## Why is this bad?
/// Performing a membership test against a container (like a `list` or `set`)
/// with a single item is less readable and less efficient than comparing
/// against the item directly.
///
/// ## Example
/// ```python
/// 1 in [1]
/// ```
///
/// Use instead:
/// ```python
/// 1 == 1
/// ```
///
/// ## Fix safety
///
/// When the right-hand side is a string, the fix is marked as unsafe.
/// This is because `c in "a"` is true both when `c` is `"a"` and when `c` is the empty string,
/// so the fix can change the behavior of your program in these cases.
///
/// Additionally, if there are comments within the fix's range,
/// it will also be marked as unsafe.
///
/// ## References
/// - [Python documentation: Comparisons](https://docs.python.org/3/reference/expressions.html#comparisons)
/// - [Python documentation: Membership test operations](https://docs.python.org/3/reference/expressions.html#membership-test-operations)
#[derive(ViolationMetadata)]
pub(crate) struct SingleItemMembershipTest {
    membership_test: MembershipTest,
}

impl Violation for SingleItemMembershipTest {
    const FIX_AVAILABILITY: FixAvailability = FixAvailability::Sometimes;

    #[derive_message_formats]
    fn message(&self) -> String {
        "Membership test against single-item container".to_string()
    }

    fn fix_title(&self) -> Option<String> {
        let SingleItemMembershipTest { membership_test } = self;
        match membership_test {
            MembershipTest::In => Some("Convert to equality test".to_string()),
            MembershipTest::NotIn => Some("Convert to inequality test".to_string()),
        }
    }
}

/// FURB171
pub(crate) fn single_item_membership_test(
    checker: &Checker,
    expr: &Expr,
    left: &Expr,
    ops: &[CmpOp],
    comparators: &[Expr],
) {
    let ([op], [right]) = (ops, comparators) else {
        return;
    };

    // Ensure that the comparison is a membership test.
    let membership_test = match op {
        CmpOp::In => MembershipTest::In,
        CmpOp::NotIn => MembershipTest::NotIn,
        _ => return,
    };

    // Check if the right-hand side is a single-item object.
    let Some(item) = single_item(right) else {
        return;
    };

    let diagnostic = Diagnostic::new(SingleItemMembershipTest { membership_test }, expr.range());

    let edit = Edit::range_replacement(
        pad(
            generate_comparison(
                left,
                &[membership_test.replacement_op()],
                &[item.clone()],
                expr.into(),
                checker.comment_ranges(),
                checker.source(),
            ),
            expr.range(),
            checker.locator(),
        ),
        expr.range(),
    );

    let applicability =
        if right.is_string_literal_expr() || checker.comment_ranges().intersects(expr.range()) {
            Applicability::Unsafe
        } else {
            Applicability::Safe
        };

    let fix = Fix::applicable_edit(edit, applicability);

    checker.report_diagnostic(diagnostic.with_fix(fix));
}

/// Return the single item wrapped in `Some` if the expression contains a single
/// item, otherwise return `None`.
fn single_item(expr: &Expr) -> Option<&Expr> {
    match expr {
        Expr::List(ast::ExprList { elts, .. })
        | Expr::Tuple(ast::ExprTuple { elts, .. })
        | Expr::Set(ast::ExprSet { elts, .. }) => match elts.as_slice() {
            [Expr::Starred(_)] => None,
            [item] => Some(item),
            _ => None,
        },
        string_expr @ Expr::StringLiteral(ExprStringLiteral { value: string, .. })
            if string.chars().count() == 1 =>
        {
            Some(string_expr)
        }
        _ => None,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum MembershipTest {
    /// Ex) `1 in [1]`
    In,
    /// Ex) `1 not in [1]`
    NotIn,
}

impl MembershipTest {
    /// Returns the replacement comparison operator for this membership test.
    fn replacement_op(self) -> CmpOp {
        match self {
            MembershipTest::In => CmpOp::Eq,
            MembershipTest::NotIn => CmpOp::NotEq,
        }
    }
}
