use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, ViolationMetadata};
use ruff_python_ast::helpers::{is_dunder, is_sunder};
use ruff_python_ast::name::UnqualifiedName;
use ruff_python_ast::{self as ast, Expr};
use ruff_python_semantic::analyze::typing;
use ruff_python_semantic::analyze::typing::TypeChecker;
use ruff_python_semantic::{BindingKind, ScopeKind, SemanticModel};
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;
use crate::rules::pylint::helpers::is_dunder_operator_method;

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
#[derive(ViolationMetadata)]
pub(crate) struct PrivateMemberAccess {
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
pub(crate) fn private_member_access(checker: &Checker, expr: &Expr) {
    let Expr::Attribute(ast::ExprAttribute { value, attr, .. }) = expr else {
        return;
    };

    let semantic = checker.semantic();
    let current_scope = semantic.current_scope();

    if semantic.in_annotation() {
        return;
    }

    if !attr.starts_with('_') || is_dunder(attr) || is_sunder(attr) {
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
    if let ScopeKind::Function(ast::StmtFunctionDef { name, .. }) = current_scope.kind {
        if is_dunder_operator_method(name) {
            return;
        }
    }

    // Allow some documented private methods, like `os._exit()`.
    if let Some(qualified_name) = semantic.resolve_qualified_name(expr) {
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
        if semantic
            .resolve_name(name)
            .and_then(|id| {
                if let BindingKind::ClassDefinition(scope) = semantic.binding(id).kind {
                    Some(scope)
                } else {
                    None
                }
            })
            .is_some_and(|scope| semantic.current_scope_ids().any(|parent| scope == parent))
        {
            return;
        }

        if is_same_class_instance(name, semantic) {
            return;
        }
    }

    checker.report_diagnostic(Diagnostic::new(
        PrivateMemberAccess {
            access: attr.to_string(),
        },
        expr.range(),
    ));
}

/// Check for the following cases:
///
/// * Parameter annotation:
///
///     ```python
///     class C[T]:
///         def f(self, other: C): ...
///         def f(self, other: C[...]): ...
///         def f(self, other: Annotated[C, ...]): ...
///     ```
///
/// * `super().__new__`/`cls` call:
///
///     ```python
///     class C:
///         def __new__(cls): ...
///             instance = super().__new__(cls)
///         @classmethod
///         def m(cls):
///             instance = cls()
///     ```
///
/// This function is intentionally naive and does not handle more complex cases.
/// It is expected to be expanded overtime, possibly when type-aware APIs are available.
fn is_same_class_instance(name: &ast::ExprName, semantic: &SemanticModel) -> bool {
    let Some(binding_id) = semantic.resolve_name(name) else {
        return false;
    };

    let binding = semantic.binding(binding_id);
    typing::check_type::<SameClassInstanceChecker>(binding, semantic)
}

struct SameClassInstanceChecker;

impl SameClassInstanceChecker {
    /// Whether `name` resolves to a class which the semantic model is traversing.
    fn is_current_class_name(name: &ast::ExprName, semantic: &SemanticModel) -> bool {
        semantic.current_scopes().any(|scope| {
            let ScopeKind::Class(class) = scope.kind else {
                return false;
            };

            class.name.id == name.id
        })
    }
}

impl TypeChecker for SameClassInstanceChecker {
    /// `C`, `C[T]`, `Annotated[C, ...]`, `Annotated[C[T], ...]`
    fn match_annotation(annotation: &Expr, semantic: &SemanticModel) -> bool {
        let Some(class_name) = find_class_name(annotation, semantic) else {
            return false;
        };

        Self::is_current_class_name(class_name, semantic)
    }

    /// `cls()`, `C()`, `C[T]()`, `super().__new__()`
    fn match_initializer(initializer: &Expr, semantic: &SemanticModel) -> bool {
        let Expr::Call(call) = initializer else {
            return false;
        };

        match &*call.func {
            Expr::Subscript(_) => Self::match_annotation(&call.func, semantic),

            Expr::Name(name) => {
                matches!(&*name.id, "cls" | "mcs") || Self::is_current_class_name(name, semantic)
            }

            Expr::Attribute(ast::ExprAttribute { value, attr, .. }) => {
                let Expr::Call(ast::ExprCall { func, .. }) = &**value else {
                    return false;
                };

                let Expr::Name(ast::ExprName { id: func, .. }) = &**func else {
                    return false;
                };

                func == "super" && attr == "__new__"
            }

            _ => false,
        }
    }
}

/// Convert `Annotated[C[T], ...]` to `C` (and similar) to `C` recursively.
fn find_class_name<'a>(expr: &'a Expr, semantic: &'a SemanticModel) -> Option<&'a ast::ExprName> {
    match expr {
        Expr::Name(name) => Some(name),
        Expr::Subscript(ast::ExprSubscript { value, slice, .. }) => {
            if semantic.match_typing_expr(value, "Annotated") {
                let [expr, ..] = &slice.as_tuple_expr()?.elts[..] else {
                    return None;
                };

                return find_class_name(expr, semantic);
            }

            find_class_name(value, semantic)
        }
        _ => None,
    }
}
