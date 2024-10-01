use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::name::UnqualifiedName;
use ruff_python_ast::{self as ast, Expr};
use ruff_python_semantic::{BindingKind, ScopeKind};
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;

/// ## What it does
/// Checks for accesses on "private" class members.
///
/// ## Why is this bad?
/// In Python, the convention is such that class members that are prefixed
/// with a single underscore, or prefixed but not suffixed with a double
/// underscore, are considered private and intended for internal use.
///
/// Using such "private" members is considered a misuse of the class, as
/// there are no guarantees that the member will be present in future
/// versions, that it will have the same type, or that it will have the same
/// behavior. Instead, use the class's public interface.
///
/// This rule ignores accesses on dunder methods (e.g., `__init__`) and sunder
/// methods (e.g., `_missing_`).
///
/// ## Example
/// ```python
/// class Class:
///     def __init__(self):
///         self._private_member = "..."
///
///
/// var = Class()
/// print(var._private_member)
/// ```
///
/// Use instead:
/// ```python
/// class Class:
///     def __init__(self):
///         self.public_member = "..."
///
///
/// var = Class()
/// print(var.public_member)
/// ```
///
/// ## Options
/// - `lint.flake8-self.ignore-names`
///
/// ## References
/// - [_What is the meaning of single or double underscores before an object name?_](https://stackoverflow.com/questions/1301346/what-is-the-meaning-of-single-and-double-underscore-before-an-object-name)
#[violation]
pub struct PrivateMemberAccess {
    access: String,
}

impl Violation for PrivateMemberAccess {
    #[derive_message_formats]
    fn message(&self) -> String {
        let PrivateMemberAccess { access } = self;
        format!("Private member accessed: `{access}`")
    }
}

/// SLF001
pub(crate) fn private_member_access(checker: &mut Checker, expr: &Expr) {
    let Expr::Attribute(ast::ExprAttribute { value, attr, .. }) = expr else {
        return;
    };

    if checker.semantic().in_annotation() {
        return;
    }

    // Ignore non-private accesses.
    if !attr.starts_with('_') {
        return;
    }

    // Ignore dunder accesses.
    let is_dunder = attr.starts_with("__") && attr.ends_with("__");
    if is_dunder {
        return;
    }

    // Ignore sunder accesses.
    let is_sunder = attr.starts_with('_')
        && attr.ends_with('_')
        && !attr.starts_with("__")
        && !attr.ends_with("__");
    if is_sunder {
        return;
    }

    if checker
        .settings
        .flake8_self
        .ignore_names
        .contains(attr.id())
    {
        return;
    }

    // Ignore accesses on instances within special methods (e.g., `__eq__`).
    if let ScopeKind::Function(ast::StmtFunctionDef { name, .. }) =
        checker.semantic().current_scope().kind
    {
        if matches!(
            name.as_str(),
            "__lt__"
                | "__le__"
                | "__eq__"
                | "__ne__"
                | "__gt__"
                | "__ge__"
                | "__add__"
                | "__sub__"
                | "__mul__"
                | "__matmul__"
                | "__truediv__"
                | "__floordiv__"
                | "__mod__"
                | "__divmod__"
                | "__pow__"
                | "__lshift__"
                | "__rshift__"
                | "__and__"
                | "__xor__"
                | "__or__"
                | "__radd__"
                | "__rsub__"
                | "__rmul__"
                | "__rmatmul__"
                | "__rtruediv__"
                | "__rfloordiv__"
                | "__rmod__"
                | "__rdivmod__"
                | "__rpow__"
                | "__rlshift__"
                | "__rrshift__"
                | "__rand__"
                | "__rxor__"
                | "__ror__"
                | "__iadd__"
                | "__isub__"
                | "__imul__"
                | "__imatmul__"
                | "__itruediv__"
                | "__ifloordiv__"
                | "__imod__"
                | "__ipow__"
                | "__ilshift__"
                | "__irshift__"
                | "__iand__"
                | "__ixor__"
                | "__ior__"
        ) {
            return;
        }
    }

    // Allow some documented private methods, like `os._exit()`.
    if let Some(qualified_name) = checker.semantic().resolve_qualified_name(expr) {
        if matches!(qualified_name.segments(), ["os", "_exit"]) {
            return;
        }
    }

    if let Expr::Call(ast::ExprCall { func, .. }) = value.as_ref() {
        // Ignore `super()` calls.
        if let Some(name) = UnqualifiedName::from_expr(func) {
            if matches!(name.segments(), ["super"]) {
                return;
            }
        }
    }

    if let Some(name) = UnqualifiedName::from_expr(value) {
        // Ignore `self` and `cls` accesses.
        if matches!(name.segments(), ["self" | "cls" | "mcs"]) {
            return;
        }
    }

    if let Expr::Name(name) = value.as_ref() {
        // Ignore accesses on class members from _within_ the class.
        if checker
            .semantic()
            .resolve_name(name)
            .and_then(|id| {
                if let BindingKind::ClassDefinition(scope) = checker.semantic().binding(id).kind {
                    Some(scope)
                } else {
                    None
                }
            })
            .is_some_and(|scope| {
                checker
                    .semantic()
                    .current_scope_ids()
                    .any(|parent| scope == parent)
            })
        {
            return;
        }
    }

    checker.diagnostics.push(Diagnostic::new(
        PrivateMemberAccess {
            access: attr.to_string(),
        },
        expr.range(),
    ));
}
