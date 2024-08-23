use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast as ast;
use ruff_python_ast::ParameterWithDefault;
use ruff_python_semantic::analyze::function_type;
use ruff_python_semantic::Scope;
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;

/// ## What it does
/// Checks for static methods that use `self` or `cls` as their first argument.
///
/// ## Why is this bad?
/// [PEP 8] recommends the use of `self` and `cls` as the first arguments for
/// instance methods and class methods, respectively. Naming the first argument
/// of a static method as `self` or `cls` can be misleading, as static methods
/// do not receive an instance or class reference as their first argument.
///
/// ## Example
/// ```python
/// class Wolf:
///     @staticmethod
///     def eat(self):
///         pass
/// ```
///
/// Use instead:
/// ```python
/// class Wolf:
///     @staticmethod
///     def eat(sheep):
///         pass
/// ```
///
/// [PEP 8]: https://peps.python.org/pep-0008/#function-and-method-arguments
#[violation]
pub struct BadStaticmethodArgument {
    argument_name: String,
}

impl Violation for BadStaticmethodArgument {
    #[derive_message_formats]
    fn message(&self) -> String {
        let Self { argument_name } = self;
        format!("First argument of a static method should not be named `{argument_name}`")
    }
}

/// PLW0211
pub(crate) fn bad_staticmethod_argument(
    checker: &Checker,
    scope: &Scope,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let Some(func) = scope.kind.as_function() else {
        return;
    };

    let ast::StmtFunctionDef {
        name,
        decorator_list,
        parameters,
        ..
    } = func;

    let Some(parent) = checker.semantic().first_non_type_parent_scope(scope) else {
        return;
    };

    let type_ = function_type::classify(
        name,
        decorator_list,
        parent,
        checker.semantic(),
        &checker.settings.pep8_naming.classmethod_decorators,
        &checker.settings.pep8_naming.staticmethod_decorators,
    );
    if !matches!(type_, function_type::FunctionType::StaticMethod) {
        return;
    }

    let Some(ParameterWithDefault {
        parameter: self_or_cls,
        ..
    }) = parameters
        .posonlyargs
        .first()
        .or_else(|| parameters.args.first())
    else {
        return;
    };

    match (name.as_str(), self_or_cls.name.as_str()) {
        ("__new__", "cls") => {
            return;
        }
        (_, "self" | "cls") => {}
        _ => return,
    }

    diagnostics.push(Diagnostic::new(
        BadStaticmethodArgument {
            argument_name: self_or_cls.name.to_string(),
        },
        self_or_cls.range(),
    ));
}
