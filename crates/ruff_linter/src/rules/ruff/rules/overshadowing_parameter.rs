use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, ViolationMetadata};
use ruff_python_semantic::{
    Binding, BindingId, BindingKind, ScopeId, ScopeKind, Scopes, SemanticModel,
};

use crate::checkers::ast::Checker;
use crate::rules::flake8_pytest_style::rules::fixture_decorator;

/// ## What it does
/// Checks for parameters that have the same name as a symbol from outer scopes.
///
/// ## Why is this bad?
/// Having two symbols with the same name is confusing and may lead to bugs.
///
/// As an exception, parameters referencing Pytest fixtures are ignored.
/// Parameters shadowing built-in symbols (e.g., `id` or `type`)
/// are also not reported, as they are within the scope of [`builtin-argument-shadowing`][A002].
///
/// ## Example
///
/// ```python
/// a = 1
///
///
/// def f(a):
///     print(a)
/// ```
///
/// Use instead:
///
/// ```python
/// a = 1
///
///
/// def f(b):
///     print(b)
/// ```
///
/// [A002]: https://docs.astral.sh/ruff/rules/builtin-argument-shadowing
#[derive(ViolationMetadata)]
pub(crate) struct OvershadowingParameter;

impl Violation for OvershadowingParameter {
    #[derive_message_formats]
    fn message(&self) -> String {
        "Parameter overshadows symbol from outer scope".to_string()
    }

    fn fix_title(&self) -> Option<String> {
        Some("Rename parameter".to_string())
    }
}

/// RUF059
pub(crate) fn overshadowing_parameter(checker: &Checker, binding: &Binding) -> Option<Diagnostic> {
    if !matches!(binding.kind, BindingKind::Argument) {
        return None;
    }

    let semantic = checker.semantic();
    let (parent_scope, overshadowed) = find_overshadowed_binding(binding, checker)?;

    if parent_scope.is_global() && binding_is_pytest_fixture(overshadowed, semantic) {
        return None;
    }

    // Parameters shadowing builtins are already reported by A005
    if semantic.binding(overshadowed).kind.is_builtin() {
        return None;
    }

    Some(Diagnostic::new(OvershadowingParameter, binding.range))
}

/// Look for an existing binding from outer scopes
/// with the same name as `binding`
/// using [`SemanticModel::lookup_symbol_in_scope`].
///
/// If the parent scope is that of a class, it is skipped.
/// This is because from within the function,
/// a class-based binding cannot be accessed directly,
/// except for class-level type parameters,
/// which are bound to [`ScopeKind::Type`] scopes.
///
/// For example:
///
/// ```python
/// class C[T]:
///     def __init__(self, a, T):  # Same as `T` the type parameter
///         self._a = a  # Not `a` the property/method
///
///     @property
///     def a(self): ...
/// ```
fn find_overshadowed_binding(binding: &Binding, checker: &Checker) -> Option<(ScopeId, BindingId)> {
    let semantic = checker.semantic();
    let source = checker.source();
    let scopes = &semantic.scopes;

    let parent_scope_id = actual_parent_scope(binding, scopes)?;

    let binding_name = binding.name(source);
    let parent_scope = &scopes[parent_scope_id];

    let scope_to_start_searching_in = if parent_scope.kind.is_class() {
        parent_scope.parent?
    } else {
        parent_scope_id
    };

    let overshadowed =
        semantic.lookup_symbol_in_scope(binding_name, scope_to_start_searching_in, false)?;

    Some((parent_scope_id, overshadowed))
}

/// Return the grandparent scope of the function scope
/// in which `parameter` is defined.
///
/// This is necessary since the immediate parent scope of a function scope
/// is always its "type" scope (see [`ScopeKind::Type`]).
fn actual_parent_scope(parameter: &Binding, scopes: &Scopes) -> Option<ScopeId> {
    let current = &scopes[parameter.scope];

    let type_scope_id = current.parent?;
    let type_scope = &scopes[type_scope_id];

    type_scope.parent
}

fn binding_is_pytest_fixture(id: BindingId, semantic: &SemanticModel) -> bool {
    let binding = semantic.binding(id);

    let BindingKind::FunctionDefinition(scope_id) = binding.kind else {
        return false;
    };

    let ScopeKind::Function(function_def) = semantic.scopes[scope_id].kind else {
        return false;
    };

    fixture_decorator(&function_def.decorator_list, semantic).is_some()
}
