use rustpython_ast::{Expr, ExprKind};

use crate::ast::types::Range;
use crate::checks::{Check, CheckKind};

/// Check whether a function call is a `print` or `pprint` invocation
pub fn print_call(expr: &Expr, func: &Expr, location: Range) -> Option<Check> {
    if let ExprKind::Name { id, .. } = &func.node {
        if id == "print" {
            return Some(Check::new(CheckKind::PrintFound, Range::from_located(expr)));
        } else if id == "pprint" {
            return Some(Check::new(CheckKind::PPrintFound, location));
        }
    }

    if let ExprKind::Attribute { value, attr, .. } = &func.node {
        if let ExprKind::Name { id, .. } = &value.node {
            if id == "pprint" && attr == "pprint" {
                return Some(Check::new(CheckKind::PPrintFound, location));
            }
        }
    }

    None
}
