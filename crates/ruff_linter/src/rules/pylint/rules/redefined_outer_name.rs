use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, ViolationMetadata};
use ruff_python_ast::{Expr, Stmt};
use ruff_python_semantic::{
    Binding, BindingId, BindingKind, Scope, ScopeId, ScopeKind, Scopes, SemanticModel,
    TypingOnlyBindingsStatus,
};
use ruff_source_file::LineRanges;
use ruff_text_size::{Ranged, TextRange, TextSize};

use crate::checkers::ast::Checker;
use crate::rules::flake8_pytest_style::rules::fixture_decorator;

/// ## What it does
/// Checks for variables that share the same name as a name defined in an outer scope.
///
/// ## Why is this bad?
/// Having two symbols with the same name is confusing and may lead to bugs.
/// Currently, this rule only report parameters.
///
/// Exceptions to this rule include:
/// * Parameters referencing Pytest fixtures.
/// * Variables shadowing built-in symbols (e.g., `id` or `type`),
///   as they are within the scope of [`builtin-argument-shadowing`][A002].
/// * Variables shadowing a `__future__` import.
/// * Variables whose names match [`lint.dummy-variable-rgx`].
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
/// ## Options
/// - `lint.dummy-variable-rgx`
///
/// [A002]: https://docs.astral.sh/ruff/rules/builtin-argument-shadowing
#[derive(ViolationMetadata)]
pub(crate) struct RedefinedOuterName {
    name: String,
    overshadowed_line: u32,
}

impl Violation for RedefinedOuterName {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!(
            "Redefining name `{}` from outer scope (line {})",
            self.name, self.overshadowed_line
        )
    }

    fn fix_title(&self) -> Option<String> {
        Some("Rename parameter, or remove it if they are the same object".to_string())
    }
}

/// PLW0621
pub(crate) fn redefined_outer_name(checker: &Checker, binding: &Binding) -> Option<Diagnostic> {
    if !matches!(binding.kind, BindingKind::Argument) {
        return None;
    }

    redefined_outer_name_parameter(checker, binding)
}

fn redefined_outer_name_parameter(checker: &Checker, binding: &Binding) -> Option<Diagnostic> {
    let binding_name = binding.name(checker.source());

    if checker.settings.dummy_variable_rgx.is_match(binding_name) {
        return None;
    }

    let semantic = checker.semantic();
    let (parent_scope, overshadowed) = find_overshadowed_binding(binding, checker)?;

    if parent_scope.is_global() && binding_is_pytest_fixture(overshadowed, semantic) {
        return None;
    }

    let overshadowed = semantic.binding(overshadowed);

    // Parameters shadowing builtins are already reported by A005
    if overshadowed.kind.is_builtin() {
        return None;
    }

    if overshadowed.kind.is_future_import() {
        return None;
    }

    let source = checker.source();
    let kind = RedefinedOuterName {
        name: binding.name(source).to_string(),
        overshadowed_line: source.count_lines(TextRange::up_to(overshadowed.start())) + 1,
    };

    Some(Diagnostic::new(kind, binding.range))
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

    let current_scope = &scopes[binding.scope];
    let parent_scope_id = actual_parent_scope(current_scope, scopes)?;
    let parent_scope = &scopes[parent_scope_id];

    let scope_to_start_searching_in = if parent_scope.kind.is_class() {
        parent_scope.parent?
    } else {
        parent_scope_id
    };

    let binding_name = binding.name(source);

    if let Some(overshadowed) =
        semantic.lookup_symbol_in_scope(binding_name, scope_to_start_searching_in, false)
    {
        return Some((parent_scope_id, overshadowed));
    }

    if parent_scope.kind.is_class() {
        return None;
    }

    let overshadowed =
        find_exception_binding(binding, binding_name, current_scope.parent?, semantic)?;

    Some((parent_scope_id, overshadowed))
}

fn find_exception_binding(
    binding: &Binding,
    binding_name: &str,
    scope: ScopeId,
    semantic: &SemanticModel,
) -> Option<BindingId> {
    let overshadowed = semantic.simulate_runtime_load_at_location_in_scope(
        binding_name,
        TextRange::at(scope_start_offset(binding, semantic)?, 0.into()),
        scope,
        TypingOnlyBindingsStatus::Disallowed,
    )?;

    if !semantic.binding(overshadowed).kind.is_bound_exception() {
        return None;
    }

    Some(overshadowed)
}

/// If the current scope is that of a lambda, return the parent scope.
/// If the current scope is that of a function, return the grandparent scope.
///
/// This is necessary since the immediate parent of a function scope
/// is always its "type" scope (see [`ScopeKind::Type`]).
/// On the other hand, lambdas do not have such a wrapper scope.
fn actual_parent_scope(current: &Scope, scopes: &Scopes) -> Option<ScopeId> {
    match current.kind {
        ScopeKind::Lambda(_) => current.parent,
        ScopeKind::Function(_) => {
            let type_scope_id = current.parent?;
            let type_scope = &scopes[type_scope_id];

            type_scope.parent
        }
        _ => None,
    }
}

/// Return the position right before the function/lambda
/// the parameter is defined in.
fn scope_start_offset(parameter: &Binding, semantic: &SemanticModel) -> Option<TextSize> {
    if let Some(Expr::Lambda(lambda)) = parameter.expression(semantic) {
        return Some(lambda.start());
    }

    if let Some(Stmt::FunctionDef(function)) = parameter.statement(semantic) {
        return Some(function.start());
    }

    None
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
