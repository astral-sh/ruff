use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, ViolationMetadata};
use ruff_python_ast::helpers::is_dunder;
use ruff_python_semantic::{Binding, BindingFlags, BindingKind};
use ruff_python_stdlib::builtins::is_python_builtin;
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;

/// ## What it does
/// Checks for usages of variables marked as unused (variable names starting with an underscore, except '_') in functions.
///
/// ## Why is this bad?
/// Marking variables with a leading underscore conveys that they are intentionally unused within the function or method.
/// When these variables are later referenced in the code, it causes confusion and potential misunderstandings about
/// the code's intention. A variable marked as "unused" but subsequently used suggests oversight or unintentional use
/// and detracts from the clarity and maintainability of the codebase.
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
#[derive(ViolationMetadata)]
pub(crate) struct UnusedVariableAccessed {
    name: String,
    shadowed_kind: ShadowedKind,
}

impl Violation for UnusedVariableAccessed {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!(
            "Local variable `{}` with leading underscore is accessed",
            self.name
        )
    }

    fn fix_title(&self) -> Option<String> {
        Some(match self.shadowed_kind {
            ShadowedKind::BuiltIn => {
                "Prefer using trailing underscores to avoid shadowing a built-in".to_string()
            }
            ShadowedKind::Some => {
                "Prefer using trailing underscores to avoid shadowing a variable".to_string()
            }
            ShadowedKind::None => "Remove leading underscores".to_string(),
        })
    }
}

/// RUF052
pub(crate) fn unused_variable_accessed(
    checker: &Checker,
    binding: &Binding,
) -> Option<Vec<Diagnostic>> {
    let name = binding.name(checker.source());

    // only variables marked as private
    if !name.starts_with('_') || name == "_" || is_dunder(name) {
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
    // Only variables defined in function scopes
    if !checker.semantic().scopes[binding.scope].kind.is_function() {
        return None;
    }
    if !checker.settings.dummy_variable_rgx.is_match(name) {
        return None;
    }

    let trimmed_name = name.trim_start_matches('_');
    let mut kind = ShadowedKind::None;
    // let mut fix = trimmed_name.to_string();

    if !trimmed_name.is_empty() {
        if is_python_builtin(
            trimmed_name,
            checker.settings.target_version.minor(),
            checker.source_type.is_ipynb(),
        ) {
            kind = ShadowedKind::BuiltIn;
        } else if checker.semantic().scopes[binding.scope].has(trimmed_name) {
            kind = ShadowedKind::Some;
        }
    }

    Some(
        binding
            .references
            .iter()
            .map(|ref_id| {
                Diagnostic::new(
                    UnusedVariableAccessed {
                        name: name.to_string(),
                        shadowed_kind: kind,
                    },
                    checker.semantic().reference(*ref_id).range(),
                )
            })
            .collect(),
    )
}

#[derive(Debug, PartialEq, Eq, Copy, Clone)]
enum ShadowedKind {
    Some,
    BuiltIn,
    None,
}
