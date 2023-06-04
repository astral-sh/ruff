use rustpython_parser::ast::{self, Constant, Expr, Ranged};

use ruff_diagnostics::{Diagnostic, DiagnosticKind};

use crate::checkers::ast::Checker;

pub(super) const FUNC_CALL_NAME_ALLOWLIST: &[&str] = &[
    "append",
    "assertEqual",
    "assertEquals",
    "assertNotEqual",
    "assertNotEquals",
    "bytes",
    "count",
    "failIfEqual",
    "failUnlessEqual",
    "float",
    "fromkeys",
    "get",
    "getattr",
    "getboolean",
    "getfloat",
    "getint",
    "index",
    "insert",
    "int",
    "param",
    "pop",
    "remove",
    "setattr",
    "__setattr__",
    "setdefault",
    "str",
];

pub(super) const FUNC_DEF_NAME_ALLOWLIST: &[&str] = &["__setitem__"];

/// Returns `true` if an argument is allowed to use a boolean trap. To return
/// `true`, the function name must be explicitly allowed, and the argument must
/// be either the first or second argument in the call.
pub(super) fn allow_boolean_trap(func: &Expr) -> bool {
    if let Expr::Attribute(ast::ExprAttribute { attr, .. }) = func {
        return FUNC_CALL_NAME_ALLOWLIST.contains(&attr.as_ref());
    }

    if let Expr::Name(ast::ExprName { id, .. }) = func {
        return FUNC_CALL_NAME_ALLOWLIST.contains(&id.as_ref());
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
