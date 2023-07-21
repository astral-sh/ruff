use rustpython_parser::ast::{self, Constant, Expr, Ranged};

use ruff_diagnostics::{Diagnostic, DiagnosticKind};

use crate::checkers::ast::Checker;

/// Returns `true` if a function call is allowed to use a boolean trap.
pub(super) fn is_allowed_func_call(name: &str) -> bool {
    matches!(
        name,
        "append"
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
            | "param"
            | "pop"
            | "remove"
            | "set_blocking"
            | "set_enabled"
            | "setattr"
            | "__setattr__"
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

const fn is_boolean_arg(arg: &Expr) -> bool {
    matches!(
        &arg,
        Expr::Constant(ast::ExprConstant {
            value: Constant::Bool(_),
            ..
        })
    )
}

pub(super) fn add_if_boolean(checker: &mut Checker, arg: &Expr, kind: DiagnosticKind) {
    if is_boolean_arg(arg) {
        checker.diagnostics.push(Diagnostic::new(kind, arg.range()));
    }
}
