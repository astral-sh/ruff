use ruff_text_size::TextRange;
use rustpython_parser::ast::{self, Expr, ExprKind, Keyword};

fn relocate_keyword(keyword: &mut Keyword, location: TextRange) {
    keyword.range = location;
    relocate_expr(&mut keyword.node.value, location);
}

/// Change an expression's location (recursively) to match a desired, fixed
/// location.
pub fn relocate_expr(expr: &mut Expr, location: TextRange) {
    expr.range = location;
    match &mut expr.node {
        ExprKind::BoolOp(ast::ExprBoolOp { values, .. }) => {
            for expr in values {
                relocate_expr(expr, location);
            }
        }
        ExprKind::NamedExpr(ast::ExprNamedExpr { target, value }) => {
            relocate_expr(target, location);
            relocate_expr(value, location);
        }
        ExprKind::BinOp(ast::ExprBinOp { left, right, .. }) => {
            relocate_expr(left, location);
            relocate_expr(right, location);
        }
        ExprKind::UnaryOp(ast::ExprUnaryOp { operand, .. }) => {
            relocate_expr(operand, location);
        }
        ExprKind::Lambda(ast::ExprLambda { body, .. }) => {
            relocate_expr(body, location);
        }
        ExprKind::IfExp(ast::ExprIfExp { test, body, orelse }) => {
            relocate_expr(test, location);
            relocate_expr(body, location);
            relocate_expr(orelse, location);
        }
        ExprKind::Dict(ast::ExprDict { keys, values }) => {
            for expr in keys.iter_mut().flatten() {
                relocate_expr(expr, location);
            }
            for expr in values {
                relocate_expr(expr, location);
            }
        }
        ExprKind::Set(ast::ExprSet { elts }) => {
            for expr in elts {
                relocate_expr(expr, location);
            }
        }
        ExprKind::ListComp(ast::ExprListComp { elt, .. }) => {
            relocate_expr(elt, location);
        }
        ExprKind::SetComp(ast::ExprSetComp { elt, .. }) => {
            relocate_expr(elt, location);
        }
        ExprKind::DictComp(ast::ExprDictComp { key, value, .. }) => {
            relocate_expr(key, location);
            relocate_expr(value, location);
        }
        ExprKind::GeneratorExp(ast::ExprGeneratorExp { elt, .. }) => {
            relocate_expr(elt, location);
        }
        ExprKind::Await(ast::ExprAwait { value }) => relocate_expr(value, location),
        ExprKind::Yield(ast::ExprYield { value }) => {
            if let Some(expr) = value {
                relocate_expr(expr, location);
            }
        }
        ExprKind::YieldFrom(ast::ExprYieldFrom { value }) => relocate_expr(value, location),
        ExprKind::Compare(ast::ExprCompare {
            left, comparators, ..
        }) => {
            relocate_expr(left, location);
            for expr in comparators {
                relocate_expr(expr, location);
            }
        }
        ExprKind::Call(ast::ExprCall {
            func,
            args,
            keywords,
        }) => {
            relocate_expr(func, location);
            for expr in args {
                relocate_expr(expr, location);
            }
            for keyword in keywords {
                relocate_keyword(keyword, location);
            }
        }
        ExprKind::FormattedValue(ast::ExprFormattedValue {
            value, format_spec, ..
        }) => {
            relocate_expr(value, location);
            if let Some(expr) = format_spec {
                relocate_expr(expr, location);
            }
        }
        ExprKind::JoinedStr(ast::ExprJoinedStr { values }) => {
            for expr in values {
                relocate_expr(expr, location);
            }
        }
        ExprKind::Constant(_) => {}
        ExprKind::Attribute(ast::ExprAttribute { value, .. }) => {
            relocate_expr(value, location);
        }
        ExprKind::Subscript(ast::ExprSubscript { value, slice, .. }) => {
            relocate_expr(value, location);
            relocate_expr(slice, location);
        }
        ExprKind::Starred(ast::ExprStarred { value, .. }) => {
            relocate_expr(value, location);
        }
        ExprKind::Name(_) => {}
        ExprKind::List(ast::ExprList { elts, .. }) => {
            for expr in elts {
                relocate_expr(expr, location);
            }
        }
        ExprKind::Tuple(ast::ExprTuple { elts, .. }) => {
            for expr in elts {
                relocate_expr(expr, location);
            }
        }
        ExprKind::Slice(ast::ExprSlice { lower, upper, step }) => {
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
