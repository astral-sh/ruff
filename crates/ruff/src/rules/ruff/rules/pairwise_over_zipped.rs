use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{violation, derive_message_formats};
use rustpython_parser::ast::{Expr, ExprKind};

use ruff_python_ast::helpers::unparse_constant;
use crate::checkers::ast::Checker;
use crate::Range;

#[violation]
pub struct PairwiseOverZipped {}

impl Violation for PairwiseOverZipped {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Prefer `itertools.pairwise()` to `zip()` if trying to fetch an iterable's successive overlapping pairs.")
    }
}

pub fn pairwise_over_zipped(checker: &mut Checker, func: &Expr, args: &[Expr]) {
    if let ExprKind::Name { id, .. } = &func.node {
        if id == "zip" {
            let ExprKind::Name { id: first_arg_id, .. } = &args[0].node else {
               return;
            };

            let ExprKind::Subscript { value: second_arg, slice, .. } = &args[1].node else {
                return;
            };

            let ExprKind::Name { id: second_arg_id, .. } = &second_arg.node else {
                return;
            };

            // Make sure that the lower end of the slice is 1, or else we can't guarantee that
            // successive pairs are desired
            let mut lower_bound = 0;
            if let ExprKind::Slice { lower, .. } = &slice.node {
                // If there's no lower bound, it can't be a successive pair request
                let ExprKind::Constant { value, .. } = &lower.as_ref().unwrap().node else { 
                    return;
                };

                lower_bound = unparse_constant(value, checker.stylist).parse::<i32>().unwrap();
            }

            if first_arg_id == second_arg_id && lower_bound == 1 {
                checker.diagnostics.push(Diagnostic::new(
                    PairwiseOverZipped {},
                    Range::from(func),
                ));
            }
        }
    }
}
