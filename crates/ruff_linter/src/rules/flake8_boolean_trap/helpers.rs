use ruff_python_ast::{self as ast, Constant, Expr};

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
            | "index"
            | "insert"
            | "int"
            | "is_"
            | "is_not"
            | "next"
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
pub(super) fn allow_boolean_trap(func: &Expr) -> bool {
    if let Expr::Attribute(ast::ExprAttribute { attr, .. }) = func {
        return is_allowed_func_call(attr);
    }

    if let Expr::Name(ast::ExprName { id, .. }) = func {
        return is_allowed_func_call(id);
    }

    false
}

/// Returns `true` if an expression is a boolean literal.
pub(super) const fn is_boolean(expr: &Expr) -> bool {
    matches!(
        &expr,
        Expr::Constant(ast::ExprConstant {
            value: Constant::Bool(_),
            ..
        })
    )
}
