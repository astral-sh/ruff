use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::call_path::{from_qualified_name, CallPath};
use ruff_python_ast::{self as ast, ParameterWithDefault};
use ruff_python_semantic::{
    analyze::{function_type, visibility},
    Scope, ScopeKind,
};
use ruff_text_size::Ranged;

use crate::{checkers::ast::Checker, rules::flake8_unused_arguments::helpers};

/// ## What it does
/// Checks for the presence of unused `self` parameter in methods definitions.
///
/// ## Why is this bad?
/// Unused `self` parameters are usually a sign of a method that could be
/// replaced by a function or a static method.
///
/// ## Example
/// ```python
/// class Person:
///     def greeting(self):
///         print("Greetings friend!")
/// ```
///
/// Use instead:
/// ```python
/// class Person:
///     @staticmethod
///     def greeting():
///         print(f"Greetings friend!")
/// ```
#[violation]
pub struct NoSelfUse {
    method_name: String,
}

impl Violation for NoSelfUse {
    #[derive_message_formats]
    fn message(&self) -> String {
        let NoSelfUse { method_name } = self;
        format!("Method `{method_name}` could be a function or static method")
    }
}

/// PLR6301
pub(crate) fn no_self_use(checker: &Checker, scope: &Scope, diagnostics: &mut Vec<Diagnostic>) {
    let Some(parent) = &checker.semantic().first_non_type_parent_scope(scope) else {
        return;
    };

    let ScopeKind::Function(ast::StmtFunctionDef {
        name,
        parameters,
        body,
        decorator_list,
        ..
    }) = scope.kind
    else {
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

    let property_decorators = checker
        .settings
        .pydocstyle
        .property_decorators
        .iter()
        .map(|decorator| from_qualified_name(decorator))
        .collect::<Vec<CallPath>>();

    if helpers::is_empty(body)
        || visibility::is_magic(name)
        || visibility::is_abstract(decorator_list, checker.semantic())
        || visibility::is_override(decorator_list, checker.semantic())
        || visibility::is_overload(decorator_list, checker.semantic())
        || visibility::is_property(decorator_list, &property_decorators, checker.semantic())
    {
        return;
    }

    // Identify the `self` parameter.
    let Some(parameter) = parameters
        .posonlyargs
        .iter()
        .chain(&parameters.args)
        .chain(&parameters.kwonlyargs)
        .next()
        .map(ParameterWithDefault::as_parameter)
    else {
        return;
    };

    if parameter.name.as_str() == "self"
        && scope
            .get("self")
            .map(|binding_id| checker.semantic().binding(binding_id))
            .is_some_and(|binding| binding.kind.is_argument() && !binding.is_used())
    {
        diagnostics.push(Diagnostic::new(
            NoSelfUse {
                method_name: name.to_string(),
            },
            parameter.range(),
        ));
    }
}
