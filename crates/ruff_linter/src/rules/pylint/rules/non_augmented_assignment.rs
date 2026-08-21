use ast::Expr;
use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_python_ast as ast;
use ruff_python_ast::comparable::ComparableExpr;
use ruff_python_ast::token::{Tokens, parenthesized_range};
use ruff_python_ast::{ExprBinOp, ExprRef, Operator};
use ruff_source_file::LineRanges;
use ruff_text_size::{Ranged, TextRange};

use crate::checkers::ast::Checker;
use crate::{AlwaysFixableViolation, Edit, Fix};

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
/// The same applies to chains of the same operator, as long as the operator is
/// commutative and associative: `x = x * 2 * y` can be rewritten as
/// `x *= 2 * y`.
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
/// Matrix multiplication (`@`) chains are deliberately left alone. `@` is
/// associative, but the grouping determines how much work is done: for a
/// vector `x`, `(x @ a) @ b` is a pair of cheap matrix-vector products, while
/// `x @= a @ b` builds the full matrix-matrix product `a @ b` first.
///
/// ## Example
/// ```python
/// x = x + 1
/// y = y * 2 * z
/// ```
///
/// Use instead:
/// ```python
/// x += 1
/// y *= 2 * z
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
///
/// It also regroups the operands of a chain, e.g., `x = x + y + z` becomes
/// `x += y + z`. Floating-point addition and multiplication are not truly
/// associative, so the result can differ in the last bits: `(0.1 + 0.2) + 0.3`
/// is not `0.1 + (0.2 + 0.3)`.
///
/// An augmented assignment can also fail where the plain form succeeds. NumPy
/// writes the result into the target's buffer, so `a *= b` raises where
/// `a = a * b` would broadcast to a new shape or promote the dtype. The same
/// applies to `a @= b`, which requires the product to have the target's shape.
///
/// The fix replaces the whole statement, so any comments inside it are lost.
#[derive(ViolationMetadata)]
#[violation_metadata(preview_since = "v0.3.7")]
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
        report_augmented_assignment(
            checker,
            assign,
            target,
            operator,
            operand_range(checker, &value.right, value),
        );

        return;
    }

    // Every remaining rewrite either moves the target to the front of the expression, or regroups
    // the operands, or both, so none of them are valid unless the operands can be rearranged.
    if !operator.allows_rearranging_operands() {
        return;
    }

    // Match a chain of the same operator, e.g., `x = x * 2 * y`. Python parses `a * b * c` as
    // `(a * b) * c`, so the target sits at the bottom of the chain's left spine.
    let innermost = innermost_chain_link(checker.tokens(), value);
    if ComparableExpr::from(target) == ComparableExpr::from(&innermost.left) {
        // Everything to the right of the target, e.g., `2 * y` in `x = x * 2 * y`. `innermost`
        // sits on `value`'s left spine, so its right operand always precedes `value`'s.
        let operand = FixOperand {
            range: TextRange::new(
                operand_range(checker, &innermost.right, innermost)
                    .range
                    .start(),
                operand_range(checker, &value.right, value).range.end(),
            ),
            // The span covers at least two operands and the operator between them, so it is
            // never a single parenthesized group.
            parenthesized: false,
        };

        report_augmented_assignment(checker, assign, target, operator, operand);

        return;
    }

    // Match, e.g., `x = 1 + x`, but limit such matches to expressions that are guaranteed to
    // evaluate to a number. Commutativity only holds for the conventional numeric meanings of these
    // operators: `x = "prefix-" + x` is not `x += "prefix-"`.
    //
    // The left-hand side is reused verbatim, so its own grouping is preserved: this also covers
    // chains such as `x = 2 * 3 * x`, which becomes `x *= 2 * 3`.
    if is_numeric_constant(&value.left)
        && ComparableExpr::from(target) == ComparableExpr::from(&value.right)
    {
        report_augmented_assignment(
            checker,
            assign,
            target,
            operator,
            operand_range(checker, &value.left, value),
        );
    }
}

/// Walks down the left spine of `value` for as long as the nested operations use the same operator,
/// and returns the innermost one.
///
/// For example, given `x * 2 * y` (parsed as `(x * 2) * y`), this returns `x * 2`.
///
/// Descent stops at a parenthesized operand, e.g., `(x * 2) * y`. Those parentheses sit in the
/// middle of the source text we would otherwise reuse verbatim for the fix, so a chain that
/// contains them can't be rewritten by slicing.
fn innermost_chain_link<'a>(tokens: &Tokens, value: &'a ExprBinOp) -> &'a ExprBinOp {
    let mut current = value;

    while let Expr::BinOp(left) = &*current.left
        && left.op == current.op
        && parenthesized_range(ExprRef::from(&*current.left), current.into(), tokens).is_none()
    {
        current = left;
    }

    current
}

