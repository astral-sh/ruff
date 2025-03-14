use ast::Expr;
use ruff_diagnostics::{AlwaysFixableViolation, Diagnostic, Edit, Fix};
use ruff_macros::{derive_message_formats, ViolationMetadata};
use ruff_python_ast as ast;
use ruff_python_ast::comparable::ComparableExpr;
use ruff_python_ast::parenthesize::parenthesized_range;
use ruff_python_ast::{ExprBinOp, ExprRef, Operator};
use ruff_text_size::{Ranged, TextRange};

use crate::checkers::ast::Checker;

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
    operator: AugmentedOperator,
}

impl AlwaysFixableViolation for NonAugmentedAssignment {
    #[derive_message_formats]
    fn message(&self) -> String {
        let NonAugmentedAssignment { operator } = self;
        format!("Use `{operator}` to perform an augmented assignment directly")
    }

    fn fix_title(&self) -> String {
        "Replace with augmented assignment".to_string()
    }
}

/// PLR6104
pub(crate) fn non_augmented_assignment(checker: &Checker, assign: &ast::StmtAssign) {
    // Ignore multiple assignment targets.
    let [target] = assign.targets.as_slice() else {
        return;
    };

    // Match, e.g., `x = x + 1`.
    let Expr::BinOp(value) = &*assign.value else {
        return;
    };

    let operator = AugmentedOperator::from(value.op);

    // Match, e.g., `x = x + 1`.
    if ComparableExpr::from(target) == ComparableExpr::from(&value.left) {
        let mut diagnostic = Diagnostic::new(NonAugmentedAssignment { operator }, assign.range());
        diagnostic.set_fix(Fix::unsafe_edit(augmented_assignment(
            checker,
            target,
            operator,
            &value.right,
            value,
            assign.range,
        )));
        checker.report_diagnostic(diagnostic);
        return;
    }

    // If the operator is commutative, match, e.g., `x = 1 + x`, but limit such matches to primitive
    // types.
    if operator.is_commutative()
        && (value.left.is_number_literal_expr() || value.left.is_boolean_literal_expr())
        && ComparableExpr::from(target) == ComparableExpr::from(&value.right)
    {
        let mut diagnostic = Diagnostic::new(NonAugmentedAssignment { operator }, assign.range());
        diagnostic.set_fix(Fix::unsafe_edit(augmented_assignment(
            checker,
            target,
            operator,
            &value.left,
            value,
            assign.range,
        )));
        checker.report_diagnostic(diagnostic);
    }
}

/// Generate a fix to convert an assignment statement to an augmented assignment.
///
/// For example, given `x = x + 1`, the fix would be `x += 1`.
fn augmented_assignment(
    checker: &Checker,
    target: &Expr,
    operator: AugmentedOperator,
    right_operand: &Expr,
    original_expr: &ExprBinOp,
    range: TextRange,
) -> Edit {
    let locator = checker.locator();

    let right_operand_ref = ExprRef::from(right_operand);
    let parent = original_expr.into();
    let comment_ranges = checker.comment_ranges();
    let source = checker.source();

    let right_operand_range =
        parenthesized_range(right_operand_ref, parent, comment_ranges, source)
            .unwrap_or(right_operand.range());
    let right_operand_expr = locator.slice(right_operand_range);

    let target_expr = locator.slice(target);

    let new_content = format!("{target_expr} {operator} {right_operand_expr}");

    Edit::range_replacement(new_content, range)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AugmentedOperator {
    Add,
    BitAnd,
    BitOr,
    BitXor,
    Div,
    FloorDiv,
    LShift,
    MatMult,
    Mod,
    Mult,
    Pow,
    RShift,
    Sub,
}

impl AugmentedOperator {
    /// Returns `true` if the operator is commutative.
    fn is_commutative(self) -> bool {
        matches!(
            self,
            Self::Add | Self::BitAnd | Self::BitOr | Self::BitXor | Self::Mult
        )
    }
}

impl From<Operator> for AugmentedOperator {
    fn from(value: Operator) -> Self {
        match value {
            Operator::Add => Self::Add,
            Operator::BitAnd => Self::BitAnd,
            Operator::BitOr => Self::BitOr,
            Operator::BitXor => Self::BitXor,
            Operator::Div => Self::Div,
            Operator::FloorDiv => Self::FloorDiv,
            Operator::LShift => Self::LShift,
            Operator::MatMult => Self::MatMult,
            Operator::Mod => Self::Mod,
            Operator::Mult => Self::Mult,
            Operator::Pow => Self::Pow,
            Operator::RShift => Self::RShift,
            Operator::Sub => Self::Sub,
        }
    }
}

impl std::fmt::Display for AugmentedOperator {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Add => f.write_str("+="),
            Self::BitAnd => f.write_str("&="),
            Self::BitOr => f.write_str("|="),
            Self::BitXor => f.write_str("^="),
            Self::Div => f.write_str("/="),
            Self::FloorDiv => f.write_str("//="),
            Self::LShift => f.write_str("<<="),
            Self::MatMult => f.write_str("@="),
            Self::Mod => f.write_str("%="),
            Self::Mult => f.write_str("*="),
            Self::Pow => f.write_str("**="),
            Self::RShift => f.write_str(">>="),
            Self::Sub => f.write_str("-="),
        }
    }
}
