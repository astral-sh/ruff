use ruff_db::diagnostic::Diagnostic;
use ruff_python_ast::name::QualifiedName;
use ruff_python_ast::{self as ast, Expr};
use ruff_python_semantic::SemanticModel;
use ruff_python_semantic::analyze::function_type::is_subject_to_liskov_substitution_principle;

use crate::checkers::ast::Checker;
use crate::settings::LinterSettings;

/// Returns `true` if a function call is allowed to use a boolean trap.
fn is_allowed_func_call(name: &str) -> bool {
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

/// Returns `true` if a call is semantically allowed to use a boolean trap.
fn is_semantically_allowed_func_call(call: &ast::ExprCall, semantic: &SemanticModel) -> bool {
    semantic
        .resolve_qualified_name(call.func.as_ref())
        .is_some_and(|qualified_name| {
            ["multiprocessing.Value"]
                .iter()
                .map(|target| QualifiedName::from_dotted_name(target))
                .any(|target| qualified_name == target)
        })
}

/// Returns `true` if a call is allowed by the user to use a boolean trap.
fn is_user_allowed_func_call(
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

/// Returns `true` if a function defines a binary operator.
///
/// This only includes operators, i.e., functions that are usually not called directly.
///
/// See: <https://docs.python.org/3/library/operator.html>
fn is_operator_method(name: &str) -> bool {
    match name {
        // Membership (`in`).
        "__contains__" => true,
        // Item access (`[]`, `[]=`, and `del []`).
        "__getitem__" | "__setitem__" | "__delitem__" => true,
        // Addition (`+` and `+=`).
        "__add__" | "__radd__" | "__iadd__" => true,
        // Subtraction (`-` and `-=`).
        "__sub__" | "__rsub__" | "__isub__" => true,
        // Multiplication (`*` and `*=`).
        "__mul__" | "__rmul__" | "__imul__" => true,
        // Division (`/` and `/=`).
        "__truediv__" | "__rtruediv__" | "__itruediv__" => true,
        // Floor division (`//` and `//=`).
        "__floordiv__" | "__rfloordiv__" | "__ifloordiv__" => true,
        // Remainder (`%` and `%=`).
        "__mod__" | "__rmod__" | "__imod__" => true,
        // Exponentiation (`**` and `**=`).
        "__pow__" | "__rpow__" | "__ipow__" => true,
        // Left shift (`<<` and `<<=`).
        "__lshift__" | "__rlshift__" | "__ilshift__" => true,
        // Right shift (`>>` and `>>=`).
        "__rshift__" | "__rrshift__" | "__irshift__" => true,
        // Matrix multiplication (`@` and `@=`).
        "__matmul__" | "__rmatmul__" | "__imatmul__" => true,
        // Meet (`&` and `&=`).
        "__and__" | "__rand__" | "__iand__" => true,
        // Join (`|` and `|=`).
        "__or__" | "__ror__" | "__ior__" => true,
        // Exclusive-or (`^` and `^=`).
        "__xor__" | "__rxor__" | "__ixor__" => true,
        // Comparison (`>`, `<`, `>=`, `<=`, `==`, and `!=`).
        "__gt__" | "__lt__" | "__ge__" | "__le__" | "__eq__" | "__ne__" => true,
        // Unary operators (`+`, `-`, and `~`), included for completeness.
        "__pos__" | "__neg__" | "__invert__" => true,
        _ => false,
    }
}

/// Returns `true` if a function definition is allowed to use a boolean trap.
pub(super) fn is_allowed_func_def(name: &str) -> bool {
    matches!(name, "__post_init__") || is_operator_method(name)
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

    // If the function is explicitly allowed, then the boolean trap is allowed.
    if is_semantically_allowed_func_call(call, checker.semantic()) {
        return true;
    }

    // If the call is explicitly allowed by the user, then the boolean trap is allowed.
    if is_user_allowed_func_call(call, checker.semantic(), checker.settings()) {
        return true;
    }

    false
}

pub(super) fn add_liskov_substitution_principle_help(
    diagnostic: &mut Diagnostic,
    function_name: &str,
    decorator_list: &[ast::Decorator],
    checker: &Checker,
) {
    let semantic = checker.semantic();
    let parent_scope = semantic.current_scope();
    let pep8_settings = &checker.settings().pep8_naming;
    if is_subject_to_liskov_substitution_principle(
        function_name,
        decorator_list,
        parent_scope,
        semantic,
        &pep8_settings.classmethod_decorators,
        &pep8_settings.staticmethod_decorators,
    ) {
        diagnostic.help(
            "Consider adding `@typing.override` if changing the function signature \
                would violate the Liskov Substitution Principle",
        );
    }
}
