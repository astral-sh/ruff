use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::{self as ast, Expr, Int};
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;

/// ## What it does
/// Checks for use of `zip()` to iterate over successive pairs of elements.
///
/// ## Why is this bad?
/// When iterating over successive pairs of elements, prefer
/// `itertools.pairwise()` over `zip()`.
///
/// `itertools.pairwise()` is more readable and conveys the intent of the code
/// more clearly.
///
/// ## Example
/// ```python
/// letters = "ABCD"
/// zip(letters, letters[1:])  # ("A", "B"), ("B", "C"), ("C", "D")
/// ```
///
/// Use instead:
/// ```python
/// from itertools import pairwise
///
/// letters = "ABCD"
/// pairwise(letters)  # ("A", "B"), ("B", "C"), ("C", "D")
/// ```
///
/// ## References
/// - [Python documentation: `itertools.pairwise`](https://docs.python.org/3/library/itertools.html#itertools.pairwise)
#[violation]
pub struct PairwiseOverZipped;

impl Violation for PairwiseOverZipped {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Prefer `itertools.pairwise()` over `zip()` when iterating over successive pairs")
    }
}

#[derive(Debug)]
struct SliceInfo {
    id: String,
    slice_start: Option<i32>,
}

/// Return the argument name, lower bound, and upper bound for an expression, if it's a slice.
fn match_slice_info(expr: &Expr) -> Option<SliceInfo> {
    let Expr::Subscript(ast::ExprSubscript { value, slice, .. }) = expr else {
        return None;
    };

    let Expr::Name(ast::ExprName { id, .. }) = value.as_ref() else {
        return None;
    };

    let Expr::Slice(ast::ExprSlice { lower, step, .. }) = slice.as_ref() else {
        return None;
    };

    // Avoid false positives for slices with a step.
    if let Some(step) = step {
        if !matches!(
            step.as_ref(),
            Expr::NumberLiteral(ast::ExprNumberLiteral {
                value: ast::Number::Int(Int::ONE),
                ..
            })
        ) {
            return None;
        }
    }

    // If the slice start is a non-constant, we can't be sure that it's successive.
    let slice_start = if let Some(lower) = lower.as_ref() {
        let Expr::NumberLiteral(ast::ExprNumberLiteral {
            value: ast::Number::Int(int),
            range: _,
        }) = lower.as_ref()
        else {
            return None;
        };
        Some(int.as_i32()?)
    } else {
        None
    };

    Some(SliceInfo {
        id: id.to_string(),
        slice_start,
    })
}

/// RUF007
pub(crate) fn pairwise_over_zipped(checker: &mut Checker, func: &Expr, args: &[Expr]) {
    // Require exactly two positional arguments.
    let [first, second] = args else {
        return;
    };

    // Require second argument to be a `Subscript`.
    if !second.is_subscript_expr() {
        return;
    }

    // Require the function to be the builtin `zip`.
    if !checker.semantic().match_builtin_expr(func, "zip") {
        return;
    }

    // Allow the first argument to be a `Name` or `Subscript`.
    let Some(first_arg_info) = ({
        if let Expr::Name(ast::ExprName { id, .. }) = first {
            Some(SliceInfo {
                id: id.to_string(),
                slice_start: None,
            })
        } else {
            match_slice_info(first)
        }
    }) else {
        return;
    };

    let Some(second_arg_info) = match_slice_info(second) else {
        return;
    };

    // Verify that the arguments match the same name.
    if first_arg_info.id != second_arg_info.id {
        return;
    }

    // Verify that the arguments are successive.
    if second_arg_info.slice_start.unwrap_or(0) - first_arg_info.slice_start.unwrap_or(0) != 1 {
        return;
    }

    checker
        .diagnostics
        .push(Diagnostic::new(PairwiseOverZipped, func.range()));
}
