use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_python_semantic::{ResolvedReference, Scope, ScopeId};
use ruff_text_size::Ranged;

use crate::Violation;
use crate::checkers::ast::Checker;
use crate::codes::Category;

/// ## What it does
/// Checks for `global` variables that are not assigned a value in the current
/// scope.
///
/// ## Why is this bad?
/// The `global` keyword allows an inner scope to modify a variable declared
/// in the outer scope. If the variable is not modified within the inner scope,
/// there is no need to use `global`.
///
/// ## Example
/// ```python
/// DEBUG = True
///
///
/// def foo():
///     global DEBUG
///     if DEBUG:
///         print("foo() called")
///     ...
/// ```
///
/// Use instead:
/// ```python
/// DEBUG = True
///
///
/// def foo():
///     if DEBUG:
///         print("foo() called")
///     ...
/// ```
///
/// ## References
/// - [Python documentation: The `global` statement](https://docs.python.org/3/reference/simple_stmts.html#the-global-statement)
#[derive(ViolationMetadata)]
#[violation_metadata(stable_since = "v0.0.174", category = Category::Style)]
pub(crate) struct GlobalVariableNotAssigned {
    name: String,
}

impl Violation for GlobalVariableNotAssigned {
    #[derive_message_formats]
    fn message(&self) -> String {
        let GlobalVariableNotAssigned { name } = self;
        format!("Using global for `{name}` but no assignment is done")
    }
}

/// PLW0602
pub(crate) fn global_variable_not_assigned(checker: &Checker, scope_id: ScopeId, scope: &Scope) {
    for (name, binding_id) in scope.bindings() {
        let binding = checker.semantic().binding(binding_id);
        // If the binding is a `global`, then it's a top-level `global` that was never
        // assigned in the current scope. If it were assigned, the `global` would be
        // shadowed by the assignment.
        if binding.kind.is_global() {
            // If the binding was conditionally deleted, it will include a reference within
            // a `Del` context, but won't be shadowed by a `BindingKind::Deletion`, as in:
            // ```python
            // if condition:
            //     del var
            // ```
            if binding
                .references
                .iter()
                .map(|id| checker.semantic().reference(*id))
                .all(ResolvedReference::is_load)
            {
                // If the name is read, and an enclosing function binds it too, the `global`
                // is what makes the read refer to the global variable rather than the
                // enclosing function's local, as in:
                // ```python
                // def outer():
                //     DEBUG = False
                //
                //     def inner():
                //         global DEBUG
                //         print(DEBUG)
                // ```
                if binding.is_used() && is_shadowed_by_enclosing_scope(checker, scope_id, name) {
                    continue;
                }

                checker.report_diagnostic(
                    GlobalVariableNotAssigned {
                        name: (*name).to_string(),
                    },
                    binding.range(),
                );
            }
        }
    }
}

/// Returns `true` if, without the `global` declaration, a read of `name` in the given scope would
/// resolve to a binding in an enclosing scope rather than in the global scope.
fn is_shadowed_by_enclosing_scope(checker: &Checker, scope_id: ScopeId, name: &str) -> bool {
    let semantic = checker.semantic();
    semantic
        .scopes
        .ancestors(scope_id)
        .skip(1)
        // Names bound in a class scope aren't visible from the scopes nested within it.
        .filter(|scope| !scope.kind.is_class())
        .take_while(|scope| !scope.kind.is_module())
        .find(|scope| scope.has(name))
        // The nearest enclosing scope that mentions the name decides: if it declares the name
        // `global`, the name already refers to the global scope, so the inner `global` is
        // redundant. A `global` applies to the whole scope, so it's still at the start of the
        // binding chain after later assignments or deletions.
        .is_some_and(|scope| {
            !scope
                .get_all(name)
                .any(|binding_id| semantic.binding(binding_id).kind.is_global())
        })
}
