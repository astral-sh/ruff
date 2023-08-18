use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::{self as ast};
use ruff_python_semantic::{
    analyze::{function_type, visibility},
    Scope,
};

use crate::{checkers::ast::Checker, rules::flake8_unused_arguments::helpers};

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
    let Some(parent) = &checker.semantic().first_non_type_parent_scope(scope) else {
        return;
    };
    let Some(ast::StmtFunctionDef {
        name,
        parameters,
        body,
        decorator_list,
        ..
    }) = scope.kind.as_function() else {
        return;
    };

    if !matches!(
        function_type::classify(
            name,
            decorator_list,
            parent,
            checker.semantic(),
            &checker.settings.pep8_naming.classmethod_decorators,
            &checker.settings.pep8_naming.staticmethod_decorators,
        ),
        function_type::FunctionType::Method
    ) {
        return;
    }

    if helpers::is_empty(body)
        || visibility::is_magic(name)
        || visibility::is_abstract(decorator_list, checker.semantic())
        || visibility::is_override(decorator_list, checker.semantic())
        || visibility::is_overload(decorator_list, checker.semantic())
        || visibility::is_property(decorator_list, &[], checker.semantic())
    {
        return;
    }

    let Some(arg) = parameters.args.get(0).map(ast::ParameterWithDefault::as_parameter) else {
        return;
    };

    if scope
        .get(arg.name.as_str())
        .map(|binding_id| checker.semantic().binding(binding_id))
        .is_some_and(|binding| binding.kind.is_argument() && !binding.is_used())
    {
        diagnostics.push(Diagnostic::new(
            NoSelfUse {
                method_name: name.to_string(),
            },
            arg.range,
        ));
    }
}
