use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_python_ast::helpers::generate_comparison;
use ruff_python_ast::{self as ast, CmpOp, Expr, ExprStringLiteral};
use ruff_python_semantic::SemanticModel;
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;
use crate::fix::edits::pad;
use crate::{Edit, Fix, FixAvailability, Violation};

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
/// The fix is always marked as unsafe.
///
/// When the right-hand side is a string, this fix can change the behavior of your program.
/// This is because `c in "a"` is true both when `c` is `"a"` and when `c` is the empty string.
///
/// Additionally, converting `in`/`not in` against a single-item container to `==`/`!=` can
/// change runtime behavior: `in` may consider identity (e.g., `NaN`) and always
/// yields a `bool`.
///
/// Comments within the replacement range will also be removed.
///
/// ## References
/// - [Python documentation: Comparisons](https://docs.python.org/3/reference/expressions.html#comparisons)
/// - [Python documentation: Membership test operations](https://docs.python.org/3/reference/expressions.html#membership-test-operations)
#[derive(ViolationMetadata)]
#[violation_metadata(preview_since = "v0.1.0")]
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

    // Check if the right-hand side is a single-item object
    let Some(item) = single_item(right, checker.semantic()) else {
        return;
    };

    let edit = Edit::range_replacement(
        pad(
            generate_comparison(
                left,
                &[membership_test.replacement_op()],
                std::slice::from_ref(item),
                expr.into(),
                checker.comment_ranges(),
                checker.source(),
            ),
            expr.range(),
            checker.locator(),
        ),
        expr.range(),
    );

    // All supported cases can change runtime behavior; mark as unsafe.
    let fix = Fix::unsafe_edit(edit);

    checker
        .report_diagnostic(SingleItemMembershipTest { membership_test }, expr.range())
        .set_fix(fix);
}

/// Return the single item wrapped in `Some` if the expression contains a single
/// item, otherwise return `None`.
fn single_item<'a>(expr: &'a Expr, semantic: &'a SemanticModel) -> Option<&'a Expr> {
    match expr {
        Expr::List(ast::ExprList { elts, .. })
        | Expr::Tuple(ast::ExprTuple { elts, .. })
        | Expr::Set(ast::ExprSet { elts, .. }) => match elts.as_slice() {
            [Expr::Starred(_)] => None,
            [item] => Some(item),
            _ => None,
        },
        Expr::Call(ast::ExprCall {
            func,
            arguments,
            range: _,
            node_index: _,
        }) => {
            if arguments.len() != 1 || !is_set_method(func, semantic) {
                return None;
            }

            arguments
                .find_positional(0)
                .and_then(|arg| single_item(arg, semantic))
        }
        string_expr @ Expr::StringLiteral(ExprStringLiteral { value: string, .. })
            if string.chars().count() == 1 =>
        {
            Some(string_expr)
        }
        _ => None,
    }
}

fn is_set_method(func: &Expr, semantic: &SemanticModel) -> bool {
    ["set", "frozenset"]
        .iter()
        .any(|s| semantic.match_builtin_expr(func, s))
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
