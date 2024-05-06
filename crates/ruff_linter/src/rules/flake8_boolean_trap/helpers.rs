use ruff_python_ast::name::QualifiedName;
use ruff_python_ast::{self as ast, Expr};
use ruff_python_semantic::SemanticModel;

use crate::checkers::ast::Checker;
use crate::settings::LinterSettings;

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

/// Returns `true` if a call is allowed by the user to use a boolean trap.
pub(super) fn is_user_allowed_func_call(
    call: &ast::ExprCall,
    semantic: &SemanticModel,
    settings: &LinterSettings,
) -> bool {
    semantic
        .resolve_qualified_name(call.func.as_ref())
        .is_some_and(|qualified_name| {
            settings
                .flake8_boolean_trap
                .extend_allowed_calls
                .iter()
                .map(|target| QualifiedName::from_dotted_name(target))
                .any(|target| qualified_name == target)
        })
}

/// Returns `true` if a function definition is allowed to use a boolean trap.
pub(super) fn is_allowed_func_def(name: &str) -> bool {
    matches!(name, "__setitem__" | "__post_init__")
}

/// Returns `true` if an argument is allowed to use a boolean trap. To return
/// `true`, the function name must be explicitly allowed, and the argument must
/// be either the first or second argument in the call.
pub(super) fn allow_boolean_trap(call: &ast::ExprCall, checker: &Checker) -> bool {
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
        // Ex) `foo.set(True)`
        if func_name == "set" {
            return true;
        }

        // Ex) `foo.set_visible(True)`
        if func_name
            .strip_prefix("set")
            .is_some_and(|suffix| suffix.starts_with(|c: char| c == '_' || c.is_ascii_uppercase()))
        {
            return true;
        }
    }

    // If the call is explicitly allowed by the user, then the boolean trap is allowed.
    if is_user_allowed_func_call(call, checker.semantic(), checker.settings) {
        return true;
    }

    false
}
