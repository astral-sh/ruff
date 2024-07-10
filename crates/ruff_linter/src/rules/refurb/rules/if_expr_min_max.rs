use ruff_diagnostics::{Diagnostic, Edit, Fix, FixAvailability, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::comparable::ComparableExpr;
use ruff_python_ast::{self as ast, CmpOp, Expr};
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;
use crate::fix::snippet::SourceCodeSnippet;

/// ## What it does
/// Checks for `if` expressions that can be replaced with `min()` or `max()`
/// calls.
///
/// ## Why is this bad?
/// An `if` expression that selects the lesser or greater of two
/// sub-expressions can be replaced with a `min()` or `max()` call
/// respectively. When possible, prefer `min()` and `max()`, as they're more
/// concise and readable than the equivalent `if` expression.
///
/// ## Example
/// ```python
/// highest_score = score1 if score1 > score2 else score2
/// ```
///
/// Use instead:
/// ```python
/// highest_score = max(score2, score1)
/// ```
///
/// ## References
/// - [Python documentation: `min`](https://docs.python.org/3.11/library/functions.html#min)
/// - [Python documentation: `max`](https://docs.python.org/3.11/library/functions.html#max)
#[violation]
pub struct IfExprMinMax {
    min_max: MinMax,
    expression: SourceCodeSnippet,
    replacement: SourceCodeSnippet,
}

impl Violation for IfExprMinMax {
    const FIX_AVAILABILITY: FixAvailability = FixAvailability::Sometimes;

    #[derive_message_formats]
    fn message(&self) -> String {
        let Self {
            min_max,
            expression,
            replacement,
        } = self;

        match (expression.full_display(), replacement.full_display()) {
            (_, None) => {
                format!("Replace `if` expression with `{min_max}` call")
            }
            (None, Some(replacement)) => {
                format!("Replace `if` expression with `{replacement}`")
            }
            (Some(expression), Some(replacement)) => {
                format!("Replace `{expression}` with `{replacement}`")
            }
        }
    }

    fn fix_title(&self) -> Option<String> {
        let Self {
            replacement,
            min_max,
            ..
        } = self;
        if let Some(replacement) = replacement.full_display() {
            Some(format!("Replace with `{replacement}`"))
        } else {
            Some(format!("Replace with `{min_max}` call"))
        }
    }
}

/// FURB136
pub(crate) fn if_expr_min_max(checker: &mut Checker, if_exp: &ast::ExprIf) {
    let Expr::Compare(ast::ExprCompare {
        left,
        ops,
        comparators,
        ..
    }) = if_exp.test.as_ref()
    else {
        return;
    };

    // Ignore, e.g., `foo < bar < baz`.
    let [op] = &**ops else {
        return;
    };

    // Determine whether to use `min()` or `max()`, and whether to flip the
    // order of the arguments, which is relevant for breaking ties.
    let (mut min_max, mut flip_args) = match op {
        CmpOp::Gt => (MinMax::Max, true),
        CmpOp::GtE => (MinMax::Max, false),
        CmpOp::Lt => (MinMax::Min, true),
        CmpOp::LtE => (MinMax::Min, false),
        _ => return,
    };

    let [right] = &**comparators else {
        return;
    };

    let body_cmp = ComparableExpr::from(if_exp.body.as_ref());
    let orelse_cmp = ComparableExpr::from(if_exp.orelse.as_ref());
    let left_cmp = ComparableExpr::from(left);
    let right_cmp = ComparableExpr::from(right);

    if body_cmp == right_cmp && orelse_cmp == left_cmp {
        min_max = min_max.reverse();
        flip_args = !flip_args;
    } else if body_cmp != left_cmp || orelse_cmp != right_cmp {
        return;
    }

    let (arg1, arg2) = if flip_args {
        (right, left.as_ref())
    } else {
        (left.as_ref(), right)
    };

    let replacement = format!(
        "{min_max}({}, {})",
        checker.generator().expr(arg1),
        checker.generator().expr(arg2),
    );

    let mut diagnostic = Diagnostic::new(
        IfExprMinMax {
            min_max,
            expression: SourceCodeSnippet::from_str(checker.locator().slice(if_exp)),
            replacement: SourceCodeSnippet::from_str(replacement.as_str()),
        },
        if_exp.range(),
    );

    if checker.semantic().has_builtin_binding(min_max.as_str()) {
        diagnostic.set_fix(Fix::safe_edit(Edit::range_replacement(
            replacement,
            if_exp.range(),
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
    #[must_use]
    const fn reverse(self) -> Self {
        match self {
            Self::Min => Self::Max,
            Self::Max => Self::Min,
        }
    }

    #[must_use]
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
