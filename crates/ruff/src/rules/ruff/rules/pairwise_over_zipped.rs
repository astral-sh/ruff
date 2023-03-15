use std::num::ParseIntError;

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
    slice_start: Result<i64, ParseIntError>,
    slice_end: Result<i64, ParseIntError>,
}

impl SliceInfo {
    pub fn new(
        arg_name: String,
        slice_start: Result<i64, ParseIntError>,
        slice_end: Result<i64, ParseIntError>,
    ) -> Self {
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

    let mut lower_bound = Ok(0i64);
    let mut upper_bound = Ok(0i64);
    if let ExprKind::Slice { lower, upper, .. } = &slice.node {
        if lower.is_some() {
            if let ExprKind::Constant {
                value: lower_value, ..
            } = &lower.as_ref().unwrap().node
            {
                lower_bound = unparse_constant(lower_value, stylist).parse::<i64>();
            }
        }

        if upper.is_some() {
            if let ExprKind::Constant {
                value: upper_value, ..
            } = &upper.as_ref().unwrap().node
            {
                upper_bound = unparse_constant(upper_value, stylist).parse::<i64>();
            }
        }
    };

    Some(SliceInfo::new(arg_id.to_string(), lower_bound, upper_bound))
}

pub fn pairwise_over_zipped(checker: &mut Checker, func: &Expr, args: &[Expr]) {
    if let ExprKind::Name { id, .. } = &func.node {
        if checker.settings.target_version >= PythonVersion::Py310
            && args.len() > 1
            && id == "zip"
            && checker.ctx.is_builtin(id)
        {
            // First arg can be a Name or a Subscript
            let first_arg_info_opt = match &args[0].node {
                ExprKind::Name { id: arg_id, .. } => {
                    Some(SliceInfo::new(arg_id.to_string(), Ok(0i64), Ok(0i64)))
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

            // unwrap_or forces no diagnostic if there was a parsing error
            let args_are_successive = (first_arg_info.slice_start.unwrap_or(1) == 0
                || first_arg_info.slice_end.unwrap_or(1) == -1)
                && second_arg_info.slice_start.unwrap_or(0) == 1;

            if first_arg_info.arg_name == second_arg_info.arg_name && args_are_successive {
                checker
                    .diagnostics
                    .push(Diagnostic::new(PairwiseOverZipped {}, Range::from(func)));
            }
        }
    }
}
