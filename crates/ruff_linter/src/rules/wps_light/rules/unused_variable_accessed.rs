use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_semantic::Binding;
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;

/// ## What it does
/// Checks for use of unused marked variables (leading with underscore, except '_')
/// Forbid using method and function variables that are marked as unused.
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
#[violation]
pub struct UnusedVariableAccessed {
    name: String,
}

impl Violation for UnusedVariableAccessed {
    #[derive_message_formats]
    fn message(&self) -> String {
        let name = &self.name;
        format!("Local variable `{name}` is marked as unused but is used")
    }

    fn fix_title(&self) -> Option<String> {
        let name = &self.name;
        Some(format!("Remove leading underscores from variable `{name}`"))
    }
}

/// WPS121
pub(crate) fn unused_variable_accessed(checker: &Checker, binding: &Binding) -> Option<Diagnostic> {
    // only used variables
    if binding.is_unused() || !binding.kind.is_assignment() {
        return None;
    }

    // leading with underscore and not '_'
    let name = binding.name(checker.locator().contents());
    if !binding.is_private_declaration() || name == "_" {
        return None;
    }

    // current scope is method or function
    if !checker.semantic().scopes[binding.scope].kind.is_function() {
        return None;
    }

    Some(Diagnostic::new(
        UnusedVariableAccessed {
            name: name.to_string(),
        },
        binding.range(),
    ))
}
