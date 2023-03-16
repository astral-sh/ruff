use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::source_code::Stylist;
use rustpython_parser::ast::{Constant, Expr, ExprKind, Unaryop};

use crate::checkers::ast::Checker;
use crate::settings::types::PythonVersion;
use crate::Range;
use ruff_python_ast::helpers::unparse_constant;

#[violation]
pub struct PairwiseOverZipped {}

impl Violation for PairwiseOverZipped {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Prefer `itertools.pairwise()` to `zip()` if trying to fetch an iterable's successive overlapping pairs.")
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

// Get arg name, lower bound, and upper bound for an expression, if it's a slice
fn get_slice_info(expr: &Expr, stylist: &Stylist) -> Option<SliceInfo> {
    let ExprKind::Subscript { value, slice, .. } = &expr.node else {
        return None;
    };

    let ExprKind::Name { id: arg_id, .. } = &value.node else {
        return None;
    };

    let mut lower_bound = None;
    if let ExprKind::Slice { lower, .. } = &slice.node {
        if lower.is_some() {
            lower_bound = get_bound(&lower.as_ref().unwrap().node, stylist);
        }
    };

    Some(SliceInfo::new(arg_id.to_string(), lower_bound))
}

fn get_bound(expr: &ExprKind, stylist: &Stylist) -> Option<i64> {
    fn get_constant_value(constant: &Constant, stylist: &Stylist) -> Option<i64> {
        unparse_constant(constant, stylist).parse::<i64>().ok()
    }

    let mut bound = None;
    match expr {
        ExprKind::Constant { value, .. } => bound = get_constant_value(value, stylist),
        ExprKind::UnaryOp {
            op: Unaryop::USub | Unaryop::Invert,
            operand,
        } => {
            if let ExprKind::Constant { value, .. } = &operand.node {
                bound = get_constant_value(value, stylist).map(|v| -v);
            }
        }
        _ => (),
    }

    bound
}

pub fn pairwise_over_zipped(checker: &mut Checker, func: &Expr, args: &[Expr]) {
    if let ExprKind::Name { id, .. } = &func.node {
        if checker.settings.target_version >= PythonVersion::Py310
            && args.len() > 1
            && id == "zip"
            && checker.ctx.is_builtin(id)
        {
            // First arg can be a only be a Name or a Subscript. If it's a Name, we want the "slice" to
            // default to 0
            let first_arg_info_opt = match &args[0].node {
                ExprKind::Name { id: arg_id, .. } => {
                    Some(SliceInfo::new(arg_id.to_string(), Some(0i64)))
                }
                ExprKind::Subscript { .. } => get_slice_info(&args[0], checker.stylist),
                _ => None,
            };

            // If it's not one of those, return
            let Some(first_arg_info) = first_arg_info_opt else { return; };

            // Second arg can only be a subscript
            let ExprKind::Subscript { .. } = &args[1].node else {
                return;
            };

            let Some(second_arg_info) = get_slice_info(&args[1], checker.stylist) else { return; };

            let args_are_successive = second_arg_info.slice_start.unwrap_or(0)
                - first_arg_info.slice_start.unwrap_or(0)
                == 1;

            if first_arg_info.arg_name == second_arg_info.arg_name && args_are_successive {
                checker
                    .diagnostics
                    .push(Diagnostic::new(PairwiseOverZipped {}, Range::from(func)));
            }
        }
    }
}
