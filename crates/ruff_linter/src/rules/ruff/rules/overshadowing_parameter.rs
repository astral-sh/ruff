use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, ViolationMetadata};
use ruff_python_semantic::{Binding, BindingId, BindingKind, ScopeKind, SemanticModel};

use crate::checkers::ast::Checker;
use crate::rules::flake8_pytest_style::rules::fixture_decorator;

/// ## What it does
/// Checks for parameters that have the same name as a symbol from outer scopes.
///
/// ## Why is this bad?
/// Having two symbols with the same name is confusing and may lead to bugs.
///
/// ## Example
///
/// ```python
/// a = 1
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
    let scopes = &semantic.scopes;

    let current_scope = &scopes[binding.scope];
    let parent_scope_id = current_scope.parent?;
    let parent_scope = &scopes[parent_scope_id];

    let binding_name = binding.name(checker.source());
    let overshadowed = parent_scope.get(binding_name)?;

    if parent_scope_id.is_global() && binding_is_pytest_fixture(overshadowed, semantic) {
        return None;
    }

    Some(Diagnostic::new(OvershadowingParameter, binding.range))
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
