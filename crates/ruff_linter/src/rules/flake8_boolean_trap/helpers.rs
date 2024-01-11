use ruff_python_ast::{self as ast, Expr};

/// Returns `true` if a function call is allowed to use a boolean trap.
pub(super) fn is_allowed_func_call(name: &str) -> bool {
    matches!(
        name,
        "__setattr__"
            | "append"
            | "assertEqual"
            | "assertEquals"
            | "assertNotEqual"
            | "assertNotEquals"
            | "bool"
            | "bytes"
            | "coalesce"
            | "count"
            | "failIfEqual"
            | "failUnlessEqual"
            | "float"
            | "fromkeys"
            | "get"
            | "getattr"
            | "getboolean"
            | "getfloat"
            | "getint"
            | "ifnull"
            | "index"
            | "insert"
            | "int"
            | "is_"
            | "is_not"
            | "isnull"
            | "next"
            | "nvl"
            | "param"
            | "pop"
            | "remove"
            | "set_blocking"
            | "set_enabled"
            | "setattr"
            | "setdefault"
            | "str"
    )
}

/// Returns `true` if a function definition is allowed to use a boolean trap.
pub(super) fn is_allowed_func_def(name: &str) -> bool {
    matches!(name, "__setitem__")
}

/// Returns `true` if an argument is allowed to use a boolean trap. To return
/// `true`, the function name must be explicitly allowed, and the argument must
/// be either the first or second argument in the call.
pub(super) fn allow_boolean_trap(call: &ast::ExprCall) -> bool {
    let func_name = match call.func.as_ref() {
        Expr::Attribute(ast::ExprAttribute { attr, .. }) => attr.as_str(),
        Expr::Name(ast::ExprName { id, .. }) => id.as_str(),
        _ => return false,
    };

    // If the function name is explicitly allowed, then the boolean trap is
    // allowed.
    if is_allowed_func_call(func_name) {
        return true;
    }

    // If the function appears to be a setter (e.g., `set_visible` or `setVisible`), then the
    // boolean trap is allowed. We want to avoid raising a violation for cases in which the argument
    // is positional-only and third-party, and this tends to be the case for setters.
    if call.arguments.args.len() == 1 {
        if func_name
            .strip_prefix("set")
            .is_some_and(|suffix| suffix.starts_with(|c: char| c == '_' || c.is_ascii_uppercase()))
        {
            return true;
        }
    }

    false
}
