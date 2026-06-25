use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_python_semantic::{BindingKind, Scope, ScopeId};
use ruff_text_size::Ranged;

use crate::Violation;
use crate::checkers::ast::Checker;

/// ## What it does
/// Checks for variables defined in `for`, `try`, `with` statements
/// that redefine function parameters.
///
/// ## Why is this bad?
/// Redefined variables can cause unexpected behavior because of overridden function parameters.
/// If nested functions are declared, an inner function's body can override an outer function's parameters.
///
/// ## Example
/// ```python
/// def show(host_id=10.11):
///     for host_id, host in [[12.13, "Venus"], [14.15, "Mars"]]:
///         print(host_id, host)
/// ```
///
/// Use instead:
/// ```python
/// def show(host_id=10.11):
///     for inner_host_id, host in [[12.13, "Venus"], [14.15, "Mars"]]:
///         print(host_id, inner_host_id, host)
/// ```
///
/// ## Options
/// - `lint.dummy-variable-rgx`
///
/// ## References
/// - [Pylint documentation](https://pylint.readthedocs.io/en/latest/user_guide/messages/refactor/redefined-argument-from-local.html)
#[derive(ViolationMetadata)]
#[violation_metadata(stable_since = "0.5.0")]
pub(crate) struct RedefinedArgumentFromLocal {
    pub(crate) name: String,
}

impl Violation for RedefinedArgumentFromLocal {
    #[derive_message_formats]
    fn message(&self) -> String {
        let RedefinedArgumentFromLocal { name } = self;
        format!("Redefining argument with the local name `{name}`")
    }
}

/// PLR1704
pub(crate) fn redefined_argument_from_local(checker: &Checker, scope_id: ScopeId, scope: &Scope) {
    for (name, binding_id) in scope.bindings() {
        for shadow in checker.semantic().shadowed_bindings(scope_id, binding_id) {
            let binding = &checker.semantic().bindings[shadow.binding_id()];
            if !matches!(
                binding.kind,
                BindingKind::LoopVar | BindingKind::BoundException | BindingKind::WithItemVar
            ) {
                continue;
            }
            let shadowed = &checker.semantic().bindings[shadow.shadowed_id()];
            if !shadowed.kind.is_argument() {
                continue;
            }
            if checker.settings().dummy_variable_rgx.is_match(name) {
                continue;
            }
            let scope = &checker.semantic().scopes[binding.scope];
            if scope.kind.is_generator() {
                continue;
            }
            checker.report_diagnostic(
                RedefinedArgumentFromLocal {
                    name: name.to_string(),
                },
                binding.range(),
            );
        }
    }
}
