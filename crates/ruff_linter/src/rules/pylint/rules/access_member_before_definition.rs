use ruff_diagnostics::{Diagnostic, FixAvailability, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_semantic::{ScopeId, ScopeKind};

use ruff_source_file::SourceRow;
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;

/// ## What it does
/// Checks for accesses on instance attributes before they are initialized in __init__ method.
/// If the attribute is not initialized at all then it is not reported.
///
/// ## Why is this bad?
/// Accessing a class member before it is initialized will raise an `AttributeError`.
///
/// ## Example
/// ```python
/// class Unicorn:
///     def __init__(self, x):
///         if self.x > 9000:  # [access-member-before-definition]
///             pass
///         self.x = x
/// ```
///
/// Use instead:
/// ```python
/// class Unicorn:
///     def __init__(self, x):
///         self.x = x
///         if self.x > 9000:
///             pass
/// ```
///
#[violation]
pub struct AccessMemberBeforeDefinition {
    member: String,
    initialized_at: SourceRow,
}

impl Violation for AccessMemberBeforeDefinition {
    const FIX_AVAILABILITY: FixAvailability = FixAvailability::None;

    #[derive_message_formats]
    fn message(&self) -> String {
        let AccessMemberBeforeDefinition {
            member,
            initialized_at,
        } = self;
        format!("Accessed `{member}` before it was initialized at {initialized_at}",)
    }
}

/// PLE0203
pub(crate) fn access_member_before_definition(
    checker: &Checker,
    scope_id: ScopeId,
    diagnostics: &mut Vec<Diagnostic>,
) {
    if let Some(scope) = checker.semantic().scopes.get(scope_id) {
        // Only run on __init__ method
        match scope.kind {
            ScopeKind::Function(function) => {
                if function.name.as_str() != "__init__" {
                    return;
                }
            }
            _ => return,
        };
    };

    let Some(enclosing_class_scope_id) =
        checker.semantic().first_non_type_parent_scope_id(scope_id)
    else {
        return;
    };

    let Some(enclosing_class_scope) = checker.semantic().scopes.get(enclosing_class_scope_id)
    else {
        return;
    };

    for unresolved in checker.semantic().unresolved_attributes() {
        if unresolved.scope_id != scope_id {
            continue;
        }
        let attr_name = checker.locator().slice(unresolved.range);
        if let Some(binding_id) = enclosing_class_scope.get(attr_name) {
            let binding = checker.semantic().binding(binding_id);
            if binding.start() > unresolved.range.end() {
                let diagnostic = Diagnostic::new(
                    AccessMemberBeforeDefinition {
                        initialized_at: checker.compute_source_row(binding.start()),
                        member: attr_name.to_string(),
                    },
                    unresolved.range,
                );
                diagnostics.push(diagnostic);
            }
        };
    }
}
