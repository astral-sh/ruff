use ruff_python_ast::{Decorator, Parameters};

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_semantic::analyze::function_type;
use ruff_python_semantic::Scope;
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;

/// ## What it does
/// Checks for instance methods that use a name other than `self` for their
/// first argument.
///
/// ## Why is this bad?
/// [PEP 8] recommends the use of `self` as first argument for all instance
/// methods:
///
/// > Always use self for the first argument to instance methods.
/// >
/// > If a function argument’s name clashes with a reserved keyword, it is generally better to
/// > append a single trailing underscore rather than use an abbreviation or spelling corruption.
/// > Thus `class_` is better than `clss`. (Perhaps better is to avoid such clashes by using a synonym.)
///
/// ## Example
/// ```python
/// class Example:
///     def function(cls, data):
///         ...
/// ```
///
/// Use instead:
/// ```python
/// class Example:
///     def function(self, data):
///         ...
/// ```
///
/// ## Options
/// - `pep8-naming.classmethod-decorators`
/// - `pep8-naming.staticmethod-decorators`
/// - `pep8-naming.ignore-names`
///
/// [PEP 8]: https://peps.python.org/pep-0008/#function-and-method-arguments
#[violation]
pub struct InvalidFirstArgumentNameForMethod;

impl Violation for InvalidFirstArgumentNameForMethod {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("First argument of a method should be named `self`")
    }
}

/// N805
pub(crate) fn invalid_first_argument_name_for_method(
    checker: &Checker,
    scope: &Scope,
    name: &str,
    decorator_list: &[Decorator],
    parameters: &Parameters,
) -> Option<Diagnostic> {
    if !matches!(
        function_type::classify(
            name,
            decorator_list,
            scope,
            checker.semantic(),
            &checker.settings.pep8_naming.classmethod_decorators,
            &checker.settings.pep8_naming.staticmethod_decorators,
        ),
        function_type::FunctionType::Method
    ) {
        return None;
    }
    let arg = parameters
        .posonlyargs
        .first()
        .or_else(|| parameters.args.first())?;
    if &arg.parameter.name == "self" {
        return None;
    }
    if checker
        .settings
        .pep8_naming
        .ignore_names
        .iter()
        .any(|ignore_name| ignore_name.matches(name))
    {
        return None;
    }
    Some(Diagnostic::new(
        InvalidFirstArgumentNameForMethod,
        arg.parameter.range(),
    ))
}
