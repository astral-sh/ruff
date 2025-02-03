use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, ViolationMetadata};
use ruff_python_ast::StmtFunctionDef;
use ruff_python_semantic::{
    Binding, BindingId, BindingKind, ScopeId, ScopeKind, Scopes, SemanticModel,
    TypingOnlyBindingsStatus,
};
use ruff_text_size::{Ranged, TextRange};

use crate::checkers::ast::Checker;
use crate::rules::flake8_pytest_style::rules::fixture_decorator;

/// ## What it does
/// Checks for parameters that have the same name as a symbol from outer scopes.
///
/// ## Why is this bad?
/// Having two symbols with the same name is confusing and may lead to bugs.
///
/// As an exception, parameters referencing Pytest fixtures are ignored.
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
    let function = binding.statement(semantic)?.as_function_def_stmt()?;

    let (parent_scope, overshadowed) = find_overshadowed_binding(binding, function, checker)?;

    if parent_scope.is_global() && binding_is_pytest_fixture(overshadowed, semantic) {
        return None;
    }

    Some(Diagnostic::new(OvershadowingParameter, binding.range))
}

/// Look for an existing binding from outer scopes with the same name as `binding`.
///
/// If the parent scope has that name, return its binding.
/// Otherwise, use [`SemanticModel::simulate_runtime_load_at_location_in_scope`].
fn find_overshadowed_binding(
    binding: &Binding,
    function: &StmtFunctionDef,
    checker: &Checker,
) -> Option<(ScopeId, BindingId)> {
    let semantic = checker.semantic();
    let source = checker.source();
    let scopes = &semantic.scopes;

    let parent_scope_id = actual_parent_scope(binding, scopes)?;

    let binding_name = binding.name(source);
    let parent_scope = &scopes[parent_scope_id];

    if let Some(overshadowed) = parent_scope.get(binding_name) {
        return Some((parent_scope_id, overshadowed));
    }

    let overshadowed = semantic.simulate_runtime_load_at_location_in_scope(
        binding_name,
        TextRange::at(function.start(), 1.into()),
        parent_scope_id,
        TypingOnlyBindingsStatus::Allowed,
    )?;

    Some((parent_scope_id, overshadowed))
}

/// Return the grandparent scope of the function scope
/// in which `parameter` is defined.
///
/// This is necessary since the immediate parent scope of a function scope
/// is always its "type" scope (see [`BindingKind::TypeParam`]).
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
