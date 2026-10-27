use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_python_ast::identifier::Identifier;
use ruff_python_semantic::Scope;

use crate::Violation;
use crate::checkers::ast::Checker;
use crate::codes::Category;

/// ## What it does
/// Checks for methods that have no parameters.
///
/// Methods with decorators are ignored because decorators can change how a method
/// is called.
///
/// ## Why is this bad?
/// Calling a method through an instance passes the instance as an implicit
/// positional argument. Some special methods, such as `__init_subclass__`, receive
/// the class instead. A method without any parameters cannot accept this argument
/// and raises a `TypeError` when called this way.
///
/// Add an instance or class parameter, conventionally named `self` or `cls`.
/// If the function does not need either, use `@staticmethod` instead.
///
/// ## Example
/// ```python
/// class Example:
///     def method():
///         return 1
/// ```
///
/// Use instead:
/// ```python
/// class Example:
///     def method(self):
///         return 1
/// ```
///
/// ## Known problems
/// An ordinary function defined in a class can be called directly through the
/// class, without passing an instance: `Example.method()` works in the first
/// example, while `Example().method()` raises a `TypeError`. This rule flags the
/// definition regardless of how it is called. Use `@staticmethod`, or suppress
/// the diagnostic if class-only access is intentional.
///
/// ## References
/// - [Python documentation: Instance methods](https://docs.python.org/3/reference/datamodel.html#instance-methods)
#[derive(ViolationMetadata)]
#[violation_metadata(preview_since = "NEXT_RUFF_VERSION", category = Category::Suspicious)]
pub(crate) struct NoMethodArgument {
    method_name: String,
}

impl Violation for NoMethodArgument {
    #[derive_message_formats]
    fn message(&self) -> String {
        let Self { method_name } = self;
        format!("Method `{method_name}` has no parameter for its implicit argument")
    }
}

/// PLE0211
pub(crate) fn no_method_argument(checker: &Checker, scope: &Scope) {
    let Some(function) = scope.kind.as_function() else {
        return;
    };

    if !function.parameters.is_empty() || !function.decorator_list.is_empty() {
        return;
    }

    let Some(parent) = checker.semantic().first_non_type_parent_scope(scope) else {
        return;
    };

    if !parent.kind.is_class() {
        return;
    }

    checker.report_diagnostic(
        NoMethodArgument {
            method_name: function.name.to_string(),
        },
        function.identifier(),
    );
}
