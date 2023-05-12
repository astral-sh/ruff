use rustpython_parser::ast::{self, Expr, ExprKind, Keyword};

pub(crate) fn expr_name(func: &Expr) -> Option<&str> {
    if let ExprKind::Name(ast::ExprName { id, .. }) = &func.node {
        Some(id)
    } else {
        None
    }
}

pub(crate) fn exactly_one_argument_with_matching_function<'a>(
    name: &str,
    func: &Expr,
    args: &'a [Expr],
    keywords: &[Keyword],
) -> Option<&'a ExprKind> {
    if !keywords.is_empty() {
        return None;
    }
    if args.len() != 1 {
        return None;
    }
    if expr_name(func)? != name {
        return None;
    }
    Some(&args[0].node)
}

pub(crate) fn first_argument_with_matching_function<'a>(
    name: &str,
    func: &Expr,
    args: &'a [Expr],
) -> Option<&'a ExprKind> {
    if expr_name(func)? == name {
        Some(&args.first()?.node)
    } else {
        None
    }
}
