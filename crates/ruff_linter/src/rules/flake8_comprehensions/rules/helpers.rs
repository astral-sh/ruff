use ruff_python_ast::{Expr, Keyword};

pub(super) fn exactly_one_argument_with_matching_function<'a, 'ast>(
    name: &str,
    func: &Expr,
    args: &'a [Expr<'ast>],
    keywords: &[Keyword],
) -> Option<&'a Expr<'ast>> {
    let [arg] = args else {
        return None;
    };
    if !keywords.is_empty() {
        return None;
    }
    let func = func.as_name_expr()?;
    if func.id != name {
        return None;
    }
    Some(arg)
}

pub(super) fn first_argument_with_matching_function<'a, 'ast>(
    name: &str,
    func: &Expr,
    args: &'a [Expr<'ast>],
) -> Option<&'a Expr<'ast>> {
    if func.as_name_expr().is_some_and(|func| func.id == name) {
        args.first()
    } else {
        None
    }
}
