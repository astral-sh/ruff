use ruff_python_ast::{Decorator, ParameterWithDefault, Parameters};

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_semantic::analyze::function_type;
use ruff_python_semantic::Scope;
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;

/// ## What it does
/// Checks for class methods that use a name other than `cls` for their
/// first argument.
///
/// ## Why is this bad?
/// [PEP 8] recommends the use of `cls` as the first argument for all class
/// methods:
///
/// > Always use `cls` for the first argument to class methods.
/// >
/// > If a function argument’s name clashes with a reserved keyword, it is generally better to
/// > append a single trailing underscore rather than use an abbreviation or spelling corruption.
/// > Thus `class_` is better than `clss`. (Perhaps better is to avoid such clashes by using a synonym.)
///
/// ## Example
/// ```python
/// class Example:
///     @classmethod
///     def function(self, data):
///         ...
/// ```
///
/// Use instead:
/// ```python
/// class Example:
///     @classmethod
///     def function(cls, data):
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
pub struct InvalidFirstArgumentNameForClassMethod;

impl Violation for InvalidFirstArgumentNameForClassMethod {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("First argument of a class method should be named `cls`")
    }
}

/// N804
pub(crate) fn invalid_first_argument_name_for_class_method(
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
        function_type::FunctionType::ClassMethod
    ) {
        return None;
    }
    if let Some(ParameterWithDefault {
        parameter,
        default: _,
        range: _,
    }) = parameters
        .posonlyargs
        .first()
        .or_else(|| parameters.args.first())
    {
        if &parameter.name != "cls" {
            if checker
                .settings
                .pep8_naming
                .ignore_names
                .iter()
                .any(|ignore_name| ignore_name.matches(name))
            {
                return None;
            }
            return Some(Diagnostic::new(
                InvalidFirstArgumentNameForClassMethod,
                parameter.range(),
            ));
        }
    }
    None
}
