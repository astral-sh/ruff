use rustpython_parser::ast::{Cmpop, Expr, ExprKind};

use crate::ast::helpers::{create_expr, unparse_expr};
use crate::source_code::Stylist;

pub fn is_ambiguous_name(name: &str) -> bool {
    name == "l" || name == "I" || name == "O"
}

pub fn compare(left: &Expr, ops: &[Cmpop], comparators: &[Expr], stylist: &Stylist) -> String {
    unparse_expr(
        &create_expr(ExprKind::Compare {
            left: Box::new(left.clone()),
            ops: ops.to_vec(),
            comparators: comparators.to_vec(),
        }),
        stylist,
    )
}
