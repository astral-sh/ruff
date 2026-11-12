use ruff_python_ast::Arguments;
use ruff_python_ast::Expr;

pub(super) fn exactly_one_argument_with_matching_function<'a>(
    name: &str,
    func: &Expr,
    arguments: &'a Arguments,
) -> Option<&'a Expr> {
    let arg = arguments.as_single_positional()?;
    (func.as_name_expr()?.id == name).then_some(arg)
}
