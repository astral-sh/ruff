use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_python_ast as ast;
use ruff_python_semantic::{Scope, ScopeId};
use ruff_text_size::Ranged;

use crate::Violation;
use crate::checkers::ast::Checker;

/// ## What it does
/// Checks for private class members (methods and class variables) that are
/// defined but never used.
///
/// ## Why is this bad?
/// Unused private members add unnecessary complexity to the codebase and may
/// indicate dead code or a mistake in the implementation. They should either
/// be used, removed, or made public if intended for external use.
///
/// A member is considered private if its name starts with double underscores
/// (`__`) but does not end with double underscores (which would make it a
/// "dunder" or magic method).
///
/// ## Example
/// ```python
/// class MyClass:
///     __unused_var = 42
///
///     def __unused_method(self):
///         pass
///
///     def public_method(self):
///         pass
/// ```
///
/// Use instead:
/// ```python
/// class MyClass:
///     def public_method(self):
///         pass
/// ```
///
/// Or, if the member is intentionally unused, consider removing the double
/// underscore prefix or adding a comment explaining why it exists.
///
/// ## Known limitations
/// This rule does not detect unused private instance attributes (e.g.,
/// `self.__attr = value`) due to the complexity of tracking attribute
/// assignments across methods.
#[derive(ViolationMetadata)]
#[violation_metadata(preview_since = "0.14.13")]
pub(crate) struct UnusedPrivateMember {
    class_name: String,
    member_name: String,
}

impl Violation for UnusedPrivateMember {
    #[derive_message_formats]
    fn message(&self) -> String {
        let UnusedPrivateMember {
            class_name,
            member_name,
        } = self;
        format!("Unused private member `{class_name}.{member_name}`")
    }
}

/// Returns `true` if the given name is a private member (starts with `__` but
/// does not end with `__`).
#[inline]
fn is_private_member(name: &str) -> bool {
    name.len() > 2 && name.starts_with("__") && !name.ends_with("__")
}

/// PLW0238
pub(crate) fn unused_private_member(
    checker: &Checker,
    class_def: &ast::StmtClassDef,
    _scope_id: ScopeId,
    scope: &Scope,
) {
    let class_name = &class_def.name;

    for (name, binding_id) in scope.bindings() {
        if !is_private_member(name) {
            continue;
        }

        let binding = checker.semantic().binding(binding_id);

        if !binding.kind.is_function_definition() && !binding.kind.is_assignment() {
            continue;
        }

        if !binding.is_unused() {
            continue;
        }

        let mut diagnostic = checker.report_diagnostic(
            UnusedPrivateMember {
                class_name: class_name.to_string(),
                member_name: name.to_string(),
            },
            binding.range(),
        );
        diagnostic.help("Consider removing this unused private member or making it public");
    }
}

#[cfg(test)]
mod tests {
    use super::is_private_member;

    #[test]
    fn test_is_private_member() {
        // Private members (starts with __ but doesn't end with __)
        assert!(is_private_member("__foo"));
        assert!(is_private_member("__bar"));
        assert!(is_private_member("__private_method"));
        assert!(is_private_member("__foo_"));

        // Dunder methods (not private)
        assert!(!is_private_member("__init__"));
        assert!(!is_private_member("__str__"));
        assert!(!is_private_member("__eq__"));
        assert!(!is_private_member("____"));
        assert!(!is_private_member("___"));

        // Single underscore or no underscore (not private)
        assert!(!is_private_member("_foo"));
        assert!(!is_private_member("foo"));
        assert!(!is_private_member("public_method"));

        // Edge cases
        assert!(!is_private_member("__"));
        assert!(!is_private_member(""));
        assert!(!is_private_member("_"));
    }
}
