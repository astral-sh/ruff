use std::string::ToString;

use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_python_semantic::{Scope, ScopeId};
use ruff_text_size::Ranged;

use crate::Violation;
use crate::checkers::ast::Checker;
use crate::codes::Category;

/// ## What it does
/// Checks for undefined local variables.
///
/// ## Why is this bad?
/// Referencing a local variable before it has been assigned will raise
/// an `UnboundLocalError` at runtime.
///
/// ## Example
/// ```python
/// x = 1
///
///
/// def foo():
///     x += 1
/// ```
///
/// Use instead:
/// ```python
/// x = 1
///
///
/// def foo():
///     global x
///     x += 1
/// ```
#[derive(ViolationMetadata)]
#[violation_metadata(stable_since = "v0.0.24", category = Category::Correctness)]
pub(crate) struct UndefinedLocal {
    name: String,
}

impl Violation for UndefinedLocal {
    #[derive_message_formats]
    fn message(&self) -> String {
        let UndefinedLocal { name } = self;
        format!("Local variable `{name}` referenced before assignment")
    }
}

/// F823
pub(crate) fn undefined_local(checker: &Checker, scope_id: ScopeId, scope: &Scope) {
    if scope.kind.is_function() {
        for (name, binding_id) in scope.bindings() {
            // Only the first binding of a name in the function is linked to the binding that it
            // shadows in a parent scope; later bindings shadow the earlier ones in the function.
            // For example, in `def f(): print(x); x = 1; x = 2`, `x = 2` shadows `x = 1`, and only
            // `x = 1` shadows the global `x`.
            let Some(first_id) = scope.shadowed_bindings(binding_id).last() else {
                continue;
            };

            // If the variable shadows a binding in a parent scope...
            let Some(shadowed_id) = checker.semantic().shadowed_binding(first_id) else {
                continue;
            };

            // A load in the function can resolve to the parent binding before the name is first
            // bound in the function, or after the name is unbound again, as at the end of an
            // `except ... as x:` handler. Like pyflakes, only report the former. Loads within the
            // binding statement itself, like the one in `x += 1`, count as coming before it.
            let first = checker.semantic().binding(first_id);
            let first_end = first
                .statement(checker.semantic())
                .map_or(first.end(), Ranged::end);

            // And that binding was referenced in the current scope...
            let shadowed = checker.semantic().binding(shadowed_id);
            if let Some(range) = shadowed.references().find_map(|reference_id| {
                let reference = checker.semantic().reference(reference_id);
                (reference.scope_id() == scope_id && reference.start() < first_end)
                    .then(|| reference.range())
            }) {
                // Then it's probably an error.
                checker.report_diagnostic(
                    UndefinedLocal {
                        name: name.to_string(),
                    },
                    range,
                );
            }
        }
    }
}
