use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_python_ast::identifier::Identifier;
use ruff_python_semantic::Scope;
use ruff_python_semantic::analyze::function_type::{self, FunctionType};

use crate::Violation;
use crate::checkers::ast::Checker;
use crate::codes::Category;

/// ## What it does
/// Checks for methods that have no parameter to receive their implicit argument.
///
/// ## Why is this bad?
/// Calling a method through an instance passes the instance as an implicit
/// positional argument. Class methods receive the class instead. A method without a
/// positional parameter or `*args` cannot accept this argument and raises a
/// `TypeError` when called this way.
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
/// class, without passing an instance: `Example.method()` works in the example
/// above, while `Example().method()` raises a `TypeError`. This rule flags the
/// definition regardless of how it is called. Use `@staticmethod`, or suppress
/// the diagnostic if class-only access is intentional.
///
/// Custom decorators can change whether a method receives an implicit argument.
/// Decorators that make a method static can be configured using
/// [`lint.pep8-naming.staticmethod-decorators`].
///
/// For compatibility with Pylint's E0211, methods accepting `**kwargs` are
/// exempt, even though `**kwargs` cannot receive a positional argument.
///
/// ## Options
/// - `lint.pep8-naming.classmethod-decorators`
/// - `lint.pep8-naming.staticmethod-decorators`
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

    let parameters = &function.parameters;
    if !parameters.posonlyargs.is_empty()
        || !parameters.args.is_empty()
        || parameters.vararg.is_some()
        || parameters.kwarg.is_some()
    {
        return;
    }

    let Some(parent) = checker.semantic().first_non_type_parent_scope(scope) else {
        return;
    };

    match function_type::classify(
        &function.name,
        &function.decorator_list,
        parent,
        checker.semantic(),
        &checker.settings().pep8_naming.classmethod_decorators,
        &checker.settings().pep8_naming.staticmethod_decorators,
    ) {
        FunctionType::Function => return,
        // Python passes the class to `__new__` even when it is explicitly static.
        FunctionType::StaticMethod if function.name.as_str() != "__new__" => return,
        _ => {}
    }

    checker.report_diagnostic(
        NoMethodArgument {
            method_name: function.name.to_string(),
        },
        function.identifier(),
    );
}
