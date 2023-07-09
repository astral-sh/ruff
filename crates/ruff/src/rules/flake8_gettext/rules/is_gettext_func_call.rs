use rustpython_parser::ast::{self, Expr};

/// Returns true if the [`Expr`] is an internationalization function call.
pub(crate) fn is_gettext_func_call(func: &Expr, functions_names: &[String]) -> bool {
    if let Expr::Name(ast::ExprName { id, .. }) = func {
        functions_names.contains(id)
    } else {
        false
    }
}
