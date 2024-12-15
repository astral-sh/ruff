use ast::Expr;
use ruff_diagnostics::{AlwaysFixableViolation, Diagnostic, Edit, Fix};
use ruff_macros::{derive_message_formats, ViolationMetadata};
use ruff_python_ast as ast;
use ruff_python_ast::comparable::ComparableExpr;
use ruff_python_ast::Operator;
use ruff_text_size::TextRange;

use crate::checkers::ast::Checker;
use crate::Locator;

/// ## What it does
/// Checks for assignments that can be replaced with augmented assignment
/// statements.
///
/// ## Why is this bad?
/// If the right-hand side of an assignment statement consists of a binary
/// operation in which one operand is the same as the assignment target,
/// it can be rewritten as an augmented assignment. For example, `x = x + 1`
/// can be rewritten as `x += 1`.
///
/// When performing such an operation, an augmented assignment is more concise
/// and idiomatic.
///
/// ## Known problems
/// In some cases, this rule will not detect assignments in which the target
/// is on the right-hand side of a binary operation (e.g., `x = y + x`, as
/// opposed to `x = x + y`), as such operations are not commutative for
/// certain data types, like strings.
///
/// For example, `x = "prefix-" + x` is not equivalent to `x += "prefix-"`,
/// while `x = 1 + x` is equivalent to `x += 1`.
///
/// If the type of the left-hand side cannot be trivially inferred, the rule
/// will ignore the assignment.
///
/// ## Example
/// ```python
/// x = x + 1
/// ```
///
/// Use instead:
/// ```python
/// x += 1
/// ```
///
/// ## Fix safety
/// This rule's fix is marked as unsafe, as augmented assignments have
/// different semantics when the target is a mutable data type, like a list or
/// dictionary.
///
/// For example, consider the following:
///
/// ```python
/// foo = [1]
/// bar = foo
/// foo = foo + [2]
/// assert (foo, bar) == ([1, 2], [1])
/// ```
///
/// If the assignment is replaced with an augmented assignment, the update
/// operation will apply to both `foo` and `bar`, as they refer to the same
/// object:
///
/// ```python
/// foo = [1]
/// bar = foo
/// foo += [2]
/// assert (foo, bar) == ([1, 2], [1, 2])
/// ```
#[derive(ViolationMetadata)]
pub(crate) struct NonAugmentedAssignment {
    op_repr: String,
}

impl AlwaysFixableViolation for NonAugmentedAssignment {
    #[derive_message_formats]
    fn message(&self) -> String {
        let op_repr = &self.op_repr;
        format!("Use `{op_repr}=` to perform an augmented assignment directly")
    }

    fn fix_title(&self) -> String {
        "Replace with augmented assignment".to_string()
    }
}

/// PLR6104
pub(crate) fn non_augmented_assignment(checker: &mut Checker, assign: &ast::StmtAssign) {
    // Ignore multiple assignment targets.
    let [target] = assign.targets.as_slice() else {
        return;
    };

    let Expr::BinOp(expr) = &*assign.value else {
        return;
    };
    let (left, op, right) = (&expr.left, &expr.op, &expr.right);

    let op_repr = op.to_string();
    let Some(value) = augmentable_assignment_value(target, left, *op, right) else {
        return;
    };

    let locator = checker.locator();
    let range = assign.range;
    let fix = replace_with_augmented_assignment_fix(locator, range, target, &op_repr, value);

    let diagnostic = Diagnostic::new(NonAugmentedAssignment { op_repr }, range);

    checker.diagnostics.push(diagnostic.with_fix(fix));
}

fn augmentable_assignment_value<'a>(
    target: &'a Expr,
    left: &'a Expr,
    op: Operator,
    right: &'a Expr,
) -> Option<&'a Expr> {
    let comp_target = ComparableExpr::from(target);
    let comp_left = ComparableExpr::from(left);
    let comp_right = ComparableExpr::from(right);

    if comp_target == comp_left {
        return Some(right);
    }

    if !operator_is_commutative(op) {
        return None;
    }

    if comp_target != comp_right {
        return None;
    }

    if left.is_number_literal_expr() || left.is_boolean_literal_expr() {
        Some(left)
    } else {
        None
    }
}

fn operator_is_commutative(op: Operator) -> bool {
    matches!(
        op,
        Operator::Add | Operator::BitAnd | Operator::BitOr | Operator::BitXor | Operator::Mult
    )
}

fn replace_with_augmented_assignment_fix(
    locator: &Locator,
    range: TextRange,
    target: &Expr,
    op: &str,
    value: &Expr,
) -> Fix {
    let target_expr = locator.slice(target);
    let value_expr = locator.slice(value);

    let new_value_expr = if should_be_parenthesized_when_standalone(value) {
        format!("({value_expr})")
    } else {
        value_expr.to_string()
    };
    let new_content = format!("{target_expr} {op}= {new_value_expr}");
    let edit = Edit::range_replacement(new_content, range);

    Fix::unsafe_edit(edit)
}

/// Whether `expr` should be parenthesized when used on its own.
///
/// ```python
/// a := 0            # (a := 0)
/// a = b := 0        # a = (b := 0)
/// ```
const fn should_be_parenthesized_when_standalone(expr: &Expr) -> bool {
    matches!(expr, Expr::Named(_))
}
