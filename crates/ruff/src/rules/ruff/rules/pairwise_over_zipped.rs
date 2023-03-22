use num_traits::ToPrimitive;
use rustpython_parser::ast::{Constant, Expr, ExprKind, Unaryop};

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};

use crate::checkers::ast::Checker;
use crate::Range;

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
    pub fn new(arg_name: String, slice_start: Option<i64>) -> Self {
        Self {
            arg_name,
            slice_start,
        }
    }
}

/// Return the argument name, lower bound, and  upper bound for an expression, if it's a slice.
fn match_slice_info(expr: &Expr) -> Option<SliceInfo> {
    let ExprKind::Subscript { value, slice, .. } = &expr.node else {
        return None;
    };

    let ExprKind::Name { id: arg_id, .. } = &value.node else {
        return None;
    };

    let ExprKind::Slice { lower,  step, .. } = &slice.node else {
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
    match &expr.node {
        ExprKind::Constant {
            value: Constant::Int(value),
            ..
        } => value.to_i64(),
        ExprKind::UnaryOp {
            op: Unaryop::USub | Unaryop::Invert,
            operand,
        } => {
            if let ExprKind::Constant {
                value: Constant::Int(value),
                ..
            } = &operand.node
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
pub fn pairwise_over_zipped(checker: &mut Checker, func: &Expr, args: &[Expr]) {
    let ExprKind::Name { id, .. } = &func.node else {
        return;
    };

    // Require exactly two positional arguments.
    if args.len() != 2 {
        return;
    }

    // Require the function to be the builtin `zip`.
    if id != "zip" {
        return;
    }
    if !checker.ctx.is_builtin(id) {
        return;
    }

    // Allow the first argument to be a `Name` or `Subscript`.
    let Some(first_arg_info) = ({
        if let ExprKind::Name { id, .. } = &args[0].node {
            Some(SliceInfo::new(id.to_string(), None))
        } else {
            match_slice_info(&args[0])
        }
    }) else {
        return;
    };

    // Require second argument to be a `Subscript`.
    let ExprKind::Subscript { .. } = &args[1].node else {
        return;
    };
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
        .push(Diagnostic::new(PairwiseOverZipped, Range::from(func)));
}
