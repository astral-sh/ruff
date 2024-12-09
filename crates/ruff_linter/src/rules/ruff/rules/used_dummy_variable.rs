use ruff_diagnostics::{Diagnostic, Fix, FixAvailability, Violation};
use ruff_macros::{derive_message_formats, ViolationMetadata};
use ruff_python_ast::helpers::is_dunder;
use ruff_python_semantic::{Binding, ScopeId};
use ruff_python_stdlib::{
    builtins::is_python_builtin, identifiers::is_identifier, keyword::is_keyword,
};
use ruff_text_size::Ranged;

use crate::{checkers::ast::Checker, renamer::Renamer};

/// ## What it does
/// Checks for "dummy variables" (variables that are named as if to indicate they are unused)
/// that are in fact used.
///
/// By default, "dummy variables" are any variables with names that start with leading
/// underscores. However, this is customisable using the [`lint.dummy-variable-rgx`] setting).
///
/// ## Why is this bad?
/// Marking a variable with a leading underscore conveys that it is intentionally unused within the function or method.
/// When these variables are later referenced in the code, it causes confusion and potential misunderstandings about
/// the code's intention. If a variable marked as "unused" is subsequently used, it suggests that either the variable
/// could be given a clearer name, or that the code is accidentally making use of the wrong variable.
///
/// Sometimes leading underscores are used to avoid variables shadowing other variables, Python builtins, or Python
/// keywords. However, [PEP 8] recommends to use trailing underscores for this rather than leading underscores.
///
/// Dunder variables are ignored by this rule, as are variables named `_`.
/// Only local variables in function scopes are flagged by the rule.
///
/// ## Example
/// ```python
/// def function():
///     _variable = 3
///     # important: avoid shadowing the builtin `id()` function!
///     _id = 4
///     return _variable + _id
/// ```
///
/// Use instead:
/// ```python
/// def function():
///     variable = 3
///     # important: avoid shadowing the builtin `id()` function!
///     id_ = 4
///     return variable + id_
/// ```
///
/// ## Fix availability
/// The rule's fix is only available for variables that start with leading underscores.
/// It will also only be available if the "obvious" new name for the variable
/// would not shadow any other known variables already accessible from the scope
/// in which the variable is defined.
///
/// ## Fix safety
/// This rule's fix is marked as unsafe.
///
/// For this rule's fix, Ruff renames the variable and fixes up all known references to
/// it so they point to the renamed variable. However, some renamings also require other
/// changes such as different arguments to constructor calls or alterations to comments.
/// Ruff is aware of some of these cases: `_T = TypeVar("_T")` will be fixed to
/// `T = TypeVar("T")` if the `_T` binding is flagged by this rule. However, in general,
/// cases like these are hard to detect and hard to automatically fix.
///
/// ## Options
/// - [`lint.dummy-variable-rgx`]
///
/// [PEP 8]: https://peps.python.org/pep-0008/
#[derive(ViolationMetadata)]
pub(crate) struct UsedDummyVariable {
    name: String,
    shadowed_kind: Option<ShadowedKind>,
}

impl Violation for UsedDummyVariable {
    const FIX_AVAILABILITY: FixAvailability = FixAvailability::Sometimes;

    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Local dummy variable `{}` is accessed", self.name)
    }

    fn fix_title(&self) -> Option<String> {
        self.shadowed_kind.map(|kind| match kind {
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
        })
    }
}

/// RUF052
pub(crate) fn used_dummy_variable(checker: &Checker, binding: &Binding) -> Option<Diagnostic> {
    let name = binding.name(checker.source());

    // Ignore `_` and dunder variables
    if name == "_" || is_dunder(name) {
        return None;
    }
    // only used variables
    if binding.is_unused() {
        return None;
    }

    // We only emit the lint on variables defined via assignments.
    //
    // ## Why not also emit the lint on function parameters?
    //
    // There isn't universal agreement that leading underscores indicate "unused" parameters
    // in Python (many people use them for "private" parameters), so this would be a lot more
    // controversial than emitting the lint on assignments. Even if it's decided that it's
    // desirable to emit a lint on function parameters with "dummy variable" names, it would
    // possibly have to be a separate rule or we'd have to put it behind a configuration flag,
    // as there's much less community consensus about the issue.
    // See <https://github.com/astral-sh/ruff/issues/14796>.
    //
    // Moreover, autofixing the diagnostic for function parameters is much more troublesome than
    // autofixing the diagnostic for assignments. See:
    // - <https://github.com/astral-sh/ruff/issues/14790>
    // - <https://github.com/astral-sh/ruff/issues/14799>
    if !binding.kind.is_assignment() {
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

    let shadowed_kind = try_shadowed_kind(name, checker, binding.scope);

    let mut diagnostic = Diagnostic::new(
        UsedDummyVariable {
            name: name.to_string(),
            shadowed_kind,
        },
        binding.range(),
    );

    // If fix available
    if let Some(shadowed_kind) = shadowed_kind {
        // Get the possible fix based on the scope
        if let Some(fix) = get_possible_fix(name, shadowed_kind, binding.scope, checker) {
            diagnostic.try_set_fix(|| {
                Renamer::rename(name, &fix, scope, semantic, checker.stylist())
                    .map(|(edit, rest)| Fix::unsafe_edits(edit, rest))
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
    checker: &Checker,
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

    // Check if the fix name is again dummy identifier
    if checker.settings.dummy_variable_rgx.is_match(&fix_name) {
        return None;
    }

    // Ensure the fix name is not already taken in the scope or enclosing scopes
    if !checker
        .semantic()
        .is_available_in_scope(&fix_name, scope_id)
    {
        return None;
    }

    // Check if the fix name is a valid identifier
    is_identifier(&fix_name).then_some(fix_name)
}

/// Determines the kind of shadowing or conflict for a given variable name.
fn try_shadowed_kind(name: &str, checker: &Checker, scope_id: ScopeId) -> Option<ShadowedKind> {
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
