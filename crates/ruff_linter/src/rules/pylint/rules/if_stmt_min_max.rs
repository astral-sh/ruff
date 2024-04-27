use ruff_diagnostics::{Diagnostic, Edit, Fix, FixAvailability, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::comparable::ComparableExpr;
use ruff_python_ast::parenthesize::parenthesized_range;
use ruff_python_ast::{self as ast, CmpOp, Stmt};
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;
use crate::fix::snippet::SourceCodeSnippet;

/// ## What it does
/// Checks for `if` statements that can be replaced with `min()` or `max()`
/// calls.
///
/// ## Why is this bad?
/// An `if` statement that selects the lesser or greater of two sub-expressions
/// can be replaced with a `min()` or `max()` call respectively. When possible,
/// prefer `min()` and `max()`, as they're more concise and readable than the
/// equivalent `if` statements.
///
/// ## Example
/// ```python
/// if score > highest_score:
///     highest_score = score
/// ```
///
/// Use instead:
/// ```python
/// highest_score = max(highest_score, score)
/// ```
///
/// ## References
/// - [Python documentation: max function](https://docs.python.org/3/library/functions.html#max)
/// - [Python documentation: min function](https://docs.python.org/3/library/functions.html#min)
#[violation]
pub struct IfStmtMinMax {
    min_max: MinMax,
    replacement: SourceCodeSnippet,
}

impl Violation for IfStmtMinMax {
    const FIX_AVAILABILITY: FixAvailability = FixAvailability::Sometimes;

    #[derive_message_formats]
    fn message(&self) -> String {
        let Self {
            min_max,
            replacement,
        } = self;
        if let Some(replacement) = replacement.full_display() {
            format!("Replace `if` statement with `{replacement}`")
        } else {
            format!("Replace `if` statement with `{min_max}` call")
        }
    }

    fn fix_title(&self) -> Option<String> {
        let Self {
            min_max,
            replacement,
        } = self;
        if let Some(replacement) = replacement.full_display() {
            Some(format!("Replace with `{replacement}`"))
        } else {
            Some(format!("Replace with `{min_max}` call"))
        }
    }
}

/// R1730, R1731
pub(crate) fn if_stmt_min_max(checker: &mut Checker, stmt_if: &ast::StmtIf) {
    let ast::StmtIf {
        test,
        body,
        elif_else_clauses,
        range: _,
    } = stmt_if;

    if !elif_else_clauses.is_empty() {
        return;
    }

    let [body @ Stmt::Assign(ast::StmtAssign {
        targets: body_targets,
        value: body_value,
        ..
    })] = body.as_slice()
    else {
        return;
    };
    let [body_target] = body_targets.as_slice() else {
        return;
    };

    let Some(ast::ExprCompare {
        ops,
        left,
        comparators,
        ..
    }) = test.as_compare_expr()
    else {
        return;
    };

    // Ignore, e.g., `foo < bar < baz`.
    let [op] = &**ops else {
        return;
    };

    let [right] = &**comparators else {
        return;
    };

    let left_cmp = ComparableExpr::from(left);
    let body_target_cmp = ComparableExpr::from(body_target);
    let right_cmp = ComparableExpr::from(right);
    let body_value_cmp = ComparableExpr::from(body_value);

    let left_is_target = left_cmp == body_target_cmp;
    let right_is_target = right_cmp == body_target_cmp;
    let left_is_value = left_cmp == body_value_cmp;
    let right_is_value = right_cmp == body_value_cmp;

    // Determine whether to use `min()` or `max()`, and whether to flip the
    // order of the arguments, which is relevant for breaking ties.
    // Also ensure that we understand the operation we're trying to do,
    // by checking both sides of the comparison and assignment.
    let (min_max, flip_args) = match (
        left_is_target,
        right_is_target,
        left_is_value,
        right_is_value,
    ) {
        (true, false, false, true) => match op {
            CmpOp::Lt => (MinMax::Max, true),
            CmpOp::LtE => (MinMax::Max, false),
            CmpOp::Gt => (MinMax::Min, true),
            CmpOp::GtE => (MinMax::Min, false),
            _ => return,
        },
        (false, true, true, false) => match op {
            CmpOp::Lt => (MinMax::Min, true),
            CmpOp::LtE => (MinMax::Min, false),
            CmpOp::Gt => (MinMax::Max, true),
            CmpOp::GtE => (MinMax::Max, false),
            _ => return,
        },
        _ => return,
    };

    let (arg1, arg2) = if flip_args {
        (left.as_ref(), right)
    } else {
        (right, left.as_ref())
    };

    let replacement = format!(
        "{} = {min_max}({}, {})",
        checker.locator().slice(
            parenthesized_range(
                body_target.into(),
                body.into(),
                checker.indexer().comment_ranges(),
                checker.locator().contents()
            )
            .unwrap_or(body_target.range())
        ),
        checker.locator().slice(arg1),
        checker.locator().slice(arg2),
    );

    let mut diagnostic = Diagnostic::new(
        IfStmtMinMax {
            min_max,
            replacement: SourceCodeSnippet::from_str(replacement.as_str()),
        },
        stmt_if.range(),
    );

    if checker.semantic().has_builtin_binding(min_max.as_str()) {
        diagnostic.set_fix(Fix::safe_edit(Edit::range_replacement(
            replacement,
            stmt_if.range(),
        )));
    }

    checker.diagnostics.push(diagnostic);
}

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
enum MinMax {
    Min,
    Max,
}

impl MinMax {
    fn as_str(self) -> &'static str {
        match self {
            Self::Min => "min",
            Self::Max => "max",
        }
    }
}

impl std::fmt::Display for MinMax {
    fn fmt(&self, fmt: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(fmt, "{}", self.as_str())
    }
}
