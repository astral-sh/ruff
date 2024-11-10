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

/// Returns `true` if a function defines a binary operator.
///
/// This only includes operators, i.e., functions that are usually not called directly.
///
/// See: <https://docs.python.org/3/library/operator.html>
pub(super) fn is_operator_method(name: &str) -> bool {
    matches!(
        name,
        "__contains__"  // in
            // item access ([])
            | "__getitem__"  // []
            | "__setitem__"  // []=
            | "__delitem__"  // del []
            // addition (+)
            | "__add__"  // +
            | "__radd__"  // +
            | "__iadd__"  // +=
            // subtraction (-)
            | "__sub__"  // -
            | "__rsub__"  // -
            | "__isub__"  // -=
            // multiplication (*)
            | "__mul__"  // *
            | "__rmul__"  // *
            | "__imul__"  // *=
            // division (/)
            | "__truediv__"  // /
            | "__rtruediv__"  // /
            | "__itruediv__"  // /=
            // floor division (//)
            | "__floordiv__"  // //
            | "__rfloordiv__"  // //
            | "__ifloordiv__"  // //=
            // remainder (%)
            | "__mod__"  // %
            | "__rmod__"  // %
            | "__imod__"  // %=
            // exponentiation (**)
            | "__pow__"  // **
            | "__rpow__"  // **
            | "__ipow__"  // **=
            // left shift (<<)
            | "__lshift__"  // <<
            | "__rlshift__"  // <<
            | "__ilshift__"  // <<=
            // right shift (>>)
            | "__rshift__"  // >>
            | "__rrshift__"  // >>
            | "__irshift__"  // >>=
            // matrix multiplication (@)
            | "__matmul__"  // @
            | "__rmatmul__"  // @
            | "__imatmul__"  // @=
            // meet (&)
            | "__and__"  // &
            | "__rand__"  // &
            | "__iand__"  // &=
            // join (|)
            | "__or__"  // |
            | "__ror__"  // |
            | "__ior__"  // |=
            // xor (^)
            | "__xor__"  // ^
            | "__rxor__"  // ^
            | "__ixor__"  // ^=
            // comparison (>, <, >=, <=, ==, !=)
            | "__gt__"  // >
            | "__lt__"  // <
            | "__ge__"  // >=
            | "__le__"  // <=
            | "__eq__"  // ==
            | "__ne__" // !=
            // unary operators (included for completeness)
            | "__pos__"  // +
            | "__neg__"  // -
            | "__invert__" // ~
    )
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

    // If the call is explicitly allowed by the user, then the boolean trap is allowed.
    if is_user_allowed_func_call(call, checker.semantic(), checker.settings) {
        return true;
    }

    false
}
