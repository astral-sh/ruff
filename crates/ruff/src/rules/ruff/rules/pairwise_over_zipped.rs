use num_traits::ToPrimitive;
use rustpython_parser::ast::{self, Constant, Expr, Ranged, Unaryop};

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};

use crate::checkers::ast::Checker;

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
    arg_name: String,
    slice_start: Option<i64>,
}

impl SliceInfo {
    pub(crate) fn new(arg_name: String, slice_start: Option<i64>) -> Self {
        Self {
            arg_name,
            slice_start,
        }
    }
}

/// Return the argument name, lower bound, and  upper bound for an expression, if it's a slice.
fn match_slice_info(expr: &Expr) -> Option<SliceInfo> {
    let Expr::Subscript(ast::ExprSubscript { value, slice, .. }) = expr else {
        return None;
    };

    let Expr::Name(ast::ExprName { id: arg_id, .. }) = value.as_ref() else {
        return None;
    };

    let Expr::Slice(ast::ExprSlice { lower,  step, .. }) = slice.as_ref() else {
        return None;
    };

    // Avoid false positives for slices with a step.
    if let Some(step) = step {
        if let Some(step) = to_bound(step) {
            if step != 1 {
                return None;
            }
        } else {
            return None;
        }
    }

    Some(SliceInfo::new(
        arg_id.to_string(),
        lower.as_ref().and_then(|expr| to_bound(expr)),
    ))
}

fn to_bound(expr: &Expr) -> Option<i64> {
    match expr {
        Expr::Constant(ast::ExprConstant {
            value: Constant::Int(value),
            ..
        }) => value.to_i64(),
        Expr::UnaryOp(ast::ExprUnaryOp {
            op: Unaryop::USub | Unaryop::Invert,
            operand,
            range: _,
        }) => {
            if let Expr::Constant(ast::ExprConstant {
                value: Constant::Int(value),
                ..
            }) = operand.as_ref()
            {
                value.to_i64().map(|v| -v)
            } else {
                None
            }
        }
        _ => None,
    }
}

/// RUF007
pub(crate) fn pairwise_over_zipped(checker: &mut Checker, func: &Expr, args: &[Expr]) {
    let Expr::Name(ast::ExprName { id, .. }) = func else {
        return;
    };

    // Require exactly two positional arguments.
    if args.len() != 2 {
        return;
    }

    // Require the function to be the builtin `zip`.
    if !(id == "zip" && checker.semantic_model().is_builtin(id)) {
        return;
    }

    // Allow the first argument to be a `Name` or `Subscript`.
    let Some(first_arg_info) = ({
        if let Expr::Name(ast::ExprName { id, .. }) = &args[0] {
            Some(SliceInfo::new(id.to_string(), None))
        } else {
            match_slice_info(&args[0])
        }
    }) else {
        return;
    };

    // Require second argument to be a `Subscript`.
    if !matches!(&args[1], Expr::Subscript(_)) {
        return;
    }
    let Some(second_arg_info) = match_slice_info(&args[1]) else {
        return;
    };

    // Verify that the arguments match the same name.
    if first_arg_info.arg_name != second_arg_info.arg_name {
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
