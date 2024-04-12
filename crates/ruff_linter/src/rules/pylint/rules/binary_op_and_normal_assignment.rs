use ast::{Expr, StmtAugAssign};
use ruff_diagnostics::{Diagnostic, Edit, Fix, FixAvailability, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast as ast;
use ruff_python_ast::comparable::ComparableExpr;
use ruff_python_ast::Operator;
use ruff_python_codegen::Generator;
use ruff_text_size::{Ranged, TextRange};

use crate::checkers::ast::Checker;

/// ## What it does
/// Checks for assignments that can be replaced with augmented assignment
/// statements.
///
/// ## Why is this bad?
/// If an assignment statement consists of a binary operation in which one
/// operand is the same as the assignment target, it can be rewritten as an
/// augmented assignment. For example, `x = x + 1` can be rewritten as
/// `x += 1`.
///
/// When performing such an operation, augmented assignments are more concise
/// and idiomatic.
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
#[violation]
pub struct BinaryOpAndNormalAssignment {
    operator: AugmentedOperator,
}

impl Violation for BinaryOpAndNormalAssignment {
    const FIX_AVAILABILITY: FixAvailability = FixAvailability::Sometimes;

    #[derive_message_formats]
    fn message(&self) -> String {
        let BinaryOpAndNormalAssignment { operator } = self;
        format!("Use `{operator}` to perform an augmented assignment directly")
    }

    fn fix_title(&self) -> Option<String> {
        Some("Replace with augmented assignment".to_string())
    }
}

/// PLR6104
pub(crate) fn binary_op_and_normal_assignment(checker: &mut Checker, assign: &ast::StmtAssign) {
    // Ignore multiple assignment targets.
    let [target] = assign.targets.as_slice() else {
        return;
    };

    // Match, e.g., `x = x + 1`.
    let Expr::BinOp(value) = &*assign.value else {
        return;
    };

    // Match, e.g., `x = x + 1`.
    if ComparableExpr::from(target) == ComparableExpr::from(&value.left) {
        let mut diagnostic = Diagnostic::new(
            BinaryOpAndNormalAssignment {
                operator: AugmentedOperator::from(value.op),
            },
            assign.range(),
        );
        diagnostic.set_fix(Fix::unsafe_edit(augmented_assignment(
            checker.generator(),
            target,
            value.op,
            &value.right,
            assign.range(),
        )));
        checker.diagnostics.push(diagnostic);
        return;
    }

    // Match, e.g., `x = 1 + x`.
    if ComparableExpr::from(target) == ComparableExpr::from(&value.right) {
        let mut diagnostic = Diagnostic::new(
            BinaryOpAndNormalAssignment {
                operator: AugmentedOperator::from(value.op),
            },
            assign.range(),
        );
        diagnostic.set_fix(Fix::unsafe_edit(augmented_assignment(
            checker.generator(),
            target,
            value.op,
            &value.left,
            assign.range(),
        )));
        checker.diagnostics.push(diagnostic);
        return;
    }
}

/// Generate a fix to convert an assignment statement to an augmented assignment.
///
/// For example, given `x = x + 1`, the fix would be `x += 1`.
fn augmented_assignment(
    generator: Generator,
    target: &Expr,
    operator: Operator,
    right_operand: &Expr,
    range: TextRange,
) -> Edit {
    Edit::range_replacement(
        generator.stmt(&ast::Stmt::AugAssign(StmtAugAssign {
            range: TextRange::default(),
            target: Box::new(target.clone()),
            op: operator,
            value: Box::new(right_operand.clone()),
        })),
        range,
    )
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
