use ruff_diagnostics::{Diagnostic, Fix, FixAvailability, Violation};
use ruff_macros::{derive_message_formats, ViolationMetadata};
use ruff_python_ast::helpers::is_dunder;
use ruff_python_semantic::{Binding, BindingKind, ScopeId, SemanticModel};
use ruff_python_stdlib::{
    builtins::is_python_builtin, identifiers::is_identifier, keyword::is_keyword,
};
use ruff_text_size::Ranged;

use crate::{checkers::ast::Checker, renamer::Renamer};

/// ## What it does
/// Checks for accesses of local dummy variables, excluding `_` and dunder variables.
///
/// By default, "dummy variables" are any variables with names that start with leading
/// underscores. However, this is customisable using the `dummy-variable-rgx` setting).
///
/// ## Why is this bad?
/// Marking a variable with a leading underscore conveys that it is intentionally unused within the function or method.
/// When these variables are later referenced in the code, it causes confusion and potential misunderstandings about
/// the code's intention. A variable marked as "unused" being subsequently used suggests oversight or unintentional use.
/// This detracts from the clarity and maintainability of the codebase.
///
/// Sometimes leading underscores are used to avoid variables shadowing other variables, Python builtins, or Python
/// keywords. However, [PEP 8] recommends to use trailing underscores for this rather than leading underscores.
///
/// ## Example
/// ```python
/// def function():
///     _variable = 3
///     return _variable + 1
/// ```
///
/// Use instead:
/// ```python
/// def function():
///     variable = 3
///     return variable + 1
/// ```
///
/// ## Fix availability
/// An fix is only available for variables that start with leading underscores.
///
/// ## Options
/// - [`lint.dummy-variable-rgx`]
///
///
/// [PEP 8]: https://peps.python.org/pep-0008/
#[derive(ViolationMetadata)]
pub(crate) struct DummyVariableAccessed {
    name: String,
    fix_kind: Option<ShadowedKind>,
}

impl Violation for DummyVariableAccessed {
    const FIX_AVAILABILITY: FixAvailability = FixAvailability::Sometimes;

    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Local dummy variable `{}` is accessed", self.name)
    }

    fn fix_title(&self) -> Option<String> {
        if let Some(fix_kind) = self.fix_kind {
            return Some(match fix_kind {
                ShadowedKind::BuiltIn => {
                    "Prefer using trailing underscores to avoid shadowing a built-in".to_string()
                }
                ShadowedKind::Keyword => {
                    "Prefer using trailing underscores to avoid shadowing a keyword".to_string()
                }
                ShadowedKind::Some => {
                    "Prefer using trailing underscores to avoid shadowing a variable".to_string()
                }
                ShadowedKind::None => "Remove leading underscores".to_string(),
            });
        }
        None
    }
}

/// RUF052
pub(crate) fn dummy_variable_accessed(checker: &Checker, binding: &Binding) -> Option<Diagnostic> {
    let name = binding.name(checker.source());

    // Ignore `_` and dunder variables
    if name == "_" || is_dunder(name) {
        return None;
    }
    // only used variables
    if binding.is_unused() {
        return None;
    }
    // Only variables defined via function arguments or assignments.
    if !matches!(
        binding.kind,
        BindingKind::Argument | BindingKind::Assignment
    ) {
        return None;
    }
    // This excludes `global` and `nonlocal` variables.
    if binding.is_global() || binding.is_nonlocal() {
        return None;
    }

    let semantic = checker.semantic();

    // Only variables defined in function scopes
    let scope = &semantic.scopes[binding.scope];
    if !scope.kind.is_function() {
        return None;
    }
    if !checker.settings.dummy_variable_rgx.is_match(name) {
        return None;
    }

    let possible_fix_kind = get_possible_fix_kind(name, checker, binding.scope);

    let mut diagnostic = Diagnostic::new(
        DummyVariableAccessed {
            name: name.to_string(),
            fix_kind: possible_fix_kind,
        },
        binding.range(),
    );

    // If fix available
    if let Some(fix_kind) = possible_fix_kind {
        // Get the possible fix based on the scope
        if let Some(fix) = get_possible_fix(name, fix_kind, binding.scope, semantic) {
            diagnostic.try_set_fix(|| {
                Renamer::rename(name, &fix, scope, semantic, checker.stylist())
                    .map(|(edit, rest)| Fix::safe_edits(edit, rest))
            });
        }
    }

    Some(diagnostic)
}

/// Enumeration of various ways in which a binding can shadow other variables
#[derive(Debug, PartialEq, Eq, Copy, Clone)]
enum ShadowedKind {
    /// The variable shadows a global, nonlocal or local symbol
    Some,
    /// The variable shadows a builtin symbol
    BuiltIn,
    /// The variable shadows a keyword
    Keyword,
    /// The variable does not shadow any other symbols
    None,
}

/// Suggests a potential alternative name to resolve a shadowing conflict.
fn get_possible_fix(
    name: &str,
    kind: ShadowedKind,
    scope_id: ScopeId,
    semantic: &SemanticModel,
) -> Option<String> {
    // Remove leading underscores for processing
    let trimmed_name = name.trim_start_matches('_');

    // Construct the potential fix name based on ShadowedKind
    let fix_name = match kind {
        ShadowedKind::Some | ShadowedKind::BuiltIn | ShadowedKind::Keyword => {
            format!("{trimmed_name}_") // Append an underscore
        }
        ShadowedKind::None => trimmed_name.to_string(),
    };

    // Ensure the fix name is not already taken in the scope or enclosing scopes
    if !semantic.is_available_in_scope(&fix_name, scope_id) {
        return None;
    }

    // Check if the fix name is a valid identifier
    is_identifier(&fix_name).then_some(fix_name)
}

/// Determines the kind of shadowing or conflict for a given variable name.
fn get_possible_fix_kind(name: &str, checker: &Checker, scope_id: ScopeId) -> Option<ShadowedKind> {
    // If the name starts with an underscore, we don't consider it
    if !name.starts_with('_') {
        return None;
    }

    // Trim the leading underscores for further checks
    let trimmed_name = name.trim_start_matches('_');

    // Check the kind in order of precedence
    if is_keyword(trimmed_name) {
        return Some(ShadowedKind::Keyword);
    }

    if is_python_builtin(
        trimmed_name,
        checker.settings.target_version.minor(),
        checker.source_type.is_ipynb(),
    ) {
        return Some(ShadowedKind::BuiltIn);
    }

    if !checker
        .semantic()
        .is_available_in_scope(trimmed_name, scope_id)
    {
        return Some(ShadowedKind::Some);
    }

    // Default to no shadowing
    Some(ShadowedKind::None)
}
