use ruff_python_ast::Argument;
use ruff_python_ast::Arguments;
use ruff_python_ast::Expr;

pub(super) fn exactly_one_argument_with_matching_function<'a>(
    name: &str,
    func: &Expr,
    arguments: &'a Arguments,
) -> Option<&'a Expr> {
    let [Argument::Positional(arg)] = arguments.items.as_slice() else {
        return None;
    };
    let func = func.as_name_expr()?;
    if func.id != name {
        return None;
    }
    Some(arg)
}

pub(super) fn first_argument_with_matching_function<'a>(
    name: &str,
    func: &Expr,
    arguments: &'a Arguments,
) -> Option<&'a Expr> {
    if func.as_name_expr().is_some_and(|func| func.id == name) {
        arguments.args().next()
    } else {
        None
    }
}
