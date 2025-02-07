use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, ViolationMetadata};
use ruff_python_ast as ast;
use ruff_python_ast::identifier::Identifier;
use ruff_python_semantic::{
    analyze::{function_type, visibility},
    Scope, ScopeId, ScopeKind,
};

use crate::checkers::ast::Checker;
use crate::rules::flake8_unused_arguments::rules::is_not_implemented_stub_with_variable;

/// ## What it does
/// Checks for the presence of unused `self` parameter in methods definitions.
///
/// ## Why is this bad?
/// Unused `self` parameters are usually a sign of a method that could be
/// replaced by a function, class method, or static method.
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
/// def greeting():
///     print("Greetings friend!")
/// ```
///
/// or
///
/// ```python
/// class Person:
///     @staticmethod
///     def greeting():
///         print("Greetings friend!")
/// ```
#[derive(ViolationMetadata)]
pub(crate) struct NoSelfUse {
    method_name: String,
}

impl Violation for NoSelfUse {
    #[derive_message_formats]
    fn message(&self) -> String {
        let NoSelfUse { method_name } = self;
        format!("Method `{method_name}` could be a function, class method, or static method")
    }
}

/// PLR6301
pub(crate) fn no_self_use(checker: &Checker, scope_id: ScopeId, scope: &Scope) {
    let semantic = checker.semantic();

    let Some(parent) = semantic.first_non_type_parent_scope(scope) else {
        return;
    };

    let ScopeKind::Function(func) = scope.kind else {
        return;
    };

    let ast::StmtFunctionDef {
        name,
        parameters,
        decorator_list,
        ..
    } = func;

    if !matches!(
        function_type::classify(
            name,
            decorator_list,
            parent,
            semantic,
            &checker.settings.pep8_naming.classmethod_decorators,
            &checker.settings.pep8_naming.staticmethod_decorators,
        ),
        function_type::FunctionType::Method
    ) {
        return;
    }

    let extra_property_decorators = checker.settings.pydocstyle.property_decorators();

    if function_type::is_stub(func, semantic)
        || visibility::is_magic(name)
        || visibility::is_abstract(decorator_list, semantic)
        || visibility::is_override(decorator_list, semantic)
        || visibility::is_overload(decorator_list, semantic)
        || visibility::is_property(decorator_list, extra_property_decorators, semantic)
        || visibility::is_validator(decorator_list, semantic)
        || is_not_implemented_stub_with_variable(func, semantic)
    {
        return;
    }

    // Identify the `self` parameter.
    let Some(parameter) = parameters.posonlyargs.iter().chain(&parameters.args).next() else {
        return;
    };

    if parameter.name() != "self" {
        return;
    }

    // If the method contains a `super` reference, then it should be considered to use self
    // implicitly.
    if let Some(binding_id) = semantic.global_scope().get("super") {
        let binding = semantic.binding(binding_id);
        if binding.kind.is_builtin() {
            if binding
                .references()
                .any(|id| semantic.reference(id).scope_id() == scope_id)
            {
                return;
            }
        }
    }

    if scope
        .get("self")
        .map(|binding_id| semantic.binding(binding_id))
        .is_some_and(|binding| binding.kind.is_argument() && binding.is_unused())
    {
        checker.report_diagnostic(Diagnostic::new(
            NoSelfUse {
                method_name: name.to_string(),
            },
            func.identifier(),
        ));
    }
}
