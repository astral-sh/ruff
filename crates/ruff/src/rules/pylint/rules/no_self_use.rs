use ast::Parameter;
use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::{self as ast, ParameterWithDefault};
use ruff_python_semantic::Scope;

use crate::{checkers::ast::Checker, rules::pylint::helpers::is_known_dunder_method};

/// ## What it does
/// Checks for the presence of unused `self` parameter in methods definitions.
///
/// ## Why is this bad?
/// If you are not using the `self` parameter within your method, is likely
/// that this method may not be what you want. You should consider changing
/// to a function outside of the class or using the `@staticmethod` decorator.
///
/// ## Example
/// ```python
/// class Person:
///     def greeting_1(self):
///         print("Greetings friend!")
///
///     def greeting_2(self):
///         print("Hello")
/// ```
///
/// Use instead:
/// ```python
/// class Person:
///     name = "World"
///
///     @staticmethod
///     def greeting_1():
///         print("Greetings friend!")
///
///     def greeting_2(self):
///         print(f"Hello {self.name}")
/// ```
#[violation]
pub struct NoSelfUse {
    method_name: String,
}

impl Violation for NoSelfUse {
    #[derive_message_formats]
    fn message(&self) -> String {
        let NoSelfUse { method_name } = self;
        format!("Method `{method_name}` could be a function or a static method (no-self-use)")
    }
}

/// PLR6301
pub(crate) fn no_self_use(checker: &Checker, scope: &Scope, diagnostics: &mut Vec<Diagnostic>) {
    // Make sure the parent scope is a class.
    if checker
        .semantic()
        .first_non_type_parent_scope(scope)
        .is_some_and(|parent_scope| parent_scope.kind.as_class().is_none())
    {
        return;
    }
    let Some(function) = scope.kind.as_function() else {
        return;
    };

    if is_known_dunder_method(function.name.as_str()) {
        return;
    }

    for arg in function
        .parameters
        .args
        .iter()
        .filter_map(as_self_parameter)
    {
        if scope
            .get(arg.name.as_str())
            .map(|binding_id| checker.semantic().binding(binding_id))
            .is_some_and(|binding| binding.kind.is_argument() && !binding.is_used())
        {
            diagnostics.push(Diagnostic::new(
                NoSelfUse {
                    method_name: function.name.to_string(),
                },
                arg.range,
            ));
        }
    }
}

/// Return a `Parameter` if `ParameterWithDefault` is the `self` parameter, otherwise `None`
fn as_self_parameter(parameter_with_default: &ParameterWithDefault) -> Option<&Parameter> {
    let parameter = parameter_with_default.as_parameter();
    if parameter.name.as_str() == "self" {
        Some(parameter)
    } else {
        None
    }
}
