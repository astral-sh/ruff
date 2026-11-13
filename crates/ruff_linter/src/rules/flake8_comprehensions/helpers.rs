use ruff_python_ast::{Expr, ExprCallFields};

pub(super) fn exactly_one_argument_with_matching_function<'a>(
    name: &str,
    call: &ExprCallFields<'a>,
) -> Option<Expr<'a>> {
    let arg = call.arguments.as_single_positional()?;
    (call.func.as_name_expr()?.id() == name).then_some(arg)
}
