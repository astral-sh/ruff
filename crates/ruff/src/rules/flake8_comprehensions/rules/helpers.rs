use rustpython_parser::ast::{self, Expr, Keyword};

pub(super) fn expr_name(func: &Expr) -> Option<&str> {
    if let Expr::Name(ast::ExprName { id, .. }) = func {
        Some(id)
    } else {
        None
    }
}

pub(super) fn exactly_one_argument_with_matching_function<'a>(
    name: &str,
    func: &Expr,
    args: &'a [Expr],
    keywords: &[Keyword],
) -> Option<&'a Expr> {
    if !keywords.is_empty() {
        return None;
    }
    if args.len() != 1 {
        return None;
    }
    if expr_name(func)? != name {
        return None;
    }
    Some(&args[0])
}

pub(super) fn first_argument_with_matching_function<'a>(
    name: &str,
    func: &Expr,
    args: &'a [Expr],
) -> Option<&'a Expr> {
    if expr_name(func)? == name {
        Some(args.first()?)
    } else {
        None
    }
}
