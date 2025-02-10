use ruff_diagnostics::{Diagnostic, Edit, Fix, FixAvailability, Violation};
use ruff_macros::{derive_message_formats, ViolationMetadata};
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
/// can be replaced with a `min()` or `max()` call respectively. Where possible,
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
/// - [Python documentation: `max`](https://docs.python.org/3/library/functions.html#max)
/// - [Python documentation: `min`](https://docs.python.org/3/library/functions.html#min)
#[derive(ViolationMetadata)]
pub(crate) struct IfStmtMinMax {
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

    // extract helpful info from expression of the form
    // `if cmp_left op cmp_right: body_left = body_right`
    let cmp_left = ComparableExpr::from(left);
    let cmp_right = ComparableExpr::from(right);
    let body_left = ComparableExpr::from(body_target);
    let body_right = ComparableExpr::from(body_value);

    // these booleans are used to understand in which case we are.
    // The two possible cases that the rule addresses are:
    // - `if cmp_left op cmp_right: cmp_left = cmp_right`
    // - `if cmp_left op cmp_right: cmp_right = cmp_left `
    let cmp_left_is_body_left = cmp_left == body_left;
    let cmp_right_is_body_right = cmp_right == body_right;
    let cmp_left_is_body_right = cmp_left == body_right;
    let cmp_right_is_body_left = cmp_right == body_left;

    let (min_max, arg1, arg2) = match (
        cmp_left_is_body_left,
        cmp_right_is_body_right,
        cmp_left_is_body_right,
        cmp_right_is_body_left,
    ) {
        (true, true, false, false) => match op {
            CmpOp::LtE => (MinMax::Max, right, &**left),
            CmpOp::GtE => (MinMax::Min, right, &**left),
            CmpOp::Gt => (MinMax::Min, &**left, right),
            CmpOp::Lt => (MinMax::Max, &**left, right),
            _ => return,
        },
        (false, false, true, true) => match op {
            CmpOp::LtE => (MinMax::Min, right, &**left),
            CmpOp::GtE => (MinMax::Max, &**left, right),
            CmpOp::Gt => (MinMax::Max, right, &**left),
            CmpOp::Lt => (MinMax::Min, right, &**left),
            _ => return,
        },
        _ => return,
    };

    let replacement = format!(
        "{} = {min_max}({}, {})",
        checker.locator().slice(
            parenthesized_range(
                body_target.into(),
                body.into(),
                checker.comment_ranges(),
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
    const fn as_str(self) -> &'static str {
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
