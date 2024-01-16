use ruff_python_ast::{self as ast, Expr, ExprContext};

pub(super) fn set_context(expr: Expr, ctx: ExprContext) -> Expr {
    match expr {
        Expr::Name(ast::ExprName { id, range, .. }) => ast::ExprName { range, id, ctx }.into(),
        Expr::Tuple(ast::ExprTuple {
            elts,
            range,
            parenthesized,
            ctx: _,
        }) => ast::ExprTuple {
            elts: elts.into_iter().map(|elt| set_context(elt, ctx)).collect(),
            range,
            ctx,
            parenthesized,
        }
        .into(),

        Expr::List(ast::ExprList { elts, range, .. }) => ast::ExprList {
            elts: elts.into_iter().map(|elt| set_context(elt, ctx)).collect(),
            range,
            ctx,
        }
        .into(),
        Expr::Attribute(ast::ExprAttribute {
            value, attr, range, ..
        }) => ast::ExprAttribute {
            range,
            value,
            attr,
            ctx,
        }
        .into(),
        Expr::Subscript(ast::ExprSubscript {
            value,
            slice,
            range,
            ..
        }) => ast::ExprSubscript {
            range,
            value,
            slice,
            ctx,
        }
        .into(),
        Expr::Starred(ast::ExprStarred { value, range, .. }) => ast::ExprStarred {
            value: Box::new(set_context(*value, ctx)),
            range,
            ctx,
        }
        .into(),
        _ => expr,
    }
}
