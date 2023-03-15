use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::source_code::Stylist;
use rustpython_parser::ast::{Expr, ExprKind};

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
    slice_start: i32,
    slice_end: i32,
}

impl SliceInfo {
    pub fn new(arg_name: String, slice_start: i32, slice_end: i32) -> Self {
        Self {
            arg_name,
            slice_start,
            slice_end,
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

    let mut lower_bound = 0;
    let mut upper_bound = 0;
    if let ExprKind::Slice { lower, upper, .. } = &slice.node {
        if lower.is_some() {
            if let ExprKind::Constant {
                value: lower_value, ..
            } = &lower.as_ref().unwrap().node
            {
                lower_bound = unparse_constant(lower_value, stylist)
                    .parse::<i32>()
                    .unwrap_or(0);
            }
        }

        if upper.is_some() {
            if let ExprKind::Constant {
                value: upper_value, ..
            } = &upper.as_ref().unwrap().node
            {
                upper_bound = unparse_constant(upper_value, stylist)
                    .parse::<i32>()
                    .unwrap_or(0);
            }
        }
    };

    Some(SliceInfo::new(arg_id.to_string(), lower_bound, upper_bound))
}

pub fn pairwise_over_zipped(checker: &mut Checker, func: &Expr, args: &[Expr]) {
    if let ExprKind::Name { id, .. } = &func.node {
        // Ensure that the checker settings are valid for the rule to apply
        let valid_checker =
            checker.ctx.is_builtin(id) && checker.settings.target_version >= PythonVersion::Py310;

        if valid_checker && id == "zip" && args.len() > 1 {
            // First arg can be a Name or a Subscript
            let first_arg_info_opt = match &args[0].node {
                ExprKind::Name { id: arg_id, .. } => Some(SliceInfo::new(arg_id.to_string(), 0, 0)),
                ExprKind::Subscript { .. } => get_slice_info(&args[0], checker.stylist),
                _ => None,
            };

            // If it's not one of those, return
            if first_arg_info_opt.is_none() {
                return;
            }

            // Second arg can only be a subscript
            let ExprKind::Subscript { .. } = &args[1].node else {
                return;
            };
            let second_arg_info = get_slice_info(&args[1], checker.stylist).unwrap();

            let first_arg_info = first_arg_info_opt.unwrap();
            let args_are_successive = (first_arg_info.slice_start == 0
                || first_arg_info.slice_end == -1)
                && second_arg_info.slice_start == 1;

            if first_arg_info.arg_name == second_arg_info.arg_name && args_are_successive {
                checker
                    .diagnostics
                    .push(Diagnostic::new(PairwiseOverZipped {}, Range::from(func)));
            }
        }
    }
}
