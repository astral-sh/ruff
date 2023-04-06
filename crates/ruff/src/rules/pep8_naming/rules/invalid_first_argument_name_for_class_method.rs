use rustpython_parser::ast::{Arguments, Expr};

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::types::Range;
use ruff_python_semantic::analyze::function_type;
use ruff_python_semantic::scope::Scope;

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
/// ## Options
/// - `pep8-naming.classmethod-decorators`
/// - `pep8-naming.staticmethod-decorators`
/// - `pep8-naming.ignore-names`
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
/// [PEP 8]: https://peps.python.org/pep-0008/#function-and-method-arguments
///
#[violation]
pub struct InvalidFirstArgumentNameForClassMethod;

impl Violation for InvalidFirstArgumentNameForClassMethod {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("First argument of a class method should be named `cls`")
    }
}

/// N804
pub fn invalid_first_argument_name_for_class_method(
    checker: &Checker,
    scope: &Scope,
    name: &str,
    decorator_list: &[Expr],
    args: &Arguments,
) -> Option<Diagnostic> {
    if !matches!(
        function_type::classify(
            &checker.ctx,
            scope,
            name,
            decorator_list,
            &checker.settings.pep8_naming.classmethod_decorators,
            &checker.settings.pep8_naming.staticmethod_decorators,
        ),
        function_type::FunctionType::ClassMethod
    ) {
        return None;
    }
    if let Some(arg) = args.posonlyargs.first().or_else(|| args.args.first()) {
        if arg.node.arg != "cls" {
            if checker
                .settings
                .pep8_naming
                .ignore_names
                .iter()
                .any(|ignore_name| ignore_name == name)
            {
                return None;
            }
            return Some(Diagnostic::new(
                InvalidFirstArgumentNameForClassMethod,
                Range::from(arg),
            ));
        }
    }
    None
}
