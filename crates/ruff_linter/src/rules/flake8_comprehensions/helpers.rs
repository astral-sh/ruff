use ruff_python_ast::{Expr, ExprCallFields, NodeList};

pub(super) fn exactly_one_argument_with_matching_function<'a>(
    name: &str,
    call: &ExprCallFields<'a>,
) -> Option<Expr<'a>> {
    let args = call.arguments.args();
    if args.len() != 1
        || !call.arguments.keywords().is_empty()
        || call.func.as_name_expr()?.id() != name
    {
        return None;
    }
    args.first()
}

pub(super) fn first_argument_with_matching_function<'a>(
    name: &str,
    func: Expr<'_>,
    args: NodeList<'a, ruff_python_ast::kind::Expr>,
) -> Option<Expr<'a>> {
    if func.as_name_expr().is_some_and(|func| func.id() == name) {
        args.first()
    } else {
        None
    }
}
