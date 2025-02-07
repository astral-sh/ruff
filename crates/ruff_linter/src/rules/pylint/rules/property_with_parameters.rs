use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, ViolationMetadata};
use ruff_python_ast::{identifier::Identifier, Decorator, Parameters, Stmt};
use ruff_python_semantic::analyze::visibility::is_property;

use crate::checkers::ast::Checker;

/// ## What it does
/// Checks for property definitions that accept function parameters.
///
/// ## Why is this bad?
/// Properties cannot be called with parameters.
///
/// If you need to pass parameters to a property, create a method with the
/// desired parameters and call that method instead.
///
/// ## Example
///
/// ```python
/// class Cat:
///     @property
///     def purr(self, volume): ...
/// ```
///
/// Use instead:
///
/// ```python
/// class Cat:
///     @property
///     def purr(self): ...
///
///     def purr_volume(self, volume): ...
/// ```
///
/// ## References
/// - [Python documentation: `property`](https://docs.python.org/3/library/functions.html#property)
#[derive(ViolationMetadata)]
pub(crate) struct PropertyWithParameters;

impl Violation for PropertyWithParameters {
    #[derive_message_formats]
    fn message(&self) -> String {
        "Cannot have defined parameters for properties".to_string()
    }
}

/// PLR0206
pub(crate) fn property_with_parameters(
    checker: &Checker,
    stmt: &Stmt,
    decorator_list: &[Decorator],
    parameters: &Parameters,
) {
    if parameters.len() <= 1 {
        return;
    }
    let semantic = checker.semantic();
    let extra_property_decorators = checker.settings.pydocstyle.property_decorators();
    if is_property(decorator_list, extra_property_decorators, semantic) {
        checker.report_diagnostic(Diagnostic::new(PropertyWithParameters, stmt.identifier()));
    }
}