/// Returns `true` if `expr` is a literal number or boolean, or an operation over such literals,
/// e.g. `1`, `True`, `-2`, `(not 0)`, or `2 * 3`.
///
/// Binary operations count too, not just a single literal under unary operators: an operator applied
/// to numbers either produces a number or raises (like `2 @ 3`), and `not` always produces a boolean,
/// so the whole expression is known to be a number without inferring any types.
///
/// Note that `not` can only reach this point parenthesized, as in `x = (not 0) + x`; `not 0 + x`
/// parses as `not (0 + x)` instead.
fn is_numeric_constant(expr: &Expr) -> bool {
    match expr {
        Expr::NumberLiteral(_) | Expr::BooleanLiteral(_) => true,
        Expr::UnaryOp(ast::ExprUnaryOp { operand, .. }) => is_numeric_constant(operand),
        Expr::BinOp(ast::ExprBinOp { left, right, .. }) => {
            is_numeric_constant(left) && is_numeric_constant(right)
        }
        _ => false,
    }
}

/// The source of the operand that becomes the right-hand side of the augmented assignment, and
/// whether that source already carries the parentheses that hold it together across line breaks.
///
/// Depending on which form was matched, this can come from either side of the original operation:
/// `x = x + 1` takes it from the right, `x = 1 + x` from the left.
#[derive(Debug, Clone, Copy)]
struct FixOperand {
    range: TextRange,
    parenthesized: bool,
}

/// Returns the source of `operand` within `parent`, including its parentheses, if any.
fn operand_range(checker: &Checker, operand: &Expr, parent: &ExprBinOp) -> FixOperand {
    match parenthesized_range(ExprRef::from(operand), parent.into(), checker.tokens()) {
        Some(range) => FixOperand {
            range,
            parenthesized: true,
        },
        None => FixOperand {
            range: operand.range(),
            parenthesized: false,
        },
    }
}

/// Report `assign` and attach a fix that replaces the whole statement with an augmented assignment
/// of `target`, `operator` and `operand`.
///
/// For example, given `x = x + 1`, the fix would be `x += 1`.
fn report_augmented_assignment(
    checker: &Checker,
    assign: &ast::StmtAssign,
    target: &Expr,
    operator: AugmentedOperator,
    operand: FixOperand,
) {
    let locator = checker.locator();

    let operand_expr = locator.slice(operand.range);
    let target_expr = locator.slice(target);

    // A multi-line right-hand side may only have been valid because the whole assigned value was
    // parenthesized, e.g. `x = (\n    x\n    + 1\n    + 2\n)`. Those outer parentheses are dropped
    // along with the rest of the statement, so put a fresh pair around the operand to keep the
    // continuation lines legal.
    let new_content = if operand.parenthesized || !locator.contains_line_break(operand.range) {
        format!("{target_expr} {operator} {operand_expr}")
    } else {
        format!("{target_expr} {operator} ({operand_expr})")
    };

    let mut diagnostic =
        checker.report_diagnostic(NonAugmentedAssignment { operator }, assign.range());
    diagnostic.set_fix(Fix::unsafe_edit(Edit::range_replacement(
        new_content,
        assign.range,
    )));
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
    /// Returns `true` if the operands of this operator can be rearranged freely, i.e. the operator
    /// is both commutative and associative.
    ///
    /// Commutativity is what lets `x = 1 + x` become `x += 1`; associativity is what lets
    /// `x = x + y + z` become `x += y + z`. Both are needed here because the two rewrites share a
    /// single guard, and every operator that has one of the properties has the other, apart from
    /// [`Self::MatMult`]. If you add an operator that is associative but not commutative, or the
    /// reverse, split this into two predicates rather than widening it.
    ///
    /// [`Self::MatMult`] is excluded even though matrix multiplication is associative, because
    /// regrouping a matrix product preserves the result while potentially changing the amount of
    /// arithmetic by orders of magnitude. For a vector `x`, `(x @ a) @ b` is a pair of cheap
    /// matrix-vector products, while `x @= a @ b` builds the full matrix-matrix product `a @ b`
    /// first.
    ///
    /// Note that even for the operators listed here, both properties only hold for their
    /// conventional meanings. Floating-point addition and multiplication are not associative, `+`
    /// on strings and lists is not commutative, and an arbitrary type can overload the operator to
    /// mean anything at all. The rewrites that rely on commutativity guard against this by checking
    /// that the operands are numbers; the ones that rely on associativity don't, which is why the
    /// float caveat is called out in the rule's documentation.
    fn allows_rearranging_operands(self) -> bool {
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
