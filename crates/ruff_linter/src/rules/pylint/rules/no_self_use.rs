use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast as ast;
use ruff_python_ast::identifier::Identifier;
use ruff_python_ast::name::QualifiedName;
use ruff_python_semantic::{
    analyze::{function_type, visibility},
    Scope, ScopeId, ScopeKind,
};

use crate::checkers::ast::Checker;

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
#[violation]
pub struct NoSelfUse {
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
pub(crate) fn no_self_use(
    checker: &Checker,
    scope_id: ScopeId,
    scope: &Scope,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let Some(parent) = checker.semantic().first_non_type_parent_scope(scope) else {
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
        .map(|decorator| QualifiedName::from_dotted_name(decorator))
        .collect::<Vec<QualifiedName>>();

    if function_type::is_stub(func, checker.semantic())
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
        .next()
        .map(|param| &param.parameter)
    else {
        return;
    };

    if parameter.name.as_str() != "self" {
        return;
    }

    // If the method contains a `super` reference, then it should be considered to use self
    // implicitly.
    if let Some(binding_id) = checker.semantic().global_scope().get("super") {
        let binding = checker.semantic().binding(binding_id);
        if binding.kind.is_builtin() {
            if binding
                .references()
                .any(|id| checker.semantic().reference(id).scope_id() == scope_id)
            {
                return;
            }
        }
    }

    if scope
        .get("self")
        .map(|binding_id| checker.semantic().binding(binding_id))
        .is_some_and(|binding| binding.kind.is_argument() && !binding.is_used())
    {
        diagnostics.push(Diagnostic::new(
            NoSelfUse {
                method_name: name.to_string(),
            },
            func.identifier(),
        ));
    }
}
