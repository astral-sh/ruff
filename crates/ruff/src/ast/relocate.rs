use rustpython_parser::ast::{Expr, ExprKind, Keyword};

use crate::ast::types::Range;

fn relocate_keyword(keyword: &mut Keyword, location: Range) {
    keyword.location = location.location;
    keyword.end_location = Some(location.end_location);
    relocate_expr(&mut keyword.node.value, location);
}

/// Change an expression's location (recursively) to match a desired, fixed
/// location.
pub fn relocate_expr(expr: &mut Expr, location: Range) {
    expr.location = location.location;
    expr.end_location = Some(location.end_location);
    match &mut expr.node {
        ExprKind::BoolOp { values, .. } => {
            for expr in values {
                relocate_expr(expr, location);
            }
        }
        ExprKind::NamedExpr { target, value } => {
            relocate_expr(target, location);
            relocate_expr(value, location);
        }
        ExprKind::BinOp { left, right, .. } => {
            relocate_expr(left, location);
            relocate_expr(right, location);
        }
        ExprKind::UnaryOp { operand, .. } => {
            relocate_expr(operand, location);
        }
        ExprKind::Lambda { body, .. } => {
            relocate_expr(body, location);
        }
        ExprKind::IfExp { test, body, orelse } => {
            relocate_expr(test, location);
            relocate_expr(body, location);
            relocate_expr(orelse, location);
        }
        ExprKind::Dict { keys, values } => {
            for expr in keys.iter_mut().flatten() {
                relocate_expr(expr, location);
            }
            for expr in values {
                relocate_expr(expr, location);
            }
        }
        ExprKind::Set { elts } => {
            for expr in elts {
                relocate_expr(expr, location);
            }
        }
        ExprKind::ListComp { elt, .. } => {
            relocate_expr(elt, location);
        }
        ExprKind::SetComp { elt, .. } => {
            relocate_expr(elt, location);
        }
        ExprKind::DictComp { key, value, .. } => {
            relocate_expr(key, location);
            relocate_expr(value, location);
        }
        ExprKind::GeneratorExp { elt, .. } => {
            relocate_expr(elt, location);
        }
        ExprKind::Await { value } => relocate_expr(value, location),
        ExprKind::Yield { value } => {
            if let Some(expr) = value {
                relocate_expr(expr, location);
            }
        }
        ExprKind::YieldFrom { value } => relocate_expr(value, location),
        ExprKind::Compare {
            left, comparators, ..
        } => {
            relocate_expr(left, location);
            for expr in comparators {
                relocate_expr(expr, location);
            }
        }
        ExprKind::Call {
            func,
            args,
            keywords,
        } => {
            relocate_expr(func, location);
            for expr in args {
                relocate_expr(expr, location);
            }
            for keyword in keywords {
                relocate_keyword(keyword, location);
            }
        }
        ExprKind::FormattedValue {
            value, format_spec, ..
        } => {
            relocate_expr(value, location);
            if let Some(expr) = format_spec {
                relocate_expr(expr, location);
            }
        }
        ExprKind::JoinedStr { values } => {
            for expr in values {
                relocate_expr(expr, location);
            }
        }
        ExprKind::Constant { .. } => {}
        ExprKind::Attribute { value, .. } => {
            relocate_expr(value, location);
        }
        ExprKind::Subscript { value, slice, .. } => {
            relocate_expr(value, location);
            relocate_expr(slice, location);
        }
        ExprKind::Starred { value, .. } => {
            relocate_expr(value, location);
        }
        ExprKind::Name { .. } => {}
        ExprKind::List { elts, .. } => {
            for expr in elts {
                relocate_expr(expr, location);
            }
        }
        ExprKind::Tuple { elts, .. } => {
            for expr in elts {
                relocate_expr(expr, location);
            }
        }
        ExprKind::Slice { lower, upper, step } => {
            if let Some(expr) = lower {
                relocate_expr(expr, location);
            }
            if let Some(expr) = upper {
                relocate_expr(expr, location);
            }
            if let Some(expr) = step {
                relocate_expr(expr, location);
            }
        }
    }
}
